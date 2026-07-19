//! Daemon lifecycle, theme/CSS loading, page publication, and fast commands.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command as ProcessCommand, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use signal_hook::consts::signal::{SIGINT, SIGTERM};
use signal_hook::flag;

use crate::adapters::{
    ProductionClock, ProductionCommandRunner, ProductionDbusFacade, ProductionNotificationFacade,
};
use crate::cli::{PageDirection as CliPageDirection, RenderPage};
use crate::config::{
    Config, apply_canonical_width, cache_live_geom, default_config_path, load_config,
    machine_source_paths, resolve_style,
};
use crate::domain::boundary::{
    ClockSnapshot, CommandRunner, DbusFacade, FilesystemRoots, NotificationFacade,
};
use crate::domain::readings::{HardwareSnapshot, ReadingsSnapshot};
use crate::domain::state::{DaemonStateSnapshot, NotificationState};
use crate::error::{Error, Result};
use crate::notify::check_and_notify;
use crate::page_commands::{
    CommandLookup, Page, PageCommandCache, PageCommandContext, PageEnvironment, PageRenderKind,
    PageSource, build_pages, default_click, page_inner, pager_html, title_html,
};
use crate::render::{PageFormatter, PanelFormatter};
use crate::runtime::{self, atomic::write_atomic as write_atomic_bytes};
use crate::sensors::gpu_nvidia::NvmlFacade;
use crate::sensors::hid::BoltHidFacade;
use crate::sensors::power::BoltBatteryFacade;
use crate::sensors::process::read_top_process_page;
use crate::sensors::{
    CollectCtx, CollectorState, collect, discover_hardware, needs_periph_rescan, rescan_peripherals,
};

/// Periodic retry cadence for peripherals absent at startup.
pub const PERIPH_RESCAN_INTERVAL: Duration = Duration::from_secs(60);
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
pub trait LoopControl {
    /// Samples current clocks.
    fn snapshot(&mut self) -> ClockSnapshot;
    /// Sleeps or advances a fake clock.
    fn sleep(&mut self, duration: Duration);
    /// Whether shutdown was requested.
    fn should_stop(&self) -> bool;
}

struct ProductionLoopControl {
    clock: ProductionClock,
    stopped: Arc<AtomicBool>,
}

impl LoopControl for ProductionLoopControl {
    fn snapshot(&mut self) -> ClockSnapshot {
        self.clock.snapshot()
    }

    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }

    fn should_stop(&self) -> bool {
        self.stopped.load(Ordering::Relaxed)
    }
}

