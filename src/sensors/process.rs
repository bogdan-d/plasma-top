//! Top-process sampling and cmdline resolution.
//!
//! Owns process sampling formerly grouped inside `src/sensors.py`:
//!
//! - [`read_proc_stat_times`] scans `/proc/[pid]/stat` for the per-process
//!   jiffies/RSS snapshot that the CPU-diff uses.
//! - [`cmdline_name`] resolves a fuller process name from `/proc/[pid]/cmdline`
//!   for the tooltip processes page.
//! - [`diff_top_process`] derives `(pid, comm, cpu%, mem%)` rows from two
//!   snapshots plus an elapsed window, matching Python's "normalized to one
//!   core" semantics.
//! - [`read_top_process`] and [`read_top_process_page`] perform panel and page attempts while synchronous collection preserves cadence and warm-start behavior.
//!
//! All readers take explicit proc roots and clock snapshots so tests never touch
//! the host filesystem or sleep. The CLK_TCK / PAGE_SIZE constants mirror the
//! `os.sysconf` values Python reads once at module load (always 100 and 4096 on
//! Linux); total RAM is loaded lazily via [`crate::sensors::memory`].

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

use crate::domain::boundary::ClockSnapshot;
use crate::domain::readings::{MetricSample, RetainedMetricSample, TopProcessDetails};

/// `_CLK_TCK = os.sysconf("SC_CLK_TCK")` — always 100 on Linux.
const CLK_TCK: u64 = 100;
/// `_PAGE_SIZE = os.sysconf("SC_PAGE_SIZE")` — always 4096 on Linux.
const PAGE_SIZE: u64 = 4096;
/// `/proc/[pid]/stat` read cap (always covers comm + fields through rss).
const PROC_STAT_READ: usize = 1024;
/// `/proc/[pid]/cmdline` read cap (argv\[0\] + first args).
pub const CMDLINE_READ: usize = 512;
/// Resolved cmdline name cap; the formatter truncates further to the column.
pub const CMDLINE_MAX: usize = 64;
/// Panel top-process row count (Top 1/2/3). Mirrors Python's
/// `TOP_PROCESS_COUNT`. The collector slices the full list to this many.
pub const TOP_PROCESS_COUNT: usize = 3;
/// Panel top-process freshness budget; scanning `/proc/[pid]/stat` is too costly on every display pass.
pub const TOP_PROCESS_TTL: Duration = Duration::from_secs(15);

/// Parsed `/proc/[pid]/stat` row retained for the top-process diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcStatRow {
    /// Kernel-capped `comm` (≤16 chars, decoded latin-1).
    pub comm: String,
    /// `utime + stime` in jiffies.
    pub total_jiffies: u64,
    /// RSS in pages.
    pub rss_pages: u64,
}

/// Mutable process-diff/cache state that persists between polls.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcessState {
    /// Previous per-pid `utime + stime` totals seen by the panel path.
    pub proc_prev_times: BTreeMap<u32, u64>,
    /// Monotonic instant of the previous panel sample.
    pub proc_prev_sample_at: Option<Duration>,
    /// Latest valid panel rows and latest attempt time.
    pub(super) panel: RetainedMetricSample<Vec<TopProcessDetails>>,
    /// Previous per-pid totals used by the tooltip processes page (its own
    /// cadence, warm-started from `proc_prev_times` on the first call).
    pub page_proc_prev_times: BTreeMap<u32, u64>,
    /// Monotonic instant of the previous page sample.
    pub page_proc_prev_sample_at: Option<Duration>,
    /// Latest valid process-page rows and attempt diagnostics.
    pub page: RetainedMetricSample<Vec<TopProcessDetails>>,
    /// Cached total RAM in bytes (matches Python's module-level cache). `None`
    /// means "not yet read"; the resolved value (including 0 on read failure)
    /// is stored on first lookup.
    pub total_mem_bytes_cache: Option<u64>,
}

impl ProcessState {
    pub(crate) fn reconcile_panel(&mut self, demanded: bool) {
        if !demanded {
            self.reset_panel();
        }
    }

    /// Returns the cached total-RAM value, reading `/proc/meminfo` once.
    fn total_mem_bytes(&mut self, proc_root: &Path) -> u64 {
        *self.total_mem_bytes_cache.get_or_insert_with(|| {
            crate::sensors::memory::read_mem_total_bytes(proc_root).unwrap_or(0)
        })
    }

    pub(crate) fn reset_panel(&mut self) {
        self.panel.invalidate();
        self.proc_prev_times.clear();
        self.proc_prev_sample_at = None;
    }

