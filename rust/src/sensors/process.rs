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
//! - [`read_top_process`] / [`read_top_process_cached`] / [`read_top_process_page`]
//!   drive the panel and tooltip cadences with the same TTL and warm-start
//!   behavior as Python.
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
use crate::domain::readings::TopProcessDetails;

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
/// Panel top-process TTL — scanning `/proc/[pid]/stat` is too costly per poll.
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
    /// TTL-cached panel top-process rows (`None` until a real sample lands).
    pub top_process_cache: Option<Vec<TopProcessDetails>>,
    /// Monotonic instant of the cached panel sample.
    pub top_process_cache_sample_at: Option<Duration>,
    /// Previous per-pid totals used by the tooltip processes page (its own
    /// cadence, warm-started from `proc_prev_times` on the first call).
    pub page_proc_prev_times: BTreeMap<u32, u64>,
    /// Monotonic instant of the previous page sample.
    pub page_proc_prev_sample_at: Option<Duration>,
    /// Cached total RAM in bytes (matches Python's module-level cache). `None`
    /// means "not yet read"; the resolved value (including 0 on read failure)
    /// is stored on first lookup.
    pub total_mem_bytes_cache: Option<u64>,
}

impl ProcessState {
    /// Returns the cached total-RAM value, reading `/proc/meminfo` once.
    fn total_mem_bytes(&mut self, proc_root: &Path) -> u64 {
        *self.total_mem_bytes_cache.get_or_insert_with(|| {
            crate::sensors::memory::read_mem_total_bytes(proc_root).unwrap_or(0)
        })
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
    let mut result = BTreeMap::new();
    let Ok(entries) = fs::read_dir(proc_root) else {
        return result;
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
    result
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
    let total_mem = state.total_mem_bytes(proc_root);
    let current = read_proc_stat_times(proc_root);
    let dt = elapsed_since(state.proc_prev_sample_at, clock.monotonic);
    let result = diff_top_process(&current, &state.proc_prev_times, dt, total_mem, false);
    state.proc_prev_times = current
        .iter()
        .map(|(pid, row)| (*pid, row.total_jiffies))
        .collect();
    state.proc_prev_sample_at = Some(clock.monotonic);
    (!result.is_empty()).then_some(result)
}

/// TTL-cached panel wrapper around [`read_top_process`].
///
/// Mirrors `src/sensors.py::_read_top_process_cached`: a cached `Some` value
/// is held for `TOP_PROCESS_TTL`, but a cached `None` (or expired entry)
/// refreshes immediately so the panel doesn't wait a full TTL for the first
/// real reading after startup.
#[must_use]
pub fn read_top_process_cached(
    proc_root: &Path,
    state: &mut ProcessState,
    clock: ClockSnapshot,
) -> Option<Vec<TopProcessDetails>> {
    if cache_fresh(
        state.top_process_cache.as_ref(),
        state.top_process_cache_sample_at,
        clock,
    ) {
        return state.top_process_cache.clone();
    }
    let result = read_top_process(proc_root, state, clock);
    state.top_process_cache = result.clone();
    state.top_process_cache_sample_at = Some(clock.monotonic);
    result
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
    let total_mem = state.total_mem_bytes(proc_root);
    let current = read_proc_stat_times(proc_root);
    // First open: warm-start from the panel's prev (up to TOP_PROCESS_TTL old).
    let prev = if state.page_proc_prev_times.is_empty() {
        &state.proc_prev_times
    } else {
        &state.page_proc_prev_times
    };
    let prev_sample_at = state.page_proc_prev_sample_at.or(state.proc_prev_sample_at);
    let dt = elapsed_since(prev_sample_at, clock.monotonic);
    let result = diff_top_process(&current, prev, dt, total_mem, true);
    state.page_proc_prev_times = current
        .iter()
        .map(|(pid, row)| (*pid, row.total_jiffies))
        .collect();
    state.page_proc_prev_sample_at = Some(clock.monotonic);
    if result.is_empty() {
        return None;
    }
    let limit = crate::page_commands::top_process_page_rows();
    Some(
        result
            .into_iter()
            .take(limit)
            .map(|row| TopProcessDetails {
                command: cmdline_name(proc_root, row.pid, &row.command),
                ..row
            })
            .collect(),
    )
}

fn elapsed_since(prev: Option<Duration>, now: Duration) -> f64 {
    match prev {
        Some(prev) => now.as_secs_f64() - prev.as_secs_f64(),
        None => 0.0,
    }
}

fn cache_fresh(
    cache: Option<&Vec<TopProcessDetails>>,
    sampled_at: Option<Duration>,
    clock: ClockSnapshot,
) -> bool {
    match cache {
        Some(_) => match sampled_at {
            Some(prev) => {
                let elapsed = clock.monotonic.saturating_sub(prev).as_secs_f64();
                elapsed < TOP_PROCESS_TTL.as_secs_f64()
            }
            None => false,
        },
        None => false,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn clock_at(seconds: u64) -> ClockSnapshot {
        ClockSnapshot {
            monotonic: Duration::from_secs(seconds),
            wall: UNIX_EPOCH + Duration::from_secs(seconds),
        }
    }

    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_nanos();
            let root = std::env::temp_dir()
                .join(format!("pirostats-process-{}-{unique}", std::process::id()));
            if let Err(error) = fs::create_dir_all(&root) {
                panic!("failed to create temp root {}: {error}", root.display());
            }
            Self { root }
        }

        fn proc_root(&self) -> PathBuf {
            self.root.join("proc")
        }

        fn write(&self, relative: &str, content: &[u8]) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent()
                && let Err(error) = fs::create_dir_all(parent)
            {
                panic!("failed to create {}: {error}", parent.display());
            }
            if let Err(error) = fs::write(&path, content) {
                panic!("failed to write {}: {error}", path.display());
            }
        }

        fn write_str(&self, relative: &str, content: &str) {
            self.write(relative, content.as_bytes());
        }

        /// Writes a `/proc/[pid]/stat` file with a realistic shape; `utime`,
        /// `stime`, and `rss` populate the post-`)` fields at indices 11/12/21.
        fn write_proc_stat(&self, pid: u32, comm: &str, utime: u64, stime: u64, rss: u64) {
            // 22 fields after the `) `, with utime=11, stime=12, rss=21.
            let mut fields: Vec<String> = (0..22).map(|i| (i + 100).to_string()).collect();
            fields[11] = utime.to_string();
            fields[12] = stime.to_string();
            fields[21] = rss.to_string();
            let line = format!("{} ({}) {}\n", pid, comm, fields.join(" "));
            self.write_str(&format!("proc/{pid}/stat"), &line);
        }

        fn write_proc_stat_raw(&self, pid: u32, content: &str) {
            self.write_str(&format!("proc/{pid}/stat"), content);
        }

        fn write_cmdline(&self, pid: u32, parts: &[&str]) {
            let mut bytes: Vec<u8> = Vec::new();
            for part in parts {
                bytes.extend_from_slice(part.as_bytes());
                bytes.push(0);
            }
            self.write(&format!("proc/{pid}/cmdline"), &bytes);
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn parse_proc_stat_extracts_comm_jiffies_and_rss() {
        let tmp = TempTree::new();
        // Real-shape stat: 22 post-`)` fields, with utime=12345, stime=678,
        // rss=500. Field values are arbitrary except for those three.
        tmp.write_proc_stat_raw(
            1234,
            "1234 (firefox) R 1229 1234 1229 34817 1234 4202496 1 2 3 4 12345 678 \
             0 0 20 0 1 0 100000 200000000 500 rest tail\n",
        );

        let rows = read_proc_stat_times(&tmp.proc_root());

        let row = rows.get(&1234).expect("pid 1234 parsed");
        assert_eq!(row.comm, "firefox");
        assert_eq!(row.total_jiffies, 12345 + 678);
        assert_eq!(row.rss_pages, 500);
    }

    #[test]
    fn parse_proc_stat_handles_comm_with_parens_via_rparen() {
        let tmp = TempTree::new();
        // comm contains a literal `)` — rposition picks the *last* `)`.
        tmp.write_proc_stat_raw(
            42,
            "42 (foo (bar)) R 0 0 0 0 0 0 0 0 0 0 10 20 0 0 20 0 1 0 0 0 30 tail\n",
        );

        let rows = read_proc_stat_times(&tmp.proc_root());

        let row = rows.get(&42).expect("pid 42 parsed");
        assert_eq!(row.comm, "foo (bar)");
        assert_eq!(row.total_jiffies, 30);
        assert_eq!(row.rss_pages, 30);
    }

    #[test]
    fn parse_proc_stat_decodes_latin1_comm_without_losing_bytes() {
        let tmp = TempTree::new();
        // Bytes 0xC3 0xA9 are utf-8 for é; latin-1 reads them as two code points
        // (Ã©). Python's `decode("latin-1", "replace")` produces the same.
        let raw = b"7 (caf\xc3\xa9) R 0 0 0 0 0 0 0 0 0 0 1 2 0 0 20 0 1 0 0 0 3 tail\n";
        tmp.write("proc/7/stat", raw);

        let rows = read_proc_stat_times(&tmp.proc_root());

        let row = rows.get(&7).expect("pid 7 parsed");
        assert_eq!(row.comm, "caf\u{c3}\u{a9}");
        assert_eq!(row.total_jiffies, 3);
    }

    #[test]
    fn parse_proc_stat_skips_entries_with_too_few_fields() {
        let tmp = TempTree::new();
        tmp.write_proc_stat_raw(1, "1 (init) R 0 0\n");
        tmp.write_proc_stat(2, "ok", 100, 50, 10);

        let rows = read_proc_stat_times(&tmp.proc_root());

        assert!(!rows.contains_key(&1));
        assert!(rows.contains_key(&2));
    }

    #[test]
    fn parse_proc_stat_skips_non_numeric_pid_directories() {
        let tmp = TempTree::new();
        tmp.write_str(
            "proc/self/stat",
            "0 (self) R 0 0 0 0 0 0 0 0 0 0 0 0 0 0 20 0 1 0 0 0 0 tail\n",
        );
        tmp.write_proc_stat(123, "real", 1, 2, 3);

        let rows = read_proc_stat_times(&tmp.proc_root());

        assert!(rows.contains_key(&123));
        // `self` is non-numeric and should be skipped without error.
        assert!(rows.values().all(|row| row.comm != "self"));
    }

    #[test]
    fn parse_proc_stat_skips_missing_stat_files() {
        let tmp = TempTree::new();
        fs::create_dir_all(tmp.proc_root().join("999")).unwrap();
        tmp.write_proc_stat(123, "real", 1, 2, 3);

        let rows = read_proc_stat_times(&tmp.proc_root());

        assert!(!rows.contains_key(&999));
        assert!(rows.contains_key(&123));
    }

    #[test]
    fn cmdline_name_joins_argv_basename_with_remaining_args() {
        let tmp = TempTree::new();
        tmp.write_cmdline(
            100,
            &["/usr/lib/firefox/firefox", "-contentproc", "-childID", "1"],
        );

        let name = cmdline_name(&tmp.proc_root(), 100, "firefox");

        assert_eq!(name, "firefox -contentproc -childID 1");
    }

    #[test]
    fn cmdline_name_returns_fallback_when_cmdline_is_empty() {
        let tmp = TempTree::new();
        // Kernel thread: empty cmdline (single trailing NUL or fully empty).
        tmp.write_cmdline(2, &[""]);
        tmp.write_cmdline(3, &[]);

        assert_eq!(cmdline_name(&tmp.proc_root(), 2, "kworker"), "kworker");
        assert_eq!(cmdline_name(&tmp.proc_root(), 3, "kthread"), "kthread");
    }

    #[test]
    fn cmdline_name_returns_fallback_when_file_is_missing() {
        let tmp = TempTree::new();

        assert_eq!(cmdline_name(&tmp.proc_root(), 999, "fallback"), "fallback");
    }

    #[test]
    fn cmdline_name_caps_to_max_chars_and_handles_basename_only() {
        let tmp = TempTree::new();
        let long_arg = "a".repeat(CMDLINE_READ);
        tmp.write_cmdline(5, &[&format!("/usr/bin/{long_arg}")]);

        let name = cmdline_name(&tmp.proc_root(), 5, "comm");

        assert_eq!(name.chars().count(), CMDLINE_MAX);
        // argv[0] reduced to basename.
        assert!(name.starts_with('a'));
    }

    #[test]
    fn cmdline_name_strips_basename_with_trailing_slash_to_fallback() {
        let tmp = TempTree::new();
        // rsplit('/') on a single "/" gives ["", ""]; basename is empty, join
        // collapses to empty, strip is empty → fallback (matches Python).
        tmp.write_cmdline(6, &["/"]);

        assert_eq!(cmdline_name(&tmp.proc_root(), 6, "fallback"), "fallback");
    }

    #[test]
    fn diff_top_process_returns_empty_without_prev_or_dt() {
        let mut current = BTreeMap::new();
        current.insert(
            1,
            ProcStatRow {
                comm: "a".into(),
                total_jiffies: 100,
                rss_pages: 10,
            },
        );
        let prev = BTreeMap::new();

        assert!(diff_top_process(&current, &prev, 1.0, 1_000_000_000, false).is_empty());
        assert!(diff_top_process(&current, &prev, 0.0, 1_000_000_000, false).is_empty());

        let prev = BTreeMap::from([(1, 50u64)]);
        assert!(diff_top_process(&current, &prev, 0.0, 1_000_000_000, false).is_empty());
    }

    #[test]
    fn diff_top_process_normalizes_to_one_core_and_drops_idle_when_not_keepidle() {
        // prev_total=100, current=200, dt=1.0s → 100 jiffies / 100 Hz / 1s * 100
        // = 100% (not capped — the cap is at 99 only in the gpu path).
        let mut current = BTreeMap::new();
        current.insert(
            1,
            ProcStatRow {
                comm: "busy".into(),
                total_jiffies: 200,
                rss_pages: 1000,
            },
        );
        current.insert(
            2,
            ProcStatRow {
                comm: "idle".into(),
                total_jiffies: 100,
                rss_pages: 1000,
            },
        );
        let prev = BTreeMap::from([(1u32, 100u64), (2u32, 100u64)]);

        // 4096 B/page * 1000 pages = 4_096_000 B; total 8_000_000 → 51.2%.
        let rows = diff_top_process(&current, &prev, 1.0, 8_000_000, false);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].pid, 1);
        assert_eq!(rows[0].cpu_percent, 100);
        assert_eq!(rows[0].memory_percent, 51.2);
    }

