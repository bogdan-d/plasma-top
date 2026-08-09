//! Tooltip page registry, command execution, and command-page formatting.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::domain::boundary::{BoundaryError, CommandRunner, CommandStatus};
use crate::domain::readings::MetricSample;

const CLICK_SYSTEM_MONITOR: &[&str] = &["plasma-systemmonitor"];
const PROCESS_PAGE_ROWS: usize = 15;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(5);

const PORT_DAEMON: &[(u16, &str)] = &[
    (22, "sshd"),
    (25, "smtpd"),
    (53, "named"),
    (111, "rpcbind"),
    (139, "smbd"),
    (143, "imapd"),
    (445, "smbd"),
    (465, "smtpd"),
    (587, "smtpd"),
    (631, "cupsd"),
    (993, "imapd"),
    (995, "pop3d"),
    (3306, "mysqld"),
    (5432, "postgres"),
    (6379, "redis"),
    (11211, "memcached"),
    (27017, "mongod"),
];

const INTERPRETERS: &[&str] = &[
    "python", "python3", "python2", "node", "nodejs", "ruby", "perl", "java", "php", "sh", "bash",
    "zsh", "dash",
];

/// The built-in renderer used by a deep-dive page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRenderKind {
    /// The per-process tooltip page.
    TopProcess,
    /// The per-core CPU tooltip page.
    CpuCores,
    /// The tooltip graphs page.
    Graphs,
}

/// A semantic colorizer applied to command-page text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageColorizer {
    /// Format `ss -4tlnp` output into the tooltip's listening-sockets view.
    Connections,
}

/// One tooltip page in the configured registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page {
    /// Stable page slug.
    pub id: &'static str,
    /// Human-facing label used by the title row.
    pub label: &'static str,
    /// Body source for this page.
    pub source: PageSource,
    /// Default click action while the tooltip is on this page.
    pub click: &'static [&'static str],
}

impl Page {
    /// Returns the command specification when this page is command-backed.
    #[must_use]
    pub const fn command(self) -> Option<PageCommandSpec> {
        match self.source {
            PageSource::Command(spec) => Some(spec),
            _ => None,
        }
    }

    /// Returns the built-in renderer kind when this page is data-backed.
    #[must_use]
    pub const fn render(self) -> Option<PageRenderKind> {
        match self.source {
            PageSource::Render(kind) => Some(kind),
            _ => None,
        }
    }
}

/// Where a page's body comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSource {
    /// Page 0, the main stats tooltip.
    Full,
    /// A page rendered from command output.
    Command(PageCommandSpec),
    /// A page rendered from already-collected readings.
    Render(PageRenderKind),
}

/// Static command metadata for a command-backed page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageCommandSpec {
    /// Command argv.
    pub argv: &'static [&'static str],
    /// Freshness budget for the rendered command text sample.
    pub ttl: Duration,
    /// Maximum number of visible lines kept from the command output.
    pub max_lines: usize,
    /// Whether the command should run under `script -qec` when available.
    pub pty: bool,
    /// Optional semantic colorizer applied to the command output.
    pub colorize: Option<PageColorizer>,
}

/// Result of one cadence-free command-page execution and parsing attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageCommandAttempt {
    /// The command ran and its visible output was parsed successfully.
    Completed(String),
    /// The command could not be dispatched; the string is suitable for display.
    Unavailable(String),
}

impl PageCommandAttempt {
    fn text(&self) -> &str {
        match self {
            Self::Completed(text) | Self::Unavailable(text) => text,
        }
    }
}

/// Page-environment inputs that are deliberately fixtureable in tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageEnvironment {
    /// Procfs root used for interpreter-to-script resolution in connections.
    pub proc_root: PathBuf,
    /// Optional `/etc/services` text override for deterministic tests.
    pub services_text: Option<String>,
}

impl Default for PageEnvironment {
    fn default() -> Self {
        Self {
            proc_root: PathBuf::from("/proc"),
            services_text: None,
        }
    }
}

/// Executable lookup table used by page commands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommandLookup {
    programs: BTreeMap<String, PathBuf>,
}

impl CommandLookup {
    /// Creates an empty lookup.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Records one executable token → resolved path mapping.
    pub fn insert(&mut self, name: impl Into<String>, path: impl Into<PathBuf>) -> &mut Self {
        self.programs.insert(name.into(), path.into());
        self
    }

