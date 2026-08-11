//! Daemon lifecycle, theme/CSS loading, page publication, and fast commands.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::adapters::{ProductionClock, ProductionIo};
use crate::cli::{PageDirection as CliPageDirection, RenderPage};
use crate::config::Config;
use crate::domain::boundary::{ClockSnapshot, CommandRunner, FilesystemRoots};
#[cfg(test)]
use crate::domain::boundary::{DbusFacade, IoEvent, NotificationFacade};
use crate::domain::readings::{DisplaySnapshot, HardwareInventory};
use crate::error::{Error, Result};
use crate::page_commands::{
    CommandLookup, Page, PageCommandCache, PageCommandContext, PageEnvironment, PageRenderKind,
    PageSource, build_pages, default_click, page_inner_cached, page_inner_with_clock, pager_html,
    title_html,
};
use crate::render::{PageFormatter, PanelFormatter};
use crate::runtime::{self, atomic::write_atomic as write_atomic_bytes};
#[cfg(test)]
use crate::sensors::gpu_nvidia::NvmlFacade;
#[cfg(test)]
use crate::sensors::power::BoltBatteryFacade;
#[cfg(test)]
use crate::sensors::process::ProcessState;
#[cfg(test)]
use crate::sensors::process::read_top_process_page;
mod async_loop;
#[cfg(test)]
mod scheduled_loop;
#[cfg(test)]
use crate::sensors::gpu_nvidia;
#[cfg(test)]
use std::thread;

#[cfg(test)]
use crate::domain::state::NotificationState;
#[cfg(test)]
use crate::notify::check_and_notify;
#[cfg(test)]
use crate::sensors::{
    CollectCtx, OwnerRefs, collect_with_notifications, cpu, disk, external, gpu_history, gpu_intel,
    memory, network, power,
};

/// Page counter check cadence while sleeping between polls.
pub const PAGE_WAKE_INTERVAL: Duration = Duration::from_millis(100);
/// Bounded startup readiness logging window.
pub const BOOT_WATCH_WINDOW: Duration = Duration::from_secs(90);

/// Paths owned by one daemon instance. Tests construct these under a temp root.
#[derive(Debug, Clone)]
pub struct DaemonPaths {
    /// Watched runtime root.
    pub runtime: PathBuf,
    /// Runtime state subtree.
    pub state: PathBuf,
    /// Panel publication target.
    pub panel: PathBuf,
    /// Tooltip publication target.
    pub tooltip: PathBuf,
    /// Page counter.
    pub page: PathBuf,
    /// Published page count.
    pub npages: PathBuf,
    /// Geometry publication.
    pub geom: PathBuf,
    /// Plasma applet configuration.
    pub plasma_config: PathBuf,
    /// KDE color scheme configuration.
    pub kdeglobals: PathBuf,
}

impl DaemonPaths {
    /// Resolves production paths from XDG/home/runtime contracts.
    #[must_use]
    pub fn production() -> Self {
        let home = std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
        Self {
            runtime: runtime::runtime_dir(),
            state: runtime::state_dir(),
            panel: runtime::panel_file(),
            tooltip: runtime::tooltip_file(),
            page: runtime::page_file(),
            npages: runtime::npages_file(),
            geom: runtime::geom_file(),
            plasma_config: home
                .join(".config")
                .join("plasma-org.kde.plasma.desktop-appletsrc"),
            kdeglobals: home.join(".config").join("kdeglobals"),
        }
    }
}

/// Clock/sleep/shutdown seam used by deterministic daemon tests.
#[cfg(test)]
pub trait LoopControl {
    /// Samples current clocks.
    fn snapshot(&mut self) -> ClockSnapshot;
    /// Sleeps or advances a fake clock.
    fn sleep(&mut self, duration: Duration);
    /// Whether shutdown was requested.
    fn should_stop(&self) -> bool;
    /// Drains async service signals available at this scheduler wake.
    fn drain_io_events(&mut self) -> Vec<IoEvent> {
        Vec::new()
    }
}

