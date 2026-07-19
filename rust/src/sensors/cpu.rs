//! CPU discovery and `/proc`-based readings.
//!
//! This module ports the Wave 3 CPU lane from `src/sensors.py`: CPU usage via
//! `/proc/stat` diffs, per-core usage/history for the `cpu_cores` page,
//! uptime/load average, CPU frequency, and turbo detection/discovery. The API
//! is deterministic by construction: callers provide proc/sys roots and a
//! monotonic [`ClockSnapshot`] so tests
//! never touch the host filesystem or sleep.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::{BRAILLE_LENGTH_MULTIPLIER, Config, SensorOverrides};
use crate::domain::boundary::ClockSnapshot;

const CPU_TEMPERATURE_CHIPS: [&str; 3] = ["coretemp", "k10temp", "zenpower"];

/// CPU-related discovered sysfs paths and capability flags.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CpuPaths {
    /// Resolved hwmon temperature path, when a supported CPU sensor exists.
    pub cpu_temp_path: Option<PathBuf>,
    /// `cpu0` frequency path, when the cpufreq sysfs interface exists.
    pub cpu_freq_path: Option<PathBuf>,
    /// Whether either turbo/boost sysfs knob exists.
    pub cpu_turbo_supported: bool,
}

/// Mutable CPU diff/history state that persists between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CpuState {
    /// Previous aggregate `/proc/stat` counters.
    pub cpu_prev_times: Vec<u64>,
    /// Aggregate CPU-usage history shared by sparks/braille/graphs.
    pub cpu_history: Vec<i32>,
    /// Monotonic timestamp of the last aggregate history sample.
    pub cpu_history_sample_at: Option<Duration>,
    /// Previous per-core `/proc/stat` counters.
    pub cpu_core_prev_times: Vec<Vec<u64>>,
    /// Per-core CPU history for the `cpu_cores` page.
    pub cpu_core_history: Vec<Vec<i32>>,
    /// Monotonic timestamp of the last per-core history sample.
    pub cpu_core_history_sample_at: Option<Duration>,
}

/// Discovers the CPU lane's static sysfs paths under `sys_root`.
#[must_use]
pub fn discover_cpu_paths(sys_root: &Path, overrides: &SensorOverrides) -> CpuPaths {
    CpuPaths {
        cpu_temp_path: find_cpu_temp_path(sys_root, overrides),
        cpu_freq_path: find_cpu_freq_path(sys_root),
        cpu_turbo_supported: detect_cpu_turbo_supported(sys_root),
    }
}

