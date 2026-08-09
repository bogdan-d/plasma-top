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
        let root = std::env::temp_dir().join(format!(
            "plasma-top-process-{}-{unique}",
            std::process::id()
        ));
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
fn panel_pid_reuse_skips_only_reused_pid_and_commits_current_baseline() {
    let tmp = TempTree::new();
    tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
    tmp.write_proc_stat(100, "old-process", 1000, 0, 100);
    tmp.write_proc_stat(200, "unaffected", 100, 0, 100);
    let mut state = ProcessState::default();
    assert_eq!(
        read_top_process_once(&tmp.proc_root(), &mut state, clock_at(0)),
        ProcessReadOutcome::Baseline
    );

    tmp.write_proc_stat(100, "reused-pid", 10, 0, 100);
    tmp.write_proc_stat(200, "unaffected", 1100, 0, 100);
    let rolled_back = read_top_process_once(&tmp.proc_root(), &mut state, clock_at(10));
    let ProcessReadOutcome::Rows(rolled_back) = rolled_back else {
        panic!("unaffected process should remain comparable");
    };
    assert_eq!(
        rolled_back.iter().map(|row| row.pid).collect::<Vec<_>>(),
        [200]
    );
    assert_eq!(state.proc_prev_times.get(&100), Some(&10));
    assert_eq!(state.proc_prev_times.get(&200), Some(&1100));

    tmp.write_proc_stat(100, "reused-pid", 110, 0, 100);
    tmp.write_proc_stat(200, "unaffected", 2100, 0, 100);
    let recovered = read_top_process_once(&tmp.proc_root(), &mut state, clock_at(20));
    let ProcessReadOutcome::Rows(recovered) = recovered else {
        panic!("committed baseline should make reused pid comparable");
    };
    assert!(recovered.iter().any(|row| row.pid == 100));
    assert!(recovered.iter().any(|row| row.pid == 200));
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

#[test]
fn process_page_pid_reuse_keeps_unaffected_rows_and_commits_current_baseline() {
    let tmp = TempTree::new();
    tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
    tmp.write_proc_stat(100, "old-process", 1000, 0, 100);
    tmp.write_proc_stat(200, "unaffected", 100, 0, 100);
    let mut state = ProcessState::default();
    let baseline = read_top_process_page_attempt(&tmp.proc_root(), &mut state, clock_at(0));
    assert_eq!(baseline.status, ProcessPageStatus::Baseline);

    tmp.write_proc_stat(100, "reused-pid", 10, 0, 100);
    tmp.write_proc_stat(200, "unaffected", 1100, 0, 100);
    let rolled_back = read_top_process_page_attempt(&tmp.proc_root(), &mut state, clock_at(10));
    assert_eq!(rolled_back.status, ProcessPageStatus::Captured);
    let rows = &rolled_back.sample.as_ref().expect("page rows").value;
    assert_eq!(rows.iter().map(|row| row.pid).collect::<Vec<_>>(), [200]);
    assert_eq!(state.page_proc_prev_times.get(&100), Some(&10));
    assert_eq!(state.page_proc_prev_times.get(&200), Some(&1100));

    tmp.write_proc_stat(100, "reused-pid", 110, 0, 100);
    tmp.write_proc_stat(200, "unaffected", 2100, 0, 100);
    let recovered = read_top_process_page_attempt(&tmp.proc_root(), &mut state, clock_at(20));
    assert_eq!(recovered.status, ProcessPageStatus::Captured);
    let rows = &recovered
        .sample
        .as_ref()
        .expect("recovered page rows")
        .value;
    assert!(rows.iter().any(|row| row.pid == 100));
    assert!(rows.iter().any(|row| row.pid == 200));
}

#[test]
fn process_page_failure_retains_rows_and_baseline_for_recovery() {
    let tmp = TempTree::new();
    tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
    tmp.write_proc_stat(100, "firefox", 100, 0, 100);
    let mut state = ProcessState::default();
    read_top_process(&tmp.proc_root(), &mut state, clock_at(0)).check_none();
    tmp.write_proc_stat(100, "firefox", 1100, 0, 100);

    let captured = read_top_process_page_attempt(&tmp.proc_root(), &mut state, clock_at(10));
    assert_eq!(captured.status, ProcessPageStatus::Captured);
    assert_eq!(
        captured
            .sample
            .as_ref()
            .map(|sample| sample.value[0].cpu_percent),
        Some(100)
    );

    let unavailable = tmp.root.join("proc-unavailable");
    fs::rename(tmp.proc_root(), &unavailable).expect("hide proc fixture");
    let failed = read_top_process_page_attempt(&tmp.proc_root(), &mut state, clock_at(20));

    assert_eq!(failed.status, ProcessPageStatus::Failed);
    assert_eq!(failed.failed_at, Some(Duration::from_secs(20)));
    assert_eq!(
        failed.sample.as_ref().map(|sample| sample.captured_at),
        Some(Duration::from_secs(10))
    );
    assert_eq!(
        state.page_proc_prev_sample_at,
        Some(Duration::from_secs(10))
    );
    assert_eq!(state.page_proc_prev_times.get(&100), Some(&1100));

    fs::rename(&unavailable, tmp.proc_root()).expect("restore proc fixture");
    tmp.write_proc_stat(100, "firefox", 3100, 0, 100);
    let recovered = read_top_process_page_attempt(&tmp.proc_root(), &mut state, clock_at(30));

    assert_eq!(recovered.status, ProcessPageStatus::Captured);
    assert_eq!(
        recovered
            .sample
            .as_ref()
            .map(|sample| sample.value[0].cpu_percent),
        Some(100),
        "failed scan must not shorten the recovery diff window"
    );
    assert_eq!(
        state.page_proc_prev_sample_at,
        Some(Duration::from_secs(30))
    );
}

#[test]
fn process_page_nonpositive_elapsed_retains_rows_and_baseline() {
    let tmp = TempTree::new();
    tmp.write_str("proc/meminfo", "MemTotal:        2097152 kB\n");
    tmp.write_proc_stat(100, "firefox", 100, 0, 100);
    let mut state = ProcessState::default();
    read_top_process(&tmp.proc_root(), &mut state, clock_at(0)).check_none();
    tmp.write_proc_stat(100, "firefox", 1100, 0, 100);
    let captured = read_top_process_page_attempt(&tmp.proc_root(), &mut state, clock_at(10));
    let sample = captured.sample.clone().expect("page sample");

    tmp.write_proc_stat(100, "firefox", 1200, 0, 100);
    let zero_elapsed = read_top_process_page_attempt(&tmp.proc_root(), &mut state, clock_at(10));
    assert_eq!(zero_elapsed.status, ProcessPageStatus::Baseline);
    assert_eq!(zero_elapsed.sample, Some(sample.clone()));
    assert_eq!(zero_elapsed.failed_at, None);
    assert_eq!(state.page_proc_prev_times.get(&100), Some(&1100));
    assert_eq!(
        state.page_proc_prev_sample_at,
        Some(Duration::from_secs(10))
    );

    tmp.write_proc_stat(100, "firefox", 3100, 0, 100);
    let recovered = read_top_process_page_attempt(&tmp.proc_root(), &mut state, clock_at(30));
    assert_eq!(recovered.status, ProcessPageStatus::Captured);
    assert_eq!(
        recovered.sample.map(|sample| sample.value[0].cpu_percent),
        Some(100),
        "nonpositive scan must not shorten the twenty-second recovery interval"
    );
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