/// Production/test boundary bundle retained across daemon polls.
#[cfg(test)]
pub struct DaemonBoundaries<'a> {
    /// Subprocess adapter.
    pub commands: &'a mut dyn CommandRunner,
    /// System/session D-Bus adapter.
    pub dbus: &'a mut dyn DbusFacade,
    /// Desktop notifications.
    pub notifications: &'a mut dyn NotificationFacade,
    /// Optional runtime-loaded NVML adapter.
    pub nvml: Option<&'a mut (dyn NvmlFacade + 'a)>,
    /// Optional Bolt HID adapter.
    pub bolt: Option<&'a mut (dyn BoltBatteryFacade + 'a)>,
}

/// Parses KDE's `r,g,b` value. Extra alpha fields are ignored.
#[must_use]
pub fn parse_rgb(text: &str) -> Option<(i32, i32, i32)> {
    let mut parts = text.split(',');
    Some((
        parts.next()?.trim().parse().ok()?,
        parts.next()?.trim().parse().ok()?,
        parts.next()?.trim().parse().ok()?,
    ))
}

/// Returns whether an RGB background selects the light stylesheet.
#[must_use]
pub fn is_light_rgb(rgb: (i32, i32, i32)) -> bool {
    let (r, g, b) = rgb;
    0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b) > 127.5
}

fn kdeglobals_background(path: &Path) -> Option<(i32, i32, i32)> {
    let text = fs::read_to_string(path).ok()?;
    let mut in_window = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_window = line == "[Colors:Window]";
        } else if in_window && let Some(value) = line.strip_prefix("BackgroundNormal=") {
            return parse_rgb(value);
        }
    }
    None
}

pub(crate) fn plasma_is_light(commands: &mut dyn CommandRunner, kdeglobals: &Path) -> bool {
    let args = [
        OsString::from("--file"),
        OsString::from("kdeglobals"),
        OsString::from("--group"),
        OsString::from("Colors:Window"),
        OsString::from("--key"),
        OsString::from("BackgroundNormal"),
    ];
    let from_command = commands
        .run(Path::new("kreadconfig6"), &args, Duration::from_secs(2))
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| parse_rgb(text.trim()));
    from_command
        .or_else(|| kdeglobals_background(kdeglobals))
        .is_some_and(is_light_rgb)
}

/// Reads one CSS file, removing comments and collapsing whitespace for Qt.
#[must_use]
pub fn read_css_file(path: &Path) -> String {
    let Ok(text) = fs::read_to_string(path) else {
        return String::new();
    };
    let mut stripped = String::with_capacity(text.len());
    let mut rest = text.as_str();
    while let Some(start) = rest.find("/*") {
        stripped.push_str(&rest[..start]);
        let Some(end) = rest[start + 2..].find("*/") else {
            rest = "";
            break;
        };
        rest = &rest[start + 2 + end + 2..];
    }
    stripped.push_str(rest);
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Loads base CSS and optional inspection overlay.
#[must_use]
pub fn read_css(base: &Path, overlay: Option<&Path>) -> String {
    let mut css = read_css_file(base);
    if let Some(overlay) = overlay {
        let extra = read_css_file(overlay);
        if !extra.is_empty() {
            if !css.is_empty() {
                css.push(' ');
            }
            css.push_str(&extra);
        }
    }
    css
}

fn mtime(path: &Path) -> Option<SystemTime> {
    path.metadata().and_then(|meta| meta.modified()).ok()
}

fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    write_atomic_bytes(path, content.as_bytes())
}

fn page_index(paths: &DaemonPaths, count: usize) -> usize {
    let raw = fs::read_to_string(&paths.page)
        .ok()
        .and_then(|text| text.trim().parse::<i64>().ok())
        .unwrap_or(0);
    usize::try_from(raw.rem_euclid(i64::try_from(count.max(1)).unwrap_or(i64::MAX))).unwrap_or(0)
}