/// Resolves the CPU temperature hwmon path, honoring manual overrides first.
#[must_use]
pub fn find_cpu_temp_path(sys_root: &Path, overrides: &SensorOverrides) -> Option<PathBuf> {
    if let Some(spec) = overrides.cpu_temp.as_deref() {
        return resolve_sensor_spec(sys_root, spec);
    }
    for chip in CPU_TEMPERATURE_CHIPS {
        for hwmon in hwmon_dirs_matching(sys_root, chip) {
            let path = hwmon.join("temp1_input");
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

/// Returns the `cpu0` cpufreq path when the kernel exposes it.
#[must_use]
pub fn find_cpu_freq_path(sys_root: &Path) -> Option<PathBuf> {
    let path = sys_root.join("devices/system/cpu/cpu0/cpufreq/scaling_cur_freq");
    path.exists().then_some(path)
}

/// Returns `true` when either turbo/boost sysfs knob exists.
#[must_use]
pub fn detect_cpu_turbo_supported(sys_root: &Path) -> bool {
    intel_pstate_path(sys_root).exists() || cpufreq_boost_path(sys_root).exists()
}

/// Reads aggregate CPU usage from `/proc/stat` and updates shared history.
///
/// Mirrors `src/sensors.py::_read_cpu_usage`: the first sample returns `0`,
/// later samples diff jiffies against the previous counters, cap the visible
/// percentage at `99`, and append into the shared history buffer only when the
/// configured history cadence elapses.
#[must_use]
pub fn read_cpu_usage(
    proc_root: &Path,
    state: &mut CpuState,
    cfg: &Config,
    clock: ClockSnapshot,
) -> i32 {
    let Ok(text) = fs::read_to_string(proc_root.join("stat")) else {
        return 0;
    };
    let Some(current) = parse_cpu_totals_line(&text) else {
        return 0;
    };

    let usage = usage_from_diff(&state.cpu_prev_times, &current);
    state.cpu_prev_times = current;
    maybe_append_history(
        &mut state.cpu_history,
        &mut state.cpu_history_sample_at,
        clock.monotonic,
        history_interval(cfg),
        aggregate_history_len(cfg),
        usage,
    );
    usage
}

/// Reads per-core CPU usage from `/proc/stat` and updates per-core histories.
///
/// Returns `None` on read/parse failure and otherwise one visible percentage
/// per `cpuN` line. When the core count changes, previous counters and history
/// buffers are reset to match Python's current behavior.
#[must_use]
pub fn read_cpu_cores(
    proc_root: &Path,
    state: &mut CpuState,
    cfg: &Config,
    clock: ClockSnapshot,
) -> Option<Vec<i32>> {
    let text = fs::read_to_string(proc_root.join("stat")).ok()?;
    let cores = parse_per_core_totals(&text)?;
    if cores.is_empty() {
        return None;
    }

    if state.cpu_core_prev_times.len() != cores.len() {
        state.cpu_core_prev_times = vec![Vec::new(); cores.len()];
        state.cpu_core_history = vec![Vec::new(); cores.len()];
    }

    let usage: Vec<i32> = cores
        .iter()
        .zip(state.cpu_core_prev_times.iter())
        .map(|(current, previous)| usage_from_diff(previous, current))
        .collect();

    for (slot, current) in state.cpu_core_prev_times.iter_mut().zip(cores) {
        *slot = current;
    }

    if history_due(
        &mut state.cpu_core_history_sample_at,
        clock.monotonic,
        history_interval(cfg),
    ) {
        let max_len = per_core_history_len(cfg);
        for (history, sample) in state.cpu_core_history.iter_mut().zip(&usage) {
            history.push(*sample);
            trim_to_len(history, max_len);
        }
    }

    Some(usage)
}

/// Reads system uptime from `/proc/uptime`, truncating fractional seconds.
#[must_use]
pub fn read_uptime_seconds(proc_root: &Path) -> Option<i64> {
    let text = fs::read_to_string(proc_root.join("uptime")).ok()?;
    let first = text.split_whitespace().next()?;
    let value = first.parse::<f64>().ok()?;
    Some(value as i64)
}

/// Reads the 1/5/15-minute load averages from `/proc/loadavg`.
#[must_use]
pub fn read_load_average(proc_root: &Path) -> Option<(f64, f64, f64)> {
    let text = fs::read_to_string(proc_root.join("loadavg")).ok()?;
    let mut parts = text.split_whitespace();
    let one = parts.next()?.parse::<f64>().ok()?;
    let five = parts.next()?.parse::<f64>().ok()?;
    let fifteen = parts.next()?.parse::<f64>().ok()?;
    Some((one, five, fifteen))
}

/// Reads CPU frequency in MHz from sysfs, falling back to `/proc/cpuinfo`.
///
/// The primary path matches Python's `cpu0/scaling_cur_freq` fast path. When
/// that path is absent, unreadable, or malformed, the reader falls back to the
/// first `cpu MHz` entry in `/proc/cpuinfo`.
#[must_use]
pub fn read_cpu_frequency_mhz(proc_root: &Path, freq_path: Option<&Path>) -> Option<f64> {
    if let Some(path) = freq_path {
        if let Ok(text) = fs::read_to_string(path) {
            if let Ok(khz) = text.trim().parse::<u64>() {
                return Some(khz as f64 / 1000.0);
            }
        }
    }
    let cpuinfo = fs::read_to_string(proc_root.join("cpuinfo")).ok()?;
    parse_cpuinfo_frequency_mhz(&cpuinfo)
}

/// Reads the current turbo/boost setting from sysfs.
///
/// `intel_pstate/no_turbo` is inverted (`0` means turbo enabled), while
/// `cpufreq/boost` uses the direct convention (`1` means enabled). Missing
/// paths return `None`; malformed contents mirror Python's equality check and
/// therefore evaluate to `Some(false)`.
#[must_use]
pub fn read_cpu_turbo(sys_root: &Path) -> Option<bool> {
    let intel = intel_pstate_path(sys_root);
    if intel.exists() {
        return fs::read_to_string(intel)
            .ok()
            .map(|text| text.trim() == "0");
    }
    let boost = cpufreq_boost_path(sys_root);
    if boost.exists() {
        return fs::read_to_string(boost)
            .ok()
            .map(|text| text.trim() == "1");
    }
    None
}

fn hwmon_dirs_matching(sys_root: &Path, chip_substr: &str) -> Vec<PathBuf> {
    let mut matches = Vec::new();
    let root = sys_root.join("class/hwmon");
    let Ok(entries) = fs::read_dir(root) else {
        return matches;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(name) = fs::read_to_string(path.join("name")) else {
            continue;
        };
        if name
            .trim()
            .to_ascii_lowercase()
            .contains(&chip_substr.to_ascii_lowercase())
        {
            matches.push(path);
        }
    }
    matches.sort();
    matches
}

fn resolve_sensor_spec(sys_root: &Path, spec: &str) -> Option<PathBuf> {
    let (chip, filename) = spec.split_once('|')?;
    for hwmon in hwmon_dirs_matching(sys_root, chip) {
        let path = hwmon.join(filename);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn intel_pstate_path(sys_root: &Path) -> PathBuf {
    sys_root.join("devices/system/cpu/intel_pstate/no_turbo")
}

fn cpufreq_boost_path(sys_root: &Path) -> PathBuf {
    sys_root.join("devices/system/cpu/cpufreq/boost")
}

fn parse_cpu_totals_line(text: &str) -> Option<Vec<u64>> {
    let line = text.lines().next()?;
    let mut parts = line.split_whitespace();
    if parts.next()? != "cpu" {
        return None;
    }
    let values: Option<Vec<u64>> = parts.map(|part| part.parse::<u64>().ok()).collect();
    let values = values?;
    (!values.is_empty()).then_some(values)
}

fn parse_per_core_totals(text: &str) -> Option<Vec<Vec<u64>>> {
    let mut cores = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(label) = parts.next() else {
            continue;
        };
        let Some(index) = label.strip_prefix("cpu") else {
            continue;
        };
        if index.is_empty() || !index.chars().all(|character| character.is_ascii_digit()) {
            continue;
        }
        let values: Option<Vec<u64>> = parts.map(|part| part.parse::<u64>().ok()).collect();
        let values = values?;
        if values.is_empty() {
            return None;
        }
        cores.push(values);
    }
    (!cores.is_empty()).then_some(cores)
}

fn usage_from_diff(previous: &[u64], current: &[u64]) -> i32 {
    if previous.is_empty() || previous.len() != current.len() || current.len() <= 4 {
        return 0;
    }
    let total_current: u64 = current.iter().sum();
    let total_previous: u64 = previous.iter().sum();
    let Some(total_delta) = total_current.checked_sub(total_previous) else {
        return 0;
    };
    if total_delta == 0 {
        return 0;
    }
    let idle_current = current[3].saturating_add(current[4]);
    let idle_previous = previous[3].saturating_add(previous[4]);
    let Some(idle_delta) = idle_current.checked_sub(idle_previous) else {
        return 0;
    };
    let used = 100_u64.saturating_sub(idle_delta.saturating_mul(100) / total_delta);
    used.min(99) as i32
}

fn history_interval(cfg: &Config) -> Duration {
    if cfg.display.history_interval <= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(cfg.display.history_interval)
    }
}

fn aggregate_history_len(cfg: &Config) -> usize {
    let graph_len = if cfg.pages.order.iter().any(|page| page == "graphs") {
        cfg.pages.graph_history_length
    } else {
        0
    };
    [
        cfg.spark_panel.cpu_spark_length,
        cfg.spark_tooltip.cpu_spark_length,
        cfg.braille_panel
            .cpu_braille_length
            .saturating_mul(BRAILLE_LENGTH_MULTIPLIER),
        cfg.braille_tooltip
            .cpu_braille_length
            .saturating_mul(BRAILLE_LENGTH_MULTIPLIER),
        graph_len,
    ]
    .into_iter()
    .map(|value| value.max(0) as usize)
    .max()
    .unwrap_or(0)
}

fn per_core_history_len(cfg: &Config) -> usize {
    let chars = cfg
        .braille_tooltip
        .cpu_braille_length
        .max(cfg.display.tooltip_width)
        .max(0) as usize;
    chars.saturating_mul(BRAILLE_LENGTH_MULTIPLIER as usize)
}

fn maybe_append_history(
    history: &mut Vec<i32>,
    last_sample_at: &mut Option<Duration>,
    now: Duration,
    interval: Duration,
    max_len: usize,
    sample: i32,
) {
    if history_due(last_sample_at, now, interval) {
        history.push(sample);
        trim_to_len(history, max_len);
    }
}

fn history_due(last_sample_at: &mut Option<Duration>, now: Duration, interval: Duration) -> bool {
    match last_sample_at {
        None => {
            *last_sample_at = Some(now);
            true
        }
        Some(previous) if now.saturating_sub(*previous) >= interval => {
            *previous = now;
            true
        }
        Some(_) => false,
    }
}

fn trim_to_len<T>(values: &mut Vec<T>, max_len: usize) {
    if max_len == 0 {
        values.clear();
        return;
    }
    if values.len() > max_len {
        let excess = values.len() - max_len;
        values.drain(..excess);
    }
}

fn parse_cpuinfo_frequency_mhz(text: &str) -> Option<f64> {
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if key.trim() == "cpu MHz" {
            return value.trim().parse::<f64>().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
    }

    fn fixture_proc() -> PathBuf {
        fixtures_root().join("proc")
    }

    fn fixture_sys() -> PathBuf {
        fixtures_root().join("sys")
    }

    fn clock_at(seconds: u64) -> ClockSnapshot {
        ClockSnapshot {
            monotonic: Duration::from_secs(seconds),
            wall: UNIX_EPOCH + Duration::from_secs(seconds),
        }
    }

    fn config_with_history_interval(seconds: f64) -> Config {
        let mut cfg = Config::default();
        cfg.display.history_interval = seconds;
        cfg
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
            let root =
                std::env::temp_dir().join(format!("pirostats-cpu-{}-{unique}", std::process::id()));
            if let Err(error) = fs::create_dir_all(&root) {
                panic!("failed to create temp root {}: {error}", root.display());
            }
            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn write(&self, relative: &str, content: &str) {
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
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn discover_cpu_paths_collects_all_cpu_discovery_outputs() {
        let tmp = TempTree::new();
        tmp.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
        tmp.write("sys/class/hwmon/hwmon0/temp1_input", "55000\n");
        tmp.write(
            "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
            "3200000\n",
        );
        tmp.write("sys/devices/system/cpu/intel_pstate/no_turbo", "0\n");

        let paths = discover_cpu_paths(&tmp.path().join("sys"), &SensorOverrides::default());

        assert_eq!(
            paths.cpu_temp_path,
            Some(tmp.path().join("sys/class/hwmon/hwmon0/temp1_input"))
        );
        assert_eq!(
            paths.cpu_freq_path,
            Some(
                tmp.path()
                    .join("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
            )
        );
        assert!(paths.cpu_turbo_supported);
    }

    #[test]
    fn find_cpu_temp_path_prefers_override_before_autodetect() {
        let tmp = TempTree::new();
        tmp.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
        tmp.write("sys/class/hwmon/hwmon0/temp1_input", "55000\n");
        tmp.write("sys/class/hwmon/hwmon1/name", "zenpower\n");
        tmp.write("sys/class/hwmon/hwmon1/temp3_input", "44000\n");

        let overrides = SensorOverrides {
            cpu_temp: Some(String::from("zenpower|temp3_input")),
            ..SensorOverrides::default()
        };

        let found = find_cpu_temp_path(&tmp.path().join("sys"), &overrides);

        assert_eq!(
            found,
            Some(tmp.path().join("sys/class/hwmon/hwmon1/temp3_input"))
        );
    }

    #[test]
    fn find_cpu_temp_path_autodetects_supported_fixture_chip() {
        let found = find_cpu_temp_path(&fixture_sys(), &SensorOverrides::default());

        assert_eq!(
            found,
            Some(fixture_sys().join("class/hwmon/hwmon0/temp1_input"))
        );
    }

    #[test]
    fn find_cpu_freq_path_and_turbo_support_follow_sysfs_presence() {
        let tmp = TempTree::new();
        assert_eq!(find_cpu_freq_path(&tmp.path().join("sys")), None);
        assert!(!detect_cpu_turbo_supported(&tmp.path().join("sys")));

        tmp.write(
            "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
            "3200000\n",
        );
        tmp.write("sys/devices/system/cpu/cpufreq/boost", "1\n");

        assert_eq!(
            find_cpu_freq_path(&tmp.path().join("sys")),
            Some(
                tmp.path()
                    .join("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
            )
        );
        assert!(detect_cpu_turbo_supported(&tmp.path().join("sys")));
    }

    #[test]
    fn read_cpu_usage_first_sample_is_zero_and_seeds_history() {
        let mut state = CpuState::default();
        let cfg = Config::default();

        let usage = read_cpu_usage(&fixture_proc(), &mut state, &cfg, clock_at(0));

        assert_eq!(usage, 0);
        assert_eq!(state.cpu_history, vec![0]);
        assert_eq!(state.cpu_prev_times.len(), 10);
    }

    #[test]
    fn read_cpu_usage_computes_delta_caps_at_ninety_nine_and_trims_history() {
        let tmp = TempTree::new();
        tmp.write("proc/stat", "cpu 10 0 10 80 0 0 0 0 0 0\n");

        let mut cfg = config_with_history_interval(1.0);
        cfg.spark_panel.cpu_spark_length = 1;
        cfg.spark_tooltip.cpu_spark_length = 1;
        cfg.braille_panel.cpu_braille_length = 1;
        cfg.braille_tooltip.cpu_braille_length = 1;
        cfg.pages.order = vec![String::from("graphs")];
        cfg.pages.graph_history_length = 2;

        let proc_root = tmp.path().join("proc");
        let mut state = CpuState::default();
        assert_eq!(read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(0)), 0);

        tmp.write("proc/stat", "cpu 50 0 50 80 0 0 0 0 0 0\n");
        assert_eq!(
            read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(2)),
            99
        );

        tmp.write("proc/stat", "cpu 51 0 51 80 0 0 0 0 0 0\n");
        assert_eq!(
            read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(4)),
            99
        );
        assert_eq!(state.cpu_history, vec![99, 99]);
    }

    #[test]
    fn read_cpu_usage_skips_history_until_interval_elapses_and_handles_reset() {
        let tmp = TempTree::new();
        tmp.write("proc/stat", "cpu 10 0 10 80 0 0 0 0 0 0\n");

        let cfg = config_with_history_interval(5.0);
        let proc_root = tmp.path().join("proc");
        let mut state = CpuState::default();
        assert_eq!(read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(0)), 0);

        tmp.write("proc/stat", "cpu 30 0 20 90 0 0 0 0 0 0\n");
        assert_eq!(
            read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(1)),
            75
        );
        assert_eq!(state.cpu_history, vec![0]);

        tmp.write("proc/stat", "cpu 1 0 1 8 0 0 0 0 0 0\n");
        assert_eq!(read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(6)), 0);
        assert_eq!(state.cpu_history, vec![0, 0]);
    }

    #[test]
    fn read_cpu_cores_reads_fixture_rows_and_tracks_history() {
        let mut cfg = Config::default();
        cfg.display.history_interval = 1.0;
        cfg.braille_tooltip.cpu_braille_length = 2;
        cfg.display.tooltip_width = 3;

        let mut state = CpuState::default();
        let first = read_cpu_cores(&fixture_proc(), &mut state, &cfg, clock_at(0));

        assert_eq!(first, Some(vec![0; 8]));
        assert_eq!(state.cpu_core_history.len(), 8);
        assert!(state.cpu_core_history.iter().all(|history| history == &[0]));
    }

    #[test]
    fn read_cpu_cores_resets_on_core_count_change() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/stat",
            "cpu 20 0 20 160 0 0 0 0 0 0\n\
             cpu0 10 0 10 80 0 0 0 0 0 0\n\
             cpu1 10 0 10 80 0 0 0 0 0 0\n",
        );

        let cfg = config_with_history_interval(1.0);
        let proc_root = tmp.path().join("proc");
        let mut state = CpuState::default();
        assert_eq!(
            read_cpu_cores(&proc_root, &mut state, &cfg, clock_at(0)),
            Some(vec![0, 0])
        );

        tmp.write(
            "proc/stat",
            "cpu 60 0 40 180 0 0 0 0 0 0\n\
             cpu0 30 0 20 90 0 0 0 0 0 0\n\
             cpu1 30 0 20 90 0 0 0 0 0 0\n",
        );
        assert_eq!(
            read_cpu_cores(&proc_root, &mut state, &cfg, clock_at(2)),
            Some(vec![75, 75])
        );

        tmp.write(
            "proc/stat",
            "cpu 90 0 60 210 0 0 0 0 0 0\n\
             cpu0 30 0 20 90 0 0 0 0 0 0\n\
             cpu1 30 0 20 90 0 0 0 0 0 0\n\
             cpu2 30 0 20 90 0 0 0 0 0 0\n",
        );
        assert_eq!(
            read_cpu_cores(&proc_root, &mut state, &cfg, clock_at(4)),
            Some(vec![0, 0, 0])
        );
        assert_eq!(state.cpu_core_history.len(), 3);
        assert!(state.cpu_core_history.iter().all(|history| history == &[0]));
    }

    #[test]
    fn read_cpu_cores_returns_none_for_malformed_stat() {
        let tmp = TempTree::new();
        tmp.write("proc/stat", "cpu x y z\n");

        let mut state = CpuState::default();
        assert_eq!(
            read_cpu_cores(
                &tmp.path().join("proc"),
                &mut state,
                &Config::default(),
                clock_at(0)
            ),
            None
        );
        assert!(state.cpu_core_prev_times.is_empty());
    }

    #[test]
    fn read_uptime_and_load_average_parse_fixture_proc_files() {
        assert_eq!(read_uptime_seconds(&fixture_proc()), Some(12345));
        assert_eq!(read_load_average(&fixture_proc()), Some((1.20, 0.90, 0.70)));
    }

    #[test]
    fn read_uptime_and_load_average_return_none_for_malformed_files() {
        let tmp = TempTree::new();
        tmp.write("proc/uptime", "nope\n");
        tmp.write("proc/loadavg", "1.0 broken\n");

        assert_eq!(read_uptime_seconds(&tmp.path().join("proc")), None);
        assert_eq!(read_load_average(&tmp.path().join("proc")), None);
    }

    #[test]
    fn read_cpu_frequency_prefers_sysfs_and_falls_back_to_cpuinfo() {
        let freq_path = fixture_sys().join("devices/system/cpu/cpu0/cpufreq/scaling_cur_freq");
        assert_eq!(
            read_cpu_frequency_mhz(&fixture_proc(), Some(&freq_path)),
            Some(3200.0)
        );

        let tmp = TempTree::new();
        tmp.write(
            "proc/cpuinfo",
            &fs::read_to_string(fixture_proc().join("cpuinfo")).unwrap_or_default(),
        );
        tmp.write(
            "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
            "bogus\n",
        );
        assert_eq!(
            read_cpu_frequency_mhz(
                &tmp.path().join("proc"),
                Some(
                    &tmp.path()
                        .join("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
                )
            ),
            Some(2195.104)
        );
    }

    #[test]
    fn read_cpu_turbo_handles_inversion_and_boost_fallback() {
        assert_eq!(read_cpu_turbo(&fixture_sys()), Some(true));

        let tmp = TempTree::new();
        tmp.write("sys/devices/system/cpu/cpufreq/boost", "1\n");
        assert_eq!(read_cpu_turbo(&tmp.path().join("sys")), Some(true));

        tmp.write("sys/devices/system/cpu/cpufreq/boost", "bogus\n");
        assert_eq!(read_cpu_turbo(&tmp.path().join("sys")), Some(false));
    }
}