    #[test]
    fn diff_top_process_keep_idle_retains_zero_cpu_rows() {
        let mut current = BTreeMap::new();
        current.insert(
            1,
            ProcStatRow {
                comm: "idle".into(),
                total_jiffies: 100,
                rss_pages: 0,
            },
        );
        let prev = BTreeMap::from([(1u32, 100u64)]);

        let rows = diff_top_process(&current, &prev, 1.0, 1_000_000_000, true);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cpu_percent, 0);
    }

    #[test]
    fn diff_top_process_skips_pid_rollback_and_unknown_pids() {
        let mut current = BTreeMap::new();
        current.insert(
            1,
            ProcStatRow {
                comm: "rollback".into(),
                total_jiffies: 50,
                rss_pages: 0,
            },
        );
        current.insert(
            2,
            ProcStatRow {
                comm: "newpid".into(),
                total_jiffies: 100,
                rss_pages: 0,
            },
        );
        // prev_total > current for pid 1 (counter reset); pid 2 not in prev.
        let prev = BTreeMap::from([(1u32, 100u64)]);

        let rows = diff_top_process(&current, &prev, 1.0, 1_000_000_000, true);

        // Both skipped: rollback + unknown.
        assert!(rows.is_empty());
    }

    #[test]
    fn diff_top_process_sorts_by_cpu_then_mem_desc() {
        let mut current = BTreeMap::new();
        current.insert(
            1,
            ProcStatRow {
                comm: "a".into(),
                total_jiffies: 200,
                rss_pages: 100,
            },
        );
        current.insert(
            2,
            ProcStatRow {
                comm: "b".into(),
                total_jiffies: 200,
                rss_pages: 200,
            },
        );
        current.insert(
            3,
            ProcStatRow {
                comm: "c".into(),
                total_jiffies: 300,
                rss_pages: 0,
            },
        );
        let prev = BTreeMap::from([(1u32, 100u64), (2u32, 100u64), (3u32, 100u64)]);

        let rows = diff_top_process(&current, &prev, 1.0, 1_000_000, false);

        // pct = delta/100/1*100 = delta: pid1=100, pid2=100, pid3=200.
        // mem = rss*4096/1e6*100: pid1=40.96, pid2=81.92, pid3=0.
        // Sort by cpu desc then mem desc → pid3 (200%), pid2 (100,81.92), pid1.
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].pid, 3);
        assert_eq!(rows[1].pid, 2);
        assert_eq!(rows[2].pid, 1);
    }

    #[test]
    fn read_top_process_first_call_returns_none_and_seeds_prev() {
        let tmp = TempTree::new();
        tmp.write_proc_stat(100, "firefox", 100, 50, 200);
        tmp.write_proc_stat(101, "kwin", 10, 5, 50);
        tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");

        let mut state = ProcessState::default();

        let first = read_top_process(&tmp.proc_root(), &mut state, clock_at(0));

        assert!(first.is_none());
        // prev was seeded with current totals (pid → utime+stime).
        assert_eq!(state.proc_prev_times.get(&100).copied(), Some(150));
        assert_eq!(state.proc_prev_times.get(&101).copied(), Some(15));
        assert_eq!(state.proc_prev_sample_at, Some(Duration::ZERO));
        assert_eq!(state.total_mem_bytes_cache, Some(2 * 1024 * 1024 * 1024));
    }

    #[test]
    fn read_top_process_second_call_returns_sorted_rows() {
        let tmp = TempTree::new();
        tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
        tmp.write_proc_stat(100, "firefox", 100, 0, 200);
        tmp.write_proc_stat(101, "kwin", 10, 0, 50);

        let mut state = ProcessState::default();
        // Prime prev so the next call can diff.
        read_top_process(&tmp.proc_root(), &mut state, clock_at(0)).check_none();

        // Bump both processes' jiffies. firefox +200, kwin +10 in 1s → 200%, 10%.
        tmp.write_proc_stat(100, "firefox", 300, 0, 200);
        tmp.write_proc_stat(101, "kwin", 20, 0, 50);

        let rows =
            read_top_process(&tmp.proc_root(), &mut state, clock_at(1)).expect("non-empty result");

        assert_eq!(rows[0].pid, 100);
        assert_eq!(rows[0].cpu_percent, 200);
        assert_eq!(rows[0].command, "firefox");
        assert_eq!(rows[1].pid, 101);
        assert_eq!(rows[1].cpu_percent, 10);
    }

    #[test]
    fn read_top_process_cached_serves_within_ttl_and_refreshes_after() {
        let tmp = TempTree::new();
        tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
        tmp.write_proc_stat(100, "firefox", 100, 0, 200);

        let mut state = ProcessState::default();
        // First call seeds prev (returns None).
        read_top_process_cached(&tmp.proc_root(), &mut state, clock_at(0)).check_none();

        tmp.write_proc_stat(100, "firefox", 300, 0, 200);
        // Second call (t=1) computes a real value and caches it.
        let cached = read_top_process_cached(&tmp.proc_root(), &mut state, clock_at(1));
        assert!(cached.is_some());

        // Within TTL: no recompute even though /proc has changed.
        tmp.write_proc_stat(100, "firefox", 10_000, 0, 200);
        let served = read_top_process_cached(&tmp.proc_root(), &mut state, clock_at(2));
        assert_eq!(served, cached);

        // After TTL: refresh.
        let refreshed = read_top_process_cached(&tmp.proc_root(), &mut state, clock_at(20));
        assert_ne!(refreshed, cached);
        assert!(refreshed.is_some());
    }

    #[test]
    fn read_top_process_cached_retries_immediately_after_none_cache() {
        let tmp = TempTree::new();
        tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
        tmp.write_proc_stat(100, "firefox", 100, 0, 200);

        let mut state = ProcessState::default();
        // First call returns None (no prev) — cache should not hold None for TTL.
        read_top_process_cached(&tmp.proc_root(), &mut state, clock_at(0)).check_none();

        // Bump jiffies and clock so the diff has a non-zero dt and delta.
        tmp.write_proc_stat(100, "firefox", 300, 0, 200);
        let result = read_top_process_cached(&tmp.proc_root(), &mut state, clock_at(1));
        assert!(result.is_some());
    }

    #[test]
    fn read_top_process_page_warm_starts_from_panel_prev_then_uses_own_prev() {
        let tmp = TempTree::new();
        tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
        tmp.write_proc_stat(100, "firefox", 100, 0, 200);

        let mut state = ProcessState::default();
        // Prime the panel path so proc_prev_* is populated.
        read_top_process(&tmp.proc_root(), &mut state, clock_at(0)).check_none();
        // Page path: warm-starts from panel prev (t=0 → t=10, dt=10s).
        tmp.write_proc_stat(100, "firefox", 1100, 0, 200);
        // Bump jiffies by 1000 over 10s → 1000/100/10*100 = 100%.
        let first_page = read_top_process_page(&tmp.proc_root(), &mut state, clock_at(10));
        let row = first_page.expect("page result")[0].clone();
        assert_eq!(row.pid, 100);
        assert_eq!(row.cpu_percent, 100);
        // page_proc_prev_* now populated.
        assert!(state.page_proc_prev_times.contains_key(&100));

        // Second page call uses page_proc_prev_* (dt=10 again).
        tmp.write_proc_stat(100, "firefox", 2100, 0, 200);
        let second_page = read_top_process_page(&tmp.proc_root(), &mut state, clock_at(20));
        let row = second_page.expect("page result")[0].clone();
        assert_eq!(row.cpu_percent, 100);
    }

    #[test]
    fn read_top_process_page_resolves_cmdline_for_shown_rows_only() {
        let tmp = TempTree::new();
        tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
        // Many processes; page takes at most top_process_page_rows().
        for pid in 100u32..150 {
            tmp.write_proc_stat(pid, "comm", u64::from(pid) + 100, 0, 100);
        }
        let mut state = ProcessState::default();
        read_top_process(&tmp.proc_root(), &mut state, clock_at(0)).check_none();
        for pid in 100u32..150 {
            tmp.write_proc_stat(pid, "comm", u64::from(pid) + 600, 0, 100);
            tmp.write_cmdline(pid, &[&format!("/usr/bin/app{pid}"), "-arg"]);
        }

        let rows =
            read_top_process_page(&tmp.proc_root(), &mut state, clock_at(1)).expect("page result");

        assert_eq!(rows.len(), crate::page_commands::top_process_page_rows());
        // cmdline_name resolved (basename + arg), not the bare comm.
        for row in &rows {
            assert!(row.command.starts_with("app"));
            assert!(row.command.ends_with("-arg"));
        }
    }

    #[test]
    fn read_top_process_page_keeps_idle_rows_for_stable_height() {
        let tmp = TempTree::new();
        tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
        tmp.write_proc_stat(100, "idle", 100, 0, 100);

        let mut state = ProcessState::default();
        read_top_process(&tmp.proc_root(), &mut state, clock_at(0)).check_none();
        // No jiffies change → 0% cpu; page keeps idle rows.
        let rows = read_top_process_page(&tmp.proc_root(), &mut state, clock_at(1));
        let row = rows.expect("page keeps idle")[0].clone();
        assert_eq!(row.cpu_percent, 0);
    }

    /// Helper for the tests above: assert an `Option` is `None`.
    trait OptionNoneExt<T> {
        fn check_none(&self);
    }

    impl<T> OptionNoneExt<T> for Option<T> {
        fn check_none(&self) {
            assert!(self.is_none(), "expected None, got Some");
        }
    }
}