fn publish_pages(paths: &DaemonPaths, cfg: &Config) -> Result<Vec<Page>> {
    let pages = build_pages(&cfg.pages.order);
    write_atomic(&paths.npages, &pages.len().to_string())?;
    Ok(pages)
}

#[cfg(test)]
fn replace_page_registry(
    paths: &DaemonPaths,
    cfg: &Config,
    active: &mut Vec<Page>,
    command_cache: &mut PageCommandCache,
    process: &mut ProcessState,
) -> Result<()> {
    let pages = publish_pages(paths, cfg)?;
    command_cache.retain_pages(&pages);
    let removed_process_page = active
        .iter()
        .any(|page| page.render() == Some(PageRenderKind::TopProcess))
        && !pages
            .iter()
            .any(|page| page.render() == Some(PageRenderKind::TopProcess));
    if removed_process_page {
        process.reset_page();
    }
    *active = pages;
    Ok(())
}

pub(crate) fn executable_lookup() -> CommandLookup {
    let mut lookup = CommandLookup::new();
    for name in ["ss", "fastfetch", "script"] {
        if let Some(path) = find_executable(name) {
            lookup.insert(name, path);
        }
    }
    lookup
}

fn find_executable(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_page_with_clock(
    cfg: &Config,
    hw: &HardwareInventory,
    readings: &DisplaySnapshot,
    css: &str,
    active: &[Page],
    index: usize,
    runner: &mut dyn CommandRunner,
    command_lookup: &CommandLookup,
    command_cache: &mut PageCommandCache,
    cadence_at: Duration,
    proc_root: &Path,
    capture_clock: &mut impl FnMut() -> Duration,
) -> String {
    let index = index % active.len().max(1);
    if index == 0 {
        let main = PanelFormatter::new(cfg, hw).format_tooltip(readings, css);
        let pager = pager_html(0, cfg.display.tooltip_width.max(0) as usize, active.len());
        return insert_before_tooltip_close(main, &pager);
    }
    let page = active[index];
    let header = title_html(&page);
    let formatter = PageFormatter::new(cfg, hw);
    let pager = |width| pager_html(index, width, active.len());
    match page.source {
        PageSource::Render(PageRenderKind::CpuCores) => {
            formatter.format_cpu_cores(readings, css, &header, Some(&pager))
        }
        PageSource::Render(PageRenderKind::TopProcess) => {
            formatter.format_top_process(readings, css, &header, Some(&pager))
        }
        PageSource::Render(PageRenderKind::Graphs) => {
            formatter.format_graphs(readings, css, &header, Some(&pager))
        }
        PageSource::Command(_) => {
            let environment = PageEnvironment {
                proc_root: proc_root.to_path_buf(),
                services_text: None,
            };
            let inner = page_inner_with_clock(
                &page,
                index,
                active.len(),
                cfg.display.tooltip_width.max(0) as usize,
                &mut DynRunner(runner),
                PageCommandContext {
                    commands: command_lookup,
                    cache: command_cache,
                    now: cadence_at,
                    environment: &environment,
                },
                capture_clock,
            );
            formatter.format_page(&inner, css, &header, "")
        }
        PageSource::Full => PanelFormatter::new(cfg, hw).format_tooltip(readings, css),
    }
}

#[allow(clippy::too_many_arguments)]
fn render_page_cached(
    cfg: &Config,
    hw: &HardwareInventory,
    readings: &DisplaySnapshot,
    css: &str,
    active: &[Page],
    index: usize,
    command_cache: &PageCommandCache,
    proc_root: &Path,
) -> String {
    let index = index % active.len().max(1);
    if index == 0 {
        let main = PanelFormatter::new(cfg, hw).format_tooltip(readings, css);
        let pager = pager_html(0, cfg.display.tooltip_width.max(0) as usize, active.len());
        return insert_before_tooltip_close(main, &pager);
    }
    let page = active[index];
    let header = title_html(&page);
    let formatter = PageFormatter::new(cfg, hw);
    let pager = |width| pager_html(index, width, active.len());
    match page.source {
        PageSource::Render(PageRenderKind::CpuCores) => {
            formatter.format_cpu_cores(readings, css, &header, Some(&pager))
        }
        PageSource::Render(PageRenderKind::TopProcess) => {
            formatter.format_top_process(readings, css, &header, Some(&pager))
        }
        PageSource::Render(PageRenderKind::Graphs) => {
            formatter.format_graphs(readings, css, &header, Some(&pager))
        }
        PageSource::Command(_) => {
            let environment = PageEnvironment {
                proc_root: proc_root.to_path_buf(),
                services_text: None,
            };
            let inner = page_inner_cached(
                &page,
                index,
                active.len(),
                cfg.display.tooltip_width.max(0) as usize,
                command_cache,
                &environment,
            );
            formatter.format_page(&inner, css, &header, "")
        }
        PageSource::Full => PanelFormatter::new(cfg, hw).format_tooltip(readings, css),
    }
}

struct DynRunner<'a>(&'a mut dyn CommandRunner);