/// Production/test boundary bundle retained across daemon polls.
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
pub(crate) fn render_page(
    cfg: &Config,
    hw: &HardwareSnapshot,
    readings: &ReadingsSnapshot,
    css: &str,
    active: &[Page],
    index: usize,
    runner: &mut dyn CommandRunner,
    command_lookup: &CommandLookup,
    command_cache: &mut PageCommandCache,
    now: Duration,
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
            let inner = page_inner(
                &page,
                index,
                active.len(),
                cfg.display.tooltip_width.max(0) as usize,
                &mut DynRunner(runner),
                PageCommandContext {
                    commands: command_lookup,
                    cache: command_cache,
                    now,
                    environment: &environment,
                },
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
    readings: &ReadingsSnapshot,
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

/// Runs daemon against explicit roots and adapters. `poll_limit` bounds tests;
/// production passes `None`.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn run_daemon_with(
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    poll_limit: Option<usize>,
) -> Result<()> {
    fs::create_dir_all(&paths.runtime)?;
    fs::create_dir_all(&paths.state)?;
    cleanup(paths);
    write_atomic(&paths.page, "0")?;

    let boot = control.snapshot();
    let mut cfg = load_config(config_path, None)?;
    let cpu_count = thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut hw = discover_hardware(
        &roots.sys_root,
        &roots.proc_root,
        &cfg,
        boundaries.dbus,
        boundaries.commands,
        cpu_count,
    );
    let mut active = publish_pages(paths, &cfg)?;
    let mut state = DaemonStateSnapshot::default();
    let mut collector = CollectorState::default();
    let mut notifications = NotificationState::default();
    let mut command_cache = PageCommandCache::new();
    let command_lookup = executable_lookup();
    let mut boot_pending = BTreeSet::from([
        "battery_sys",
        "battery_mouse",
        "battery_kbd",
        "hd_temps",
        "fan_speeds",
        "gpu_nvidia",
        "gpu_intel",
        "system_updates",
        "server_check",
        "top_process",
    ]);

    let watch_path = config_path.map_or_else(default_config_path, Path::to_path_buf);
    let machine_paths = machine_source_paths(config_path);
    let mut config_stamp = mtime(&watch_path);
    let mut machine_stamps = machine_paths
        .iter()
        .map(|path| mtime(path))
        .collect::<Vec<_>>();
    let mut plasma_stamp = mtime(&paths.plasma_config);
    let mut geom_stamp = mtime(&paths.geom);
    let mut kde_stamp = mtime(&paths.kdeglobals);

    let mut light = plasma_is_light(boundaries.commands, &paths.kdeglobals);
    let mut css_path = resolve_style(if light {
        "style-light.css"
    } else {
        "style-dark.css"
    });
    let mut overlay_path = cfg
        .display
        .overlay
        .then(|| resolve_style("style-overlay.css"));
    let mut css_stamp = mtime(&css_path);
    let mut overlay_stamp = overlay_path.as_deref().and_then(mtime);
    let mut css = read_css(&css_path, overlay_path.as_deref());

    let first_clock = control.snapshot();
    let mut first_ctx = CollectCtx::new(
        roots,
        &mut *boundaries.commands,
        &mut *boundaries.dbus,
        first_clock,
    );
    first_ctx.skip_slow = true;
    first_ctx.nvml = boundaries.nvml.take();
    first_ctx.bolt = boundaries.bolt.take();
    let first = collect(
        &mut collector,
        &mut state,
        &mut hw,
        &cfg,
        &mut first_ctx,
        None,
    );
    boundaries.nvml = first_ctx.nvml.take();
    boundaries.bolt = first_ctx.bolt.take();
    let first_width = PanelFormatter::new(&cfg, &hw).canonical_width(&first);
    apply_canonical_width(&mut cfg, i32::try_from(first_width).unwrap_or(i32::MAX));
    write_atomic(
        &paths.panel,
        &PanelFormatter::with_now_unix(&cfg, &hw, clock_unix(first_clock))
            .format_panel(&first, &css),
    )?;
    let first_index = page_index(paths, active.len());
    let tooltip = render_page(
        &cfg,
        &hw,
        &first,
        &css,
        &active,
        first_index,
        boundaries.commands,
        &command_lookup,
        &mut command_cache,
        first_clock.monotonic,
        &roots.proc_root,
    );
    write_atomic(&paths.tooltip, &tooltip)?;
    println!(
        "[boot] first paint at +{:.0}ms",
        first_clock
            .monotonic
            .saturating_sub(boot.monotonic)
            .as_secs_f64()
            * 1000.0
    );
    log_boot_ready(
        &first,
        &mut boot_pending,
        boot.monotonic,
        first_clock.monotonic,
    );
    cache_live_geom();

    let mut polls = 0usize;
    while !control.should_stop() && poll_limit.is_none_or(|limit| polls < limit) {
        let start = control.snapshot();
        let new_config_stamp = mtime(&watch_path);
        let new_machine_stamps = machine_paths
            .iter()
            .map(|path| mtime(path))
            .collect::<Vec<_>>();
        if new_config_stamp != config_stamp || new_machine_stamps != machine_stamps {
            config_stamp = new_config_stamp;
            machine_stamps = new_machine_stamps;
            match load_config(config_path, None) {
                Ok(new_cfg) => {
                    let new_hw = discover_hardware(
                        &roots.sys_root,
                        &roots.proc_root,
                        &new_cfg,
                        boundaries.dbus,
                        boundaries.commands,
                        cpu_count,
                    );
                    cfg = new_cfg;
                    hw = new_hw;
                    active = publish_pages(paths, &cfg)?;
                }
                Err(error) => eprintln!("[reload] config reload failed, keeping previous: {error}"),
            }
        }

        let new_plasma_stamp = mtime(&paths.plasma_config);
        let new_geom_stamp = mtime(&paths.geom);
        if new_plasma_stamp != plasma_stamp || new_geom_stamp != geom_stamp {
            plasma_stamp = new_plasma_stamp;
            geom_stamp = new_geom_stamp;
            cache_live_geom();
            match load_config(config_path, None) {
                Ok(new_cfg) => {
                    cfg = new_cfg;
                    active = publish_pages(paths, &cfg)?;
                }
                Err(error) => {
                    eprintln!("[reload] plasma-triggered reload failed, keeping previous: {error}")
                }
            }
        }

        let new_kde_stamp = mtime(&paths.kdeglobals);
        if new_kde_stamp != kde_stamp {
            kde_stamp = new_kde_stamp;
            let new_light = plasma_is_light(boundaries.commands, &paths.kdeglobals);
            if new_light != light {
                light = new_light;
                css_path = resolve_style(if light {
                    "style-light.css"
                } else {
                    "style-dark.css"
                });
                css_stamp = mtime(&css_path);
                css = read_css(&css_path, overlay_path.as_deref());
            }
        }
        let wanted_overlay = cfg
            .display
            .overlay
            .then(|| resolve_style("style-overlay.css"));
        if wanted_overlay != overlay_path {
            overlay_path = wanted_overlay;
            overlay_stamp = overlay_path.as_deref().and_then(mtime);
            css = read_css(&css_path, overlay_path.as_deref());
        }
        let new_css_stamp = mtime(&css_path);
        let new_overlay_stamp = overlay_path.as_deref().and_then(mtime);
        if new_css_stamp != css_stamp || new_overlay_stamp != overlay_stamp {
            css_stamp = new_css_stamp;
            overlay_stamp = new_overlay_stamp;
            css = read_css(&css_path, overlay_path.as_deref());
        }

        if needs_periph_rescan(&hw, &cfg)
            && hw
                .periph_scan_at
                .is_none_or(|last| start.monotonic.saturating_sub(last) >= PERIPH_RESCAN_INTERVAL)
        {
            rescan_peripherals(&mut hw, &cfg, boundaries.dbus, boundaries.commands, start);
        }

        let mut ctx = CollectCtx::new(
            roots,
            &mut *boundaries.commands,
            &mut *boundaries.dbus,
            start,
        );
        ctx.nvml = boundaries.nvml.take();
        ctx.bolt = boundaries.bolt.take();
        let mut readings = collect(&mut collector, &mut state, &mut hw, &cfg, &mut ctx, None);
        boundaries.nvml = ctx.nvml.take();
        boundaries.bolt = ctx.bolt.take();
        let canonical_width = PanelFormatter::new(&cfg, &hw).canonical_width(&readings);
        apply_canonical_width(&mut cfg, i32::try_from(canonical_width).unwrap_or(i32::MAX));
        let report = check_and_notify(
            &readings,
            &cfg,
            &mut notifications,
            &hw,
            start.monotonic,
            &mut DynNotification(boundaries.notifications),
        );
        for failure in report.failures {
            eprintln!("[notify] {}", failure.error);
        }
        log_boot_ready(
            &readings,
            &mut boot_pending,
            boot.monotonic,
            start.monotonic,
        );

        let index = page_index(paths, active.len());
        if active[index].render() == Some(PageRenderKind::TopProcess) {
            readings.top_process_full =
                read_top_process_page(&roots.proc_root, &mut collector.process, start)
                    .or(readings.top_process_full);
        }
        write_atomic(
            &paths.panel,
            &PanelFormatter::with_now_unix(&cfg, &hw, clock_unix(start))
                .format_panel(&readings, &css),
        )?;
        write_atomic(
            &paths.tooltip,
            &render_page(
                &cfg,
                &hw,
                &readings,
                &css,
                &active,
                index,
                boundaries.commands,
                &command_lookup,
                &mut command_cache,
                start.monotonic,
                &roots.proc_root,
            ),
        )?;

        let interval = Duration::from_secs_f64(cfg.display.poll_interval.max(0.0));
        let mut remaining =
            interval.saturating_sub(control.snapshot().monotonic.saturating_sub(start.monotonic));
        let mut last_page = page_index(paths, active.len());
        while !remaining.is_zero() && !control.should_stop() {
            let step = remaining.min(PAGE_WAKE_INTERVAL);
            control.sleep(step);
            remaining = remaining.saturating_sub(step);
            let page = page_index(paths, active.len());
            if page != last_page {
                last_page = page;
                if active[page].render() == Some(PageRenderKind::TopProcess) {
                    let now = control.snapshot();
                    readings.top_process_full =
                        read_top_process_page(&roots.proc_root, &mut collector.process, now)
                            .or(readings.top_process_full);
                }
                let now = control.snapshot();
                write_atomic(
                    &paths.tooltip,
                    &render_page(
                        &cfg,
                        &hw,
                        &readings,
                        &css,
                        &active,
                        page,
                        boundaries.commands,
                        &command_lookup,
                        &mut command_cache,
                        now.monotonic,
                        &roots.proc_root,
                    ),
                )?;
            }
        }
        polls = polls.saturating_add(1);
    }
    cleanup(paths);
    Ok(())
}

struct DynNotification<'a>(&'a mut dyn NotificationFacade);

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
    let stopped = Arc::new(AtomicBool::new(false));
    flag::register(SIGTERM, Arc::clone(&stopped))
        .map_err(|error| Error::Runtime(format!("cannot register SIGTERM: {error}")))?;
    flag::register(SIGINT, Arc::clone(&stopped))
        .map_err(|error| Error::Runtime(format!("cannot register SIGINT: {error}")))?;
    let roots = FilesystemRoots::default();
    let paths = DaemonPaths::production();
    let mut commands = ProductionCommandRunner;
    let mut dbus = ProductionDbusFacade::default();
    let mut notifications = ProductionNotificationFacade::default();
    let mut bolt = BoltHidFacade::default();
    #[cfg(feature = "nvml")]
    let mut nvml = crate::sensors::gpu_nvidia::ProductionNvml::new();
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        #[cfg(feature = "nvml")]
        nvml: Some(&mut nvml),
        #[cfg(not(feature = "nvml"))]
        nvml: None,
        bolt: Some(&mut bolt),
    };
    let mut control = ProductionLoopControl {
        clock: ProductionClock::default(),
        stopped,
    };
    run_daemon_with(
        config_path,
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        None,
    )
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
mod tests {
    #![allow(clippy::expect_used)]
    use super::*;
    use crate::domain::boundary::{
        BoundaryError, CommandOutput, DbusOutput, DbusRequest, NotificationError,
        NotificationPayload,
    };
    use std::cell::Cell;