    /// Resolves a command token to the path the command runner should execute.
    #[must_use]
    pub fn resolve(&self, name: &str) -> Option<&Path> {
        self.programs.get(name).map(PathBuf::as_path)
    }
}

/// Status of the most recent command-page owner decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageCommandStatus {
    /// No command attempt has completed yet.
    #[default]
    NeverAttempted,
    /// The most recent real attempt completed successfully.
    Captured,
    /// The most recent real attempt failed.
    Failed,
    /// The compatibility wrapper reused a successful sample within its freshness budget.
    Cached,
}

/// Retained state owned by one command-backed page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PageCommandOwnerState {
    /// Latest successful parsed output, retained across failures and independent of cadence policy.
    pub latest: Option<MetricSample<String>>,
    /// Time of the most recent real attempt.
    pub attempted_at: Option<Duration>,
    /// Time of the most recent failed real attempt.
    pub failed_at: Option<Duration>,
    /// Most recent failure text retained for diagnostics.
    pub last_failure: Option<String>,
    /// Current owner status.
    pub status: PageCommandStatus,
}

impl PageCommandOwnerState {
    fn record(&mut self, attempt: &PageCommandAttempt, attempted_at: Duration) {
        self.attempted_at = Some(attempted_at);
        match attempt {
            PageCommandAttempt::Completed(text) => {
                self.latest = Some(MetricSample::new(text.clone(), attempted_at));
                self.status = PageCommandStatus::Captured;
            }
            PageCommandAttempt::Unavailable(detail) => {
                self.failed_at = Some(attempted_at);
                self.last_failure = Some(detail.clone());
                self.status = PageCommandStatus::Failed;
            }
        }
    }

    fn display_text(&self, fallback: &str) -> String {
        self.latest
            .as_ref()
            .map_or_else(|| fallback.to_owned(), |sample| sample.value.clone())
    }
}

/// Command-page owner states and synchronous freshness decisions.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PageCommandCache {
    entries: BTreeMap<&'static str, PageCommandOwnerState>,
}

/// Injected state needed while formatting one command-backed page.
pub struct PageCommandContext<'a> {
    /// Resolved executables available to page commands.
    pub commands: &'a CommandLookup,
    /// Shared command-page owner state.
    pub cache: &'a mut PageCommandCache,
    /// Fresh monotonic time used for this job's freshness decision.
    pub now: Duration,
    /// Fixtureable procfs and service-name inputs.
    pub environment: &'a PageEnvironment,
}

impl PageCommandCache {
    /// Creates empty command-page owner state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns diagnostics for one command-backed page after its first scheduling decision.
    #[must_use]
    pub fn state(&self, page_id: &str) -> Option<&PageCommandOwnerState> {
        self.entries.get(page_id)
    }

    /// Removes owner state for command pages no longer present in the registry.
    pub(crate) fn retain_pages(&mut self, pages: &[Page]) {
        self.entries.retain(|page_id, _| {
            pages
                .iter()
                .any(|page| page.id == *page_id && page.command().is_some())
        });
    }
}

/// Page 0: the main stats tooltip.
pub const FULL_PAGE: Page = Page {
    id: "full",
    label: "Full stats",
    source: PageSource::Full,
    click: CLICK_SYSTEM_MONITOR,
};

const PROCESSES_PAGE: Page = Page {
    id: "processes",
    label: "Top processes",
    source: PageSource::Render(PageRenderKind::TopProcess),
    click: CLICK_SYSTEM_MONITOR,
};

const CONNECTIONS_PAGE: Page = Page {
    id: "connections",
    label: "Connections",
    source: PageSource::Command(PageCommandSpec {
        argv: &["ss", "-4tlnp"],
        ttl: Duration::ZERO,
        max_lines: 20,
        pty: false,
        colorize: Some(PageColorizer::Connections),
    }),
    click: CLICK_SYSTEM_MONITOR,
};