impl CommandRunner for DynRunner<'_> {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        timeout: Duration,
    ) -> std::result::Result<
        crate::domain::boundary::CommandOutput,
        crate::domain::boundary::BoundaryError,
    > {
        self.0.run(program, args, timeout)
    }
}

fn insert_before_tooltip_close(mut html: String, footer: &str) -> String {
    if footer.is_empty() {
        return html;
    }
    if let Some(index) = html.rfind("</div>") {
        html.insert_str(index, footer);
    } else {
        html.push_str(footer);
    }
    html
}

fn cleanup(paths: &DaemonPaths) {
    for path in [&paths.panel, &paths.tooltip, &paths.page, &paths.npages] {
        let _ = fs::remove_file(path);
    }
}

fn clock_unix(snapshot: ClockSnapshot) -> u64 {
    snapshot
        .wall
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

fn log_boot_ready(
    readings: &DisplaySnapshot,
    pending: &mut BTreeSet<&'static str>,
    boot: Duration,
    now: Duration,
) {
    if pending.is_empty() {
        return;
    }
    let elapsed = now.saturating_sub(boot);
    if elapsed > BOOT_WATCH_WINDOW {
        pending.clear();
        return;
    }
    let ready = [
        ("battery_sys", !readings.battery_sys.is_empty()),
        ("battery_mouse", readings.battery_mouse.is_some()),
        ("battery_kbd", readings.battery_kbd.is_some()),
        ("hd_temps", readings.hd_temps.values().any(Option::is_some)),
        (
            "fan_speeds",
            readings.fan_speeds.values().any(Option::is_some),
        ),
        ("gpu_nvidia", readings.gpu_temp.is_some()),
        ("gpu_intel", readings.gpu_intel_freq.is_some()),
        ("system_updates", readings.system_updates.is_some()),
        ("server_check", readings.server_ok.is_some()),
        ("top_process", readings.top_process.is_some()),
    ];
    for (name, is_ready) in ready {
        if is_ready && pending.remove(name) {
            println!("[boot] {name} ready at +{:.2}s", elapsed.as_secs_f64());
        }
    }
}

/// Merges one selected process-page sample and stamps the completed display snapshot afterward.
#[cfg(test)]
pub(crate) fn merge_process_page_sample(
    readings: &mut DisplaySnapshot,
    proc_root: &Path,
    process: &mut ProcessState,
    clock: &mut dyn FnMut() -> ClockSnapshot,
) {
    let captured_at = clock();
    readings.top_process_full = read_top_process_page(proc_root, process, captured_at);
    readings.assembled_at = clock();
}

/// Runs daemon against explicit roots and adapters. `poll_limit` bounds post-first-paint scheduled display publications in tests; production passes `None`.
#[cfg(test)]
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn run_daemon_with(
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    poll_limit: Option<usize>,
) -> Result<()> {
    scheduled_loop::run(config_path, roots, paths, boundaries, control, poll_limit)
}

#[cfg(test)]
struct DynNotification<'a>(&'a mut dyn NotificationFacade);