    pub(crate) fn reset_page(&mut self) {
        self.page.invalidate();
        self.page_proc_prev_times.clear();
        self.page_proc_prev_sample_at = None;
    }
}

/// Scans `/proc/[pid]/stat` for every numeric pid directory.
///
/// Mirrors `src/sensors.py::_read_proc_stat_times`: 1024-byte raw read per file,
/// `comm` taken between the first `(` and the last `)`, utime/stime/rss from
/// the post-`)` fields (indices 11/12/21 with a 22-field floor). Per-process
/// errors are skipped; directory read failure returns an empty map.
#[must_use]
pub fn read_proc_stat_times(proc_root: &Path) -> BTreeMap<u32, ProcStatRow> {
    try_read_proc_stat_times(proc_root).unwrap_or_default()
}

fn try_read_proc_stat_times(proc_root: &Path) -> Option<BTreeMap<u32, ProcStatRow>> {
    let mut result = BTreeMap::new();
    let Ok(entries) = fs::read_dir(proc_root) else {
        return None;
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let stat_path = entry.path().join("stat");
        let mut file = match fs::File::open(&stat_path) {
            Ok(file) => file,
            Err(_) => continue,
        };
        let mut buf = vec![0u8; PROC_STAT_READ];
        let Ok(read) = file.read(&mut buf) else {
            continue;
        };
        let Some(row) = parse_proc_stat(&buf[..read]) else {
            continue;
        };
        result.insert(pid, row);
    }
    Some(result)
}

/// Parses a single `/proc/[pid]/stat` buffer into a [`ProcStatRow`].
fn parse_proc_stat(buf: &[u8]) -> Option<ProcStatRow> {
    let lparen = buf.iter().position(|b| *b == b'(')?;
    let rparen = buf.iter().rposition(|b| *b == b')')?;
    if rparen <= lparen + 1 {
        return None;
    }
    let comm_bytes = &buf[lparen + 1..rparen];
    // latin-1 decode (every byte maps to its own code point; the "replace"
    // error handler Python requests never triggers for latin-1).
    let comm: String = comm_bytes
        .iter()
        .map(|b| char::from_u32(u32::from(*b)).unwrap_or('\u{FFFD}'))
        .collect();

    let post = &buf[rparen + 2..];
    let mut tokens = post
        .split(|b: &u8| b.is_ascii_whitespace())
        .filter(|t| !t.is_empty());
    let mut fields: Vec<&[u8]> = Vec::with_capacity(22);
    for _ in 0..22 {
        let Some(tok) = tokens.next() else {
            break;
        };
        fields.push(tok);
    }
    if fields.len() < 22 {
        return None;
    }
    let utime = parse_u64(fields[11])?;
    let stime = parse_u64(fields[12])?;
    let rss = parse_u64(fields[21])?;
    Some(ProcStatRow {
        comm,
        total_jiffies: utime.saturating_add(stime),
        rss_pages: rss,
    })
}

fn parse_u64(bytes: &[u8]) -> Option<u64> {
    std::str::from_utf8(bytes).ok()?.parse::<u64>().ok()
}

/// Returns a process name from `/proc/[pid]/cmdline`, falling back to `comm`.
///
/// Mirrors `src/sensors.py::_cmdline_name`: argv is NUL-separated, argv\[0\] is
/// reduced to its basename with the remaining args appended, capped to
/// `CMDLINE_MAX` characters. Kernel threads and zombies (empty cmdline) fall
/// back to the supplied `comm`.
#[must_use]
pub fn cmdline_name(proc_root: &Path, pid: u32, fallback: &str) -> String {
    let cmdline_path = proc_root.join(pid.to_string()).join("cmdline");
    let mut file = match fs::File::open(&cmdline_path) {
        Ok(file) => file,
        Err(_) => return String::from(fallback),
    };
    let mut buf = vec![0u8; CMDLINE_READ];
    let Ok(read) = file.read(&mut buf) else {
        return String::from(fallback);
    };
    let raw = &buf[..read];
    let mut parts = raw.split(|b| *b == 0).filter(|part| !part.is_empty());
    let Some(argv0) = parts.next() else {
        return String::from(fallback);
    };
    let basename = argv0.rsplit(|b| *b == b'/').next().unwrap_or(argv0);
    let mut name = String::from_utf8_lossy(basename).into_owned();
    for arg in parts {
        name.push(' ');
        name.push_str(&String::from_utf8_lossy(arg));
    }
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return String::from(fallback);
    }
    let truncated: String = trimmed.chars().take(CMDLINE_MAX).collect();
    if truncated.is_empty() {
        String::from(fallback)
    } else {
        truncated
    }
}