    #[test]
    fn rgb_and_luma_match_python_boundaries() {
        assert_eq!(parse_rgb("1, 2,3,255"), Some((1, 2, 3)));
        assert_eq!(parse_rgb("bad"), None);
        assert!(!is_light_rgb((127, 127, 127)));
        assert!(is_light_rgb((255, 255, 255)));
    }

    #[test]
    fn css_comments_and_whitespace_are_stripped() {
        let dir = std::env::temp_dir().join(format!("pirostats-css-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let path = dir.join("style.css");
        fs::write(&path, "/* note */\n.a {\n color: red;\n}").expect("write fixture");
        assert_eq!(read_css_file(&path), ".a { color: red; }");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn footer_is_inserted_inside_tooltip_root() {
        assert_eq!(
            insert_before_tooltip_close("<div class=\"tooltip\">x</div>".into(), "p"),
            "<div class=\"tooltip\">xp</div>"
        );
    }

    struct AbsentCommand {
        calls: Cell<usize>,
    }

    impl CommandRunner for AbsentCommand {
        fn run(
            &mut self,
            program: &Path,
            args: &[OsString],
            _timeout: Duration,
        ) -> std::result::Result<CommandOutput, BoundaryError> {
            self.calls.set(self.calls.get().saturating_add(1));
            Err(BoundaryError::CommandFailed {
                program: program.to_path_buf(),
                args: args.to_vec(),
                detail: "fixture absent".into(),
            })
        }
    }

    struct AbsentDbus;

    impl DbusFacade for AbsentDbus {
        fn call(&mut self, request: DbusRequest) -> std::result::Result<DbusOutput, BoundaryError> {
            Err(BoundaryError::DbusCallFailed {
                bus: request.bus,
                service: request.service,
                path: request.object_path,
                interface: request.interface,
                member: request.member,
                detail: "fixture absent".into(),
            })
        }
    }

    struct RecordingNotifications;

    impl NotificationFacade for RecordingNotifications {
        fn send(
            &mut self,
            _payload: &NotificationPayload,
        ) -> std::result::Result<(), NotificationError> {
            Ok(())
        }
    }

    struct FakeControl {
        now: Duration,
        paths: DaemonPaths,
        config: PathBuf,
        sleeps: usize,
        saw_panel: bool,
        saw_tooltip: bool,
        saw_page_one: bool,
    }

    impl LoopControl for FakeControl {
        fn snapshot(&mut self) -> ClockSnapshot {
            ClockSnapshot {
                monotonic: self.now,
                wall: UNIX_EPOCH + self.now,
            }
        }

        fn sleep(&mut self, duration: Duration) {
            self.now = self.now.saturating_add(duration);
            self.sleeps = self.sleeps.saturating_add(1);
            self.saw_panel |= fs::read_to_string(&self.paths.panel)
                .is_ok_and(|html| html.contains("class=\"panel"));
            self.saw_tooltip |= fs::read_to_string(&self.paths.tooltip)
                .is_ok_and(|html| html.contains("class=\"tooltip"));
            if self.sleeps == 1 {
                fs::write(&self.paths.page, "1").expect("step fixture page");
                fs::write(&self.config, "not = [valid").expect("break hot config");
            } else if fs::read_to_string(&self.paths.tooltip)
                .is_ok_and(|html| html.contains("CPU CORES"))
            {
                self.saw_page_one = true;
            }
        }

        fn should_stop(&self) -> bool {
            false
        }
    }

    fn integration_tree() -> (PathBuf, FilesystemRoots, DaemonPaths, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "pirostats-daemon-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        let proc_root = root.join("proc");
        let sys_root = root.join("sys");
        fs::create_dir_all(&proc_root).expect("proc fixture root");
        fs::create_dir_all(&sys_root).expect("sys fixture root");
        fs::write(proc_root.join("stat"), "cpu  1 0 1 8 0 0 0 0\n").expect("proc stat");
        fs::write(
            proc_root.join("meminfo"),
            "MemTotal: 1024000 kB\nMemAvailable: 512000 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n",
        )
        .expect("meminfo");
        let runtime = root.join("run/pirostats");
        let state = runtime.join("state");
        let paths = DaemonPaths {
            runtime: runtime.clone(),
            state: state.clone(),
            panel: runtime.join("panel.html"),
            tooltip: runtime.join("tooltip.html"),
            page: state.join("page"),
            npages: state.join("npages"),
            geom: state.join("geom"),
            plasma_config: root.join("appletsrc"),
            kdeglobals: root.join("kdeglobals"),
        };
        let config = root.join("config.toml");
        fs::write(
            &config,
            "[display]\npoll_interval = 0.25\n[panel]\norder = []\n[tooltip]\norder = []\n[pages]\norder = [\"cpu_cores\"]\n",
        )
        .expect("config fixture");
        let roots = FilesystemRoots {
            runtime_root: Some(runtime),
            cache_root: Some(root.join("cache")),
            config_root: Some(root.join("config")),
            proc_root,
            sys_root,
        };
        (root, roots, paths, config)
    }

    #[test]
    fn isolated_lifecycle_paints_wakes_keeps_last_good_and_cleans_up() {
        let (root, roots, paths, config) = integration_tree();
        let mut commands = AbsentCommand {
            calls: Cell::new(0),
        };
        let mut dbus = AbsentDbus;
        let mut notifications = RecordingNotifications;
        let mut boundaries = DaemonBoundaries {
            commands: &mut commands,
            dbus: &mut dbus,
            notifications: &mut notifications,
            nvml: None,
            bolt: None,
        };
        let mut control = FakeControl {
            now: Duration::ZERO,
            paths: paths.clone(),
            config: config.clone(),
            sleeps: 0,
            saw_panel: false,
            saw_tooltip: false,
            saw_page_one: false,
        };

        let result = run_daemon_with(
            Some(&config),
            &roots,
            &paths,
            &mut boundaries,
            &mut control,
            Some(2),
        );

        assert!(result.is_ok(), "{result:?}");
        assert!(control.saw_panel && control.saw_tooltip);
        assert!(control.saw_page_one, "page wake did not republish tooltip");
        assert!(
            commands.calls.get() > 0,
            "production call path not exercised"
        );
        for path in [&paths.panel, &paths.tooltip, &paths.page, &paths.npages] {
            assert!(!path.exists(), "cleanup left {}", path.display());
        }
        let _ = fs::remove_dir_all(root);
    }
}