#[cfg(test)]
impl NotificationFacade for DynNotification<'_> {
    fn send(
        &mut self,
        payload: &crate::domain::boundary::NotificationPayload,
    ) -> std::result::Result<(), crate::domain::boundary::NotificationError> {
        self.0.send(payload)
    }
}

/// Runs production daemon until SIGINT/SIGTERM.
pub fn run_daemon(config_path: Option<&Path>) -> Result<()> {
    let clock = ProductionClock::default();
    let mut io = ProductionIo::start()?;
    let runtime = io.runtime_handle();
    let commands = io.commands();
    let dbus = io.dbus();
    let notifications = io.notifications();
    let stopped = io.stopped();
    let shutdown = io.shutdown_receiver();
    let orchestration_done = io.orchestration_done();
    let events = io.take_event_receiver().ok_or_else(|| {
        Error::Runtime("async I/O event receiver was already transferred".to_owned())
    })?;
    let roots = FilesystemRoots::default();
    let paths = DaemonPaths::production();
    let config_path = config_path.map(Path::to_path_buf);
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
    runtime.spawn(async move {
        let result = async_loop::run(
            config_path.as_deref(),
            &roots,
            &paths,
            async_loop::DaemonServices {
                commands,
                dbus,
                notifications,
                stopped,
                events,
                clock,
                shutdown,
                #[cfg(test)]
                blocked_owner: None,
            },
        )
        .await;
        let _ = orchestration_done.send(true);
        let _ = result_sender.send(result);
    });
    let result = match result_receiver.recv() {
        Ok(result) => result,
        Err(_) if io.critical_failure().is_some() => Ok(()),
        Err(_) => Err(Error::Runtime(
            "critical async daemon orchestration task exited unexpectedly".to_owned(),
        )),
    };
    io.shutdown();
    complete_daemon_run(result, io.critical_failure(), io.shutdown_timed_out())
}

fn complete_daemon_run(
    result: Result<()>,
    critical_failure: Option<crate::error::CriticalService>,
    shutdown_timed_out: bool,
) -> Result<()> {
    result?;
    if let Some(service) = critical_failure {
        return Err(Error::CriticalService(service));
    }
    if shutdown_timed_out {
        return Err(Error::CriticalShutdownTimeout);
    }
    Ok(())
}

/// Fast page-counter command.
pub fn run_page(direction: CliPageDirection) -> Result<()> {
    let direction = match direction {
        CliPageDirection::Next => runtime::page::PageDirection::Next,
        CliPageDirection::Prev => runtime::page::PageDirection::Prev,
    };
    runtime::ensure_dirs()?;
    runtime::page::step_page(direction)?;
    Ok(())
}

/// Launches current default click action detached from the CLI process.
pub fn run_click() -> Result<()> {
    let Some((program, args)) = default_click().split_first() else {
        return Ok(());
    };
    ProcessCommand::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            Error::Runtime(format!(
                "[click] failed to launch {:?}: {error}",
                default_click()
            ))
        })
}

/// Converts CLI render page to stable page id.
#[must_use]
pub const fn render_page_id(page: RenderPage) -> &'static str {
    match page {
        RenderPage::Full => "full",
        RenderPage::Processes => "processes",
        RenderPage::CpuCores => "cpu_cores",
        RenderPage::Connections => "connections",
        RenderPage::Fastfetch => "fastfetch",
        RenderPage::Graphs => "graphs",
    }
}

#[cfg(test)]
mod tests;