/// Computes the sorted top-process list from two stat snapshots.
///
/// Mirrors `src/sensors.py::_diff_top_process`: CPU% is the jiffies diff
/// normalized to one core (like `top`), mem% is RSS over total RAM. When
/// `keep_idle` is false (the panel path), processes with 0% CPU are dropped;
/// the tooltip page passes `true` to always fill a fixed row count. The return
/// is sorted by CPU desc, then mem desc, then pid desc — matching Python's
/// `(pct, mem, pid, comm)` tuple sort.
#[must_use]
pub fn diff_top_process(
    current: &BTreeMap<u32, ProcStatRow>,
    prev: &BTreeMap<u32, u64>,
    dt: f64,
    total_mem_bytes: u64,
    keep_idle: bool,
) -> Vec<TopProcessDetails> {
    let mut candidates: Vec<TopProcessDetails> = Vec::new();
    if prev.is_empty() || dt <= 0.0 {
        return candidates;
    }
    for (pid, row) in current {
        let Some(&prev_total) = prev.get(pid) else {
            continue;
        };
        if row.total_jiffies < prev_total {
            continue;
        }
        let used = row.total_jiffies - prev_total;
        let pct = (used as f64 / CLK_TCK as f64 / dt * 100.0) as i32;
        if pct <= 0 && !keep_idle {
            continue;
        }
        let mem = if total_mem_bytes > 0 {
            let bytes = row.rss_pages.saturating_mul(PAGE_SIZE);
            bytes as f64 / total_mem_bytes as f64 * 100.0
        } else {
            0.0
        };
        candidates.push(TopProcessDetails {
            pid: *pid,
            command: row.comm.clone(),
            cpu_percent: pct,
            memory_percent: mem,
        });
    }
    candidates.sort_by(|a, b| {
        b.cpu_percent
            .cmp(&a.cpu_percent)
            .then(
                b.memory_percent
                    .partial_cmp(&a.memory_percent)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(b.pid.cmp(&a.pid))
            .then(b.command.cmp(&a.command))
    });
    candidates
}

/// Panel path: scans `/proc/[pid]/stat`, diffs against the previous sample,
/// and returns the full sorted list (or `None` if empty).
///
/// Updates `state.proc_prev_*` in place. The caller slices the first
/// `TOP_PROCESS_COUNT` rows for the panel's Top 1/2/3.
#[must_use]
pub fn read_top_process(
    proc_root: &Path,
    state: &mut ProcessState,
    clock: ClockSnapshot,
) -> Option<Vec<TopProcessDetails>> {
    match read_top_process_once(proc_root, state, clock) {
        ProcessReadOutcome::Rows(rows) => Some(rows),
        ProcessReadOutcome::Baseline | ProcessReadOutcome::Empty | ProcessReadOutcome::Failed => {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProcessReadOutcome {
    Rows(Vec<TopProcessDetails>),
    Baseline,
    Empty,
    Failed,
}

/// Outcome of one typed process-page scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessPageStatus {
    /// A successful scan produced display rows.
    Captured,
    /// A successful first scan established the diff baseline.
    Baseline,
    /// A successful scan produced no comparable rows.
    Empty,
    /// Procfs could not be scanned; retained rows remain valid for display.
    Failed,
}

/// Retained result of one process-page scan.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessPageResult {
    /// Latest successful page sample, retained across scan failure.
    pub sample: Option<MetricSample<Vec<TopProcessDetails>>>,
    /// Monotonic instant of the latest scan attempt.
    pub attempted_at: Option<Duration>,
    /// Monotonic instant of the latest failed scan.
    pub failed_at: Option<Duration>,
    /// Outcome of the current attempt.
    pub status: ProcessPageStatus,
}

/// Performs one panel process scan and distinguishes valid emptiness from failure.
#[must_use]
pub(crate) fn read_top_process_once(
    proc_root: &Path,
    state: &mut ProcessState,
    clock: ClockSnapshot,
) -> ProcessReadOutcome {
    let total_mem = state.total_mem_bytes(proc_root);
    let Some(current) = try_read_proc_stat_times(proc_root) else {
        return ProcessReadOutcome::Failed;
    };
    let had_baseline = !state.proc_prev_times.is_empty();
    let dt = elapsed_since(state.proc_prev_sample_at, clock.monotonic);
    if had_baseline && dt <= 0.0 {
        return ProcessReadOutcome::Baseline;
    }
    let result = diff_top_process(&current, &state.proc_prev_times, dt, total_mem, false);
    state.proc_prev_times = current
        .iter()
        .map(|(pid, row)| (*pid, row.total_jiffies))
        .collect();
    state.proc_prev_sample_at = Some(clock.monotonic);
    if !had_baseline {
        ProcessReadOutcome::Baseline
    } else if result.is_empty() {
        ProcessReadOutcome::Empty
    } else {
        ProcessReadOutcome::Rows(result)
    }
}

/// Tooltip top-processes page: a fresh sample every call, off its own
/// prev-state so it updates each poll instead of every `TOP_PROCESS_TTL`.
///
/// Mirrors `src/sensors.py::read_top_process_page`: `keep_idle=True` keeps
/// 0%-CPU rows so the page always fills a fixed row count (stable tooltip
/// height). On the first call the panel's `proc_prev_*` state warm-starts the
/// diff (up to `TOP_PROCESS_TTL` old) so the first render is real data
/// instead of "old then resize". Resolves the fuller cmdline name only for
/// the rows actually shown (top
/// [`crate::page_commands::top_process_page_rows`]).
#[must_use]
pub fn read_top_process_page(
    proc_root: &Path,
    state: &mut ProcessState,
    clock: ClockSnapshot,
) -> Option<Vec<TopProcessDetails>> {
    read_top_process_page_attempt(proc_root, state, clock)
        .sample
        .map(|sample| sample.value)
}

/// Performs one typed process-page attempt without hiding scan failure as an empty page.
#[must_use]
pub fn read_top_process_page_attempt(
    proc_root: &Path,
    state: &mut ProcessState,
    clock: ClockSnapshot,
) -> ProcessPageResult {
    let Some(current) = try_read_proc_stat_times(proc_root) else {
        state.page.record_failure(clock.monotonic);
        return process_page_result(state, ProcessPageStatus::Failed);
    };
    let total_mem = state.total_mem_bytes(proc_root);
    // First open: warm-start from the panel's prev (up to TOP_PROCESS_TTL old).
    let prev = if state.page_proc_prev_times.is_empty() {
        &state.proc_prev_times
    } else {
        &state.page_proc_prev_times
    };
    let had_baseline = !prev.is_empty();
    let prev_sample_at = state.page_proc_prev_sample_at.or(state.proc_prev_sample_at);
    let dt = elapsed_since(prev_sample_at, clock.monotonic);
    if !had_baseline {
        state.page_proc_prev_times = current
            .iter()
            .map(|(pid, row)| (*pid, row.total_jiffies))
            .collect();
        state.page_proc_prev_sample_at = Some(clock.monotonic);
        state.page.record_baseline(clock.monotonic);
        return process_page_result(state, ProcessPageStatus::Baseline);
    }
    if dt <= 0.0 {
        state.page.record_baseline(clock.monotonic);
        return process_page_result(state, ProcessPageStatus::Baseline);
    }
    let result = diff_top_process(&current, prev, dt, total_mem, true);
    state.page_proc_prev_times = current
        .iter()
        .map(|(pid, row)| (*pid, row.total_jiffies))
        .collect();
    state.page_proc_prev_sample_at = Some(clock.monotonic);
    if result.is_empty() {
        state.page.record_absence(clock.monotonic);
        return process_page_result(
            state,
            if had_baseline {
                ProcessPageStatus::Empty
            } else {
                ProcessPageStatus::Baseline
            },
        );
    }
    let limit = crate::page_commands::top_process_page_rows();
    let rows = result
        .into_iter()
        .take(limit)
        .map(|row| TopProcessDetails {
            command: cmdline_name(proc_root, row.pid, &row.command),
            ..row
        })
        .collect();
    state.page.record_value(rows, clock.monotonic);
    process_page_result(state, ProcessPageStatus::Captured)
}

fn process_page_result(state: &ProcessState, status: ProcessPageStatus) -> ProcessPageResult {
    ProcessPageResult {
        sample: state.page.latest.clone(),
        attempted_at: state.page.attempted_at,
        failed_at: state.page.failed_at,
        status,
    }
}

fn elapsed_since(prev: Option<Duration>, now: Duration) -> f64 {
    match prev {
        Some(prev) => now.as_secs_f64() - prev.as_secs_f64(),
        None => 0.0,
    }
}

#[cfg(test)]
mod tests;