const FASTFETCH_PAGE: Page = Page {
    id: "fastfetch",
    label: "System info",
    source: PageSource::Command(PageCommandSpec {
        argv: &[
            "fastfetch",
            "--logo",
            "none",
            "--structure",
            "OS:Kernel:Loadavg:Uptime:Separator:Chassis:Board:Bios:CPU:GPU:Display:BluetoothRadio:Separator:Memory:Disk:Battery:PowerAdapter:Wifi:LocalIP:DNS:Separator:InitSystem:Shell:LM:DE:WM",
        ],
        ttl: Duration::from_secs(30),
        max_lines: 0,
        pty: true,
        colorize: None,
    }),
    click: CLICK_SYSTEM_MONITOR,
};

const CPU_CORES_PAGE: Page = Page {
    id: "cpu_cores",
    label: "CPU cores",
    source: PageSource::Render(PageRenderKind::CpuCores),
    click: CLICK_SYSTEM_MONITOR,
};

const GRAPHS_PAGE: Page = Page {
    id: "graphs",
    label: "Graphs",
    source: PageSource::Render(PageRenderKind::Graphs),
    click: CLICK_SYSTEM_MONITOR,
};

const REGISTRY: &[Page] = &[
    PROCESSES_PAGE,
    CONNECTIONS_PAGE,
    FASTFETCH_PAGE,
    CPU_CORES_PAGE,
    GRAPHS_PAGE,
];

/// Returns the selectable page list: page 0 plus the configured deep-dive pages.
#[must_use]
pub fn build_pages(page_ids: &[String]) -> Vec<Page> {
    let mut pages = Vec::with_capacity(page_ids.len() + 1);
    pages.push(FULL_PAGE);
    for page_id in page_ids {
        if let Some(page) = REGISTRY.iter().copied().find(|page| page.id == page_id) {
            pages.push(page);
        }
    }
    pages
}

/// Runs a command-backed page and returns its visible text body.
#[must_use]
pub fn run_command(
    page: &Page,
    runner: &mut impl CommandRunner,
    commands: &CommandLookup,
    cache: &mut PageCommandCache,
    now: Duration,
) -> String {
    run_command_with_clock(page, runner, commands, cache, now, &mut || now)
}

/// Runs a command-backed page using separate cadence-decision and completion clocks.
#[must_use]
pub fn run_command_with_clock(
    page: &Page,
    runner: &mut impl CommandRunner,
    commands: &CommandLookup,
    cache: &mut PageCommandCache,
    cadence_at: Duration,
    capture_clock: &mut impl FnMut() -> Duration,
) -> String {
    let Some(spec) = page.command() else {
        return String::new();
    };

    if !spec.ttl.is_zero() {
        let fresh = cache
            .entries
            .get(page.id)
            .and_then(|state| state.latest.as_ref())
            .is_some_and(|sample| cadence_at.saturating_sub(sample.captured_at) < spec.ttl);
        if fresh && let Some(state) = cache.entries.get_mut(page.id) {
            state.status = PageCommandStatus::Cached;
            return state.display_text("");
        }
    }

    let attempt =
        attempt_command_with_state_and_clock(page, runner, commands, cache, capture_clock);
    cache.entries.get(page.id).map_or_else(
        || attempt.text().to_owned(),
        |state| state.display_text(attempt.text()),
    )
}

/// Performs exactly one cadence-free command-page attempt and records its owner state.
#[must_use]
pub fn attempt_command_with_state(
    page: &Page,
    runner: &mut impl CommandRunner,
    commands: &CommandLookup,
    cache: &mut PageCommandCache,
    now: Duration,
) -> PageCommandAttempt {
    attempt_command_with_state_and_clock(page, runner, commands, cache, &mut || now)
}

/// Performs one cadence-free command-page attempt and timestamps it at completion.
#[must_use]
pub fn attempt_command_with_state_and_clock(
    page: &Page,
    runner: &mut impl CommandRunner,
    commands: &CommandLookup,
    cache: &mut PageCommandCache,
    capture_clock: &mut impl FnMut() -> Duration,
) -> PageCommandAttempt {
    let attempt = attempt_command(page, runner, commands);
    let completed_at = capture_clock();
    cache
        .entries
        .entry(page.id)
        .or_default()
        .record(&attempt, completed_at);
    attempt
}

/// Performs exactly one command-page execution and parsing attempt without applying freshness policy.
#[must_use]
pub fn attempt_command(
    page: &Page,
    runner: &mut impl CommandRunner,
    commands: &CommandLookup,
) -> PageCommandAttempt {
    let Some(spec) = page.command() else {
        return PageCommandAttempt::Unavailable(String::new());
    };

    let Some((&exe, args)) = spec.argv.split_first() else {
        return PageCommandAttempt::Unavailable(String::new());
    };
    let Some(program_path) = commands.resolve(exe) else {
        return PageCommandAttempt::Unavailable(format!("{exe}: not found"));
    };

    let command_result = if spec.pty {
        if let Some(script_path) = commands.resolve("script") {
            let args = vec![
                OsString::from("-qec"),
                OsString::from(shell_join(spec.argv)),
                OsString::from("/dev/null"),
            ];
            runner.run(script_path, &args, COMMAND_TIMEOUT)
        } else {
            let args = args.iter().map(OsString::from).collect::<Vec<_>>();
            runner.run(program_path, &args, COMMAND_TIMEOUT)
        }
    } else {
        let args = args.iter().map(OsString::from).collect::<Vec<_>>();
        runner.run(program_path, &args, COMMAND_TIMEOUT)
    };

    let output = match command_result {
        Ok(output) => output,
        Err(BoundaryError::CommandFailed { detail, .. }) => {
            return PageCommandAttempt::Unavailable(format!("{exe}: {detail}"));
        }
        Err(error) => return PageCommandAttempt::Unavailable(format!("{exe}: {error}")),
    };

    match output.status {
        CommandStatus::Exit(0) => {}
        CommandStatus::Exit(code) => {
            return PageCommandAttempt::Unavailable(format!("{exe}: exited with status {code}"));
        }
        CommandStatus::Signal(signal) => {
            return PageCommandAttempt::Unavailable(format!(
                "{exe}: terminated by signal {signal}"
            ));
        }
    }

    let raw = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    let cleaned = strip_terminal_noise(&raw);
    let mut lines = cleaned.split('\n').map(str::to_owned).collect::<Vec<_>>();
    while lines
        .last()
        .is_some_and(|line| strip_sgr(line).trim().is_empty())
    {
        lines.pop();
    }
    if spec.max_lines > 0 && lines.len() > spec.max_lines {
        lines.truncate(spec.max_lines);
    }
    if lines.is_empty() {
        lines.push(format!("{exe}: no output"));
    }
    PageCommandAttempt::Completed(lines.join("\n"))
}

/// Escapes command output for Qt RichText monospace display.
#[must_use]
pub fn text_to_mono_html(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let escaped = escape_html(line).replace(' ', "&nbsp;");
            if escaped.is_empty() {
                String::from("&nbsp;")
            } else {
                escaped
            }
        })
        .collect::<Vec<_>>()
        .join("<br>")
}

/// Returns the visible width, in monospace characters, of a text page.
#[must_use]
pub fn text_width(text: &str) -> usize {
    text.split('\n')
        .map(|line| strip_sgr(line).chars().count())
        .max()
        .unwrap_or(0)
}

/// Formats `ss -4tlnp` output into the two-column connections page.
#[must_use]
pub fn format_connections(text: &str, min_width: usize, env: &PageEnvironment) -> (String, usize) {
    let mut rows = Vec::new();
    for line in text.split('\n') {
        if line.trim().is_empty() || line.starts_with("State") || line.starts_with("Netid") {
            continue;
        }

        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() < 5 {
            continue;
        }
        let addr = fields[3];
        let process_field = if fields.len() > 5 {
            Some(fields[5..].join(" "))
        } else {
            None
        };

        if let Some(process_field) = process_field
            && let Some((comm, pid)) = first_ss_process(&process_field)
        {
            rows.push((
                proc_name(comm, pid, &env.proc_root),
                String::from(addr),
                false,
            ));
            continue;
        }

        rows.push((
            service_for_port(addr, env).unwrap_or_else(|| String::from("-")),
            String::from(addr),
            true,
        ));
    }

    if rows.is_empty() {
        let message = "no listening sockets";
        return (text_to_mono_html(message), min_width.max(message.len()));
    }

    let mut entries = Vec::with_capacity(rows.len());
    for (proc, addr, inferred) in rows {
        let (ip, port) = addr.rsplit_once(':').unwrap_or(("", addr.as_str()));
        entries.push((proc, String::from(ip), format!("{ip}:{port:<5}"), inferred));
    }

    let addr_width = entries
        .iter()
        .map(|(_, _, addr, _)| addr.chars().count())
        .max()
        .unwrap_or(0);
    let budget = 10usize.max(min_width.saturating_sub(2 + addr_width));
    let proc_width = entries
        .iter()
        .map(|(proc, _, _, _)| ellipsize(proc, budget).chars().count())
        .max()
        .unwrap_or(0);
    let width = min_width.max(proc_width + 2 + addr_width);

    let mut out = Vec::with_capacity(entries.len());
    for (proc, ip, addr, inferred) in entries {
        let proc = ellipsize(&proc, budget);
        let exposed = !ip.starts_with("127.");
        let proc_class = if exposed {
            "warn"
        } else if inferred {
            "label"
        } else {
            "active"
        };
        let addr_html = if exposed {
            format!(r#"<span class="warn">{}</span>"#, escape_spaces(&addr))
        } else {
            escape_spaces(&addr)
        };
        let gap =
            "&nbsp;".repeat(width.saturating_sub(proc.chars().count() + addr.chars().count()));
        out.push(format!(
            r#"<span class="{proc_class}">{}</span>{gap}{addr_html}"#,
            escape_spaces(&proc)
        ));
    }

    (out.join("<br>"), width)
}

/// Returns the inner HTML plus pager for a command-backed page.
#[must_use]
pub fn page_inner(
    page: &Page,
    idx: usize,
    total: usize,
    min_width: usize,
    runner: &mut impl CommandRunner,
    context: PageCommandContext<'_>,
) -> String {
    let now = context.now;
    page_inner_with_clock(page, idx, total, min_width, runner, context, &mut || now)
}

/// Returns command-page HTML while timestamping a completed refresh from an injected clock.
#[must_use]
pub fn page_inner_with_clock(
    page: &Page,
    idx: usize,
    total: usize,
    min_width: usize,
    runner: &mut impl CommandRunner,
    context: PageCommandContext<'_>,
    capture_clock: &mut impl FnMut() -> Duration,
) -> String {
    let text = run_command_with_clock(
        page,
        runner,
        context.commands,
        context.cache,
        context.now,
        capture_clock,
    );
    let Some(spec) = page.command() else {
        return String::new();
    };

    if spec.colorize == Some(PageColorizer::Connections) {
        let (inner, width) = format_connections(&text, min_width, context.environment);
        return format!(
            r#"<div class="page">{inner}</div>{}"#,
            pager_html(idx, width, total)
        );
    }

    let width = text_width(&text);
    let inner = text_to_mono_html(&text);
    format!(
        r#"<div class="page">{inner}</div>{}"#,
        pager_html(idx, width, total)
    )
}

/// Returns the tooltip title HTML for one deep-dive page.
#[must_use]
pub fn title_html(page: &Page) -> String {
    let label = escape_html(&page.label.to_ascii_uppercase());
    format!(
        r#"<div><span class="title">{label}</span></div><div width="100%" class="title-rule">&nbsp;</div>"#
    )
}

/// Returns the centered pager row for a page body width.
#[must_use]
pub fn pager_html(idx: usize, width: usize, total: usize) -> String {
    if total <= 1 {
        return String::new();
    }

    let dots = (0..total)
        .map(|page_idx| {
            let class = if page_idx == idx { "on" } else { "off" };
            format!(r#"<span class="{class}">●</span>"#)
        })
        .collect::<Vec<_>>()
        .join("&nbsp;");
    let visible = total.saturating_mul(2).saturating_sub(1);
    let pad = width.saturating_sub(visible) / 2;
    format!(r#"<div class="pager">{}{dots}</div>"#, "&nbsp;".repeat(pad))
}

/// Returns the current default click action.
#[must_use]
pub fn default_click() -> &'static [&'static str] {
    FULL_PAGE.click
}

/// Returns the fixed number of rows shown on the top-processes page.
#[must_use]
pub const fn top_process_page_rows() -> usize {
    PROCESS_PAGE_ROWS
}

fn shell_join(argv: &[&str]) -> String {
    argv.iter()
        .map(|arg| {
            if arg
                .bytes()
                .all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' | b'/' | b':'))
            {
                (*arg).to_owned()
            } else {
                let escaped = arg.replace('\'', "'\"'\"'");
                format!("'{escaped}'")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn strip_terminal_noise(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                index += 1;
            }
            0x1b => {
                if let Some(next) = bytes.get(index + 1).copied() {
                    match next {
                        b']' => {
                            index = skip_osc(bytes, index + 2);
                        }
                        b'[' => {
                            let (next_index, keep) = skip_csi(bytes, index);
                            if keep {
                                out.extend_from_slice(&bytes[index..next_index]);
                            }
                            index = next_index;
                        }
                        b'=' | b'>' | b'N' | b'O' | b'P' | b'X' | b'^' | b'_' | b'c' => {
                            index += 2;
                        }
                        _ => {
                            out.push(bytes[index]);
                            index += 1;
                        }
                    }
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn skip_osc(bytes: &[u8], mut index: usize) -> usize {
    while index < bytes.len() {
        match bytes[index] {
            0x07 => return index + 1,
            0x1b if bytes.get(index + 1) == Some(&b'\\') => return index + 2,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn skip_csi(bytes: &[u8], start: usize) -> (usize, bool) {
    let mut index = start + 2;
    while index < bytes.len() {
        let byte = bytes[index];
        if (0x40..=0x7e).contains(&byte) {
            return (index + 1, byte == b'm');
        }
        index += 1;
    }
    (bytes.len(), false)
}

fn strip_sgr(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            let (next_index, keep) = skip_csi(bytes, index);
            if keep {
                index = next_index;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_spaces(text: &str) -> String {
    escape_html(text).replace(' ', "&nbsp;")
}

fn ellipsize(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_owned();
    }
    let keep = max_chars.saturating_sub(1).max(1);
    let mut out = text.chars().take(keep).collect::<String>();
    out.push('…');
    out
}

fn first_ss_process(text: &str) -> Option<(&str, u32)> {
    let name_start = text.find("(\"")? + 2;
    let name_end = text[name_start..].find('"')? + name_start;
    let pid_marker = ",pid=";
    let pid_start = text[name_end..].find(pid_marker)? + name_end + pid_marker.len();
    let pid_end = text[pid_start..]
        .find(|ch: char| !ch.is_ascii_digit())
        .map_or(text.len(), |offset| pid_start + offset);
    let pid = text[pid_start..pid_end].parse().ok()?;
    Some((&text[name_start..name_end], pid))
}

fn proc_name(comm: &str, pid: u32, proc_root: &Path) -> String {
    if !INTERPRETERS
        .iter()
        .any(|interp| comm.eq_ignore_ascii_case(interp))
    {
        return String::from(comm);
    }
    let cmdline_path = proc_root.join(pid.to_string()).join("cmdline");
    let Ok(raw) = fs::read(&cmdline_path) else {
        return String::from(comm);
    };
    let text = String::from_utf8_lossy(&raw);
    let args = text
        .split('\0')
        .filter(|arg| !arg.is_empty())
        .collect::<Vec<_>>();
    for (index, arg) in args.iter().enumerate().skip(1) {
        if *arg == "-m" {
            if let Some(module) = args.get(index + 1) {
                return module.rsplit('/').next().unwrap_or(module).to_owned();
            }
            continue;
        }
        if arg.starts_with('-') {
            continue;
        }
        let base = arg.rsplit('/').next().unwrap_or(arg);
        return base.strip_suffix(".py").unwrap_or(base).to_owned();
    }
    String::from(comm)
}

fn service_for_port(addr: &str, env: &PageEnvironment) -> Option<String> {
    let (_, port_text) = addr.rsplit_once(':')?;
    let port = port_text.trim().parse::<u16>().ok()?;
    if let Some((_, name)) = PORT_DAEMON.iter().find(|(known, _)| *known == port) {
        return Some(String::from(*name));
    }

    let services = if let Some(text) = &env.services_text {
        text.clone()
    } else {
        fs::read_to_string("/etc/services").ok()?
    };
    parse_service_name(port, &services)
}

fn parse_service_name(port: u16, services: &str) -> Option<String> {
    for line in services.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else {
            continue;
        };
        let Some(port_proto) = fields.next() else {
            continue;
        };
        let Some((port_text, protocol)) = port_proto.split_once('/') else {
            continue;
        };
        if protocol == "tcp" && port_text.parse::<u16>().ok() == Some(port) {
            return Some(String::from(name));
        }
    }
    None
}

#[cfg(all(test, feature = "test-support"))]
mod tests;
