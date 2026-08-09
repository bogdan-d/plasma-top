//! CPU discovery and `/proc`-based readings.
//!
//! CPU usage via
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
use crate::domain::readings::{LoadAverage, RetainedMetricSample};

const CPU_TEMPERATURE_CHIPS: [&str; 3] = ["coretemp", "k10temp", "zenpower"];

/// CPU-related discovered sysfs paths and capability flags.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CpuPaths {
    /// Resolved hwmon temperature path, when a supported CPU sensor exists.
    pub cpu_temp_path: Option<PathBuf>,
    /// `cpu0` frequency path, when the cpufreq sysfs interface exists.
    pub cpu_freq_path: Option<PathBuf>,
    /// Active turbo/boost control file, when one exists.
    pub cpu_turbo_path: Option<PathBuf>,
    /// Whether either turbo/boost sysfs knob exists.
    pub cpu_turbo_supported: bool,
}

/// Mutable CPU diff/history state that persists between polls.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CpuState {
    pub(super) usage: RetainedMetricSample<i32>,
    pub(super) core_usage: RetainedMetricSample<Vec<i32>>,
    pub(super) temperature: RetainedMetricSample<i32>,
    pub(super) temperature_source: Option<PathBuf>,
    pub(super) frequency_mhz: RetainedMetricSample<f64>,
    pub(super) frequency_source: Option<PathBuf>,
    pub(super) turbo: RetainedMetricSample<bool>,
    pub(super) turbo_source: Option<PathBuf>,
    pub(super) uptime_seconds: RetainedMetricSample<i64>,
    pub(super) load_average: RetainedMetricSample<LoadAverage>,
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

impl CpuState {
    pub(crate) fn reconcile_core_page(&mut self, configured: bool) {
        if configured {
            return;
        }
        self.core_usage.invalidate();
        self.cpu_core_prev_times.clear();
        self.cpu_core_history.clear();
        self.cpu_core_history_sample_at = None;
    }
}

/// Discovers the CPU lane's static sysfs paths under `sys_root`.
#[must_use]
pub fn discover_cpu_paths(sys_root: &Path, overrides: &SensorOverrides) -> CpuPaths {
    let cpu_turbo_path = find_cpu_turbo_path(sys_root);
    CpuPaths {
        cpu_temp_path: find_cpu_temp_path(sys_root, overrides),
        cpu_freq_path: find_cpu_freq_path(sys_root),
        cpu_turbo_supported: cpu_turbo_path.is_some(),
        cpu_turbo_path,
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
    find_cpu_turbo_path(sys_root).is_some()
}

/// Returns the preferred turbo/boost control path exposed by the kernel.
#[must_use]
pub fn find_cpu_turbo_path(sys_root: &Path) -> Option<PathBuf> {
    let intel = intel_pstate_path(sys_root);
    if intel.exists() {
        return Some(intel);
    }
    let boost = cpufreq_boost_path(sys_root);
    boost.exists().then_some(boost)
}

/// Reads aggregate CPU usage from `/proc/stat` and updates shared history.
///
/// The first counter read establishes a baseline and returns `None`. Later comparable reads cap
/// the visible percentage at `99` and append into the shared history buffer only when the
/// configured history cadence elapses.
#[must_use]
pub fn read_cpu_usage(
    proc_root: &Path,
    state: &mut CpuState,
    cfg: &Config,
    clock: ClockSnapshot,
) -> Option<i32> {
    let CpuUsageReadOutcome::Value(usage) = read_cpu_usage_once(proc_root, state) else {
        return None;
    };
    maybe_append_history(
        &mut state.cpu_history,
        &mut state.cpu_history_sample_at,
        clock.monotonic,
        history_interval(cfg),
        aggregate_history_len(cfg),
        usage,
    );
    Some(usage)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CpuUsageReadOutcome {
    Value(i32),
    Baseline,
    InvalidDelta,
    Failed,
}

/// Performs one aggregate CPU counter attempt without history cadence policy.
#[must_use]
pub(crate) fn read_cpu_usage_once(proc_root: &Path, state: &mut CpuState) -> CpuUsageReadOutcome {
    let Ok(text) = fs::read_to_string(proc_root.join("stat")) else {
        return CpuUsageReadOutcome::Failed;
    };
    let Some(current) = parse_cpu_totals_line(&text) else {
        return CpuUsageReadOutcome::Failed;
    };

    if state.cpu_prev_times.is_empty() || state.cpu_prev_times.len() != current.len() {
        state.cpu_prev_times = current;
        state.cpu_history.clear();
        state.cpu_history_sample_at = None;
        return CpuUsageReadOutcome::Baseline;
    }

    let usage = usage_from_diff(&state.cpu_prev_times, &current);
    state.cpu_prev_times = current;
    usage.map_or(
        CpuUsageReadOutcome::InvalidDelta,
        CpuUsageReadOutcome::Value,
    )
}

/// Appends one aggregate CPU history point after synchronous collection marks it due.
pub(crate) fn append_cpu_history(
    state: &mut CpuState,
    cfg: &Config,
    captured_at: Duration,
    usage: i32,
) {
    state.cpu_history_sample_at = Some(captured_at);
    state.cpu_history.push(usage);
    trim_to_len(&mut state.cpu_history, aggregate_history_len(cfg));
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
    let CpuCoreReadOutcome::Value(usage) = read_cpu_cores_once(proc_root, state) else {
        return None;
    };
    if history_due(
        &mut state.cpu_core_history_sample_at,
        clock.monotonic,
        history_interval(cfg),
    ) {
        append_cpu_core_history(state, cfg, clock.monotonic, &usage);
    }
    Some(usage)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CpuCoreReadOutcome {
    Value(Vec<i32>),
    Baseline,
    InvalidDelta,
    Failed,
}

/// Performs one per-core CPU counter attempt without history cadence policy.
#[must_use]
pub(crate) fn read_cpu_cores_once(proc_root: &Path, state: &mut CpuState) -> CpuCoreReadOutcome {
    let Ok(text) = fs::read_to_string(proc_root.join("stat")) else {
        return CpuCoreReadOutcome::Failed;
    };
    let Some(cores) = parse_per_core_totals(&text) else {
        return CpuCoreReadOutcome::Failed;
    };
    if cores.is_empty() {
        return CpuCoreReadOutcome::Failed;
    }

    let same_shape = state.cpu_core_prev_times.len() == cores.len()
        && state
            .cpu_core_prev_times
            .iter()
            .zip(&cores)
            .all(|(previous, current)| !previous.is_empty() && previous.len() == current.len());
    if !same_shape {
        state.cpu_core_prev_times = cores;
        state.cpu_core_history = vec![Vec::new(); state.cpu_core_prev_times.len()];
        state.cpu_core_history_sample_at = None;
        return CpuCoreReadOutcome::Baseline;
    }

    let usage: Option<Vec<i32>> = cores
        .iter()
        .zip(state.cpu_core_prev_times.iter())
        .map(|(current, previous)| usage_from_diff(previous, current))
        .collect();
    state.cpu_core_prev_times = cores;
    usage.map_or(CpuCoreReadOutcome::InvalidDelta, CpuCoreReadOutcome::Value)
}

/// Appends per-core history after synchronous collection marks it due.
pub(crate) fn append_cpu_core_history(
    state: &mut CpuState,
    cfg: &Config,
    captured_at: Duration,
    usage: &[i32],
) {
    state.cpu_core_history_sample_at = Some(captured_at);
    let max_len = per_core_history_len(cfg);
    for (history, sample) in state.cpu_core_history.iter_mut().zip(usage) {
        history.push(*sample);
        trim_to_len(history, max_len);
    }
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
    read_cpu_turbo_path(find_cpu_turbo_path(sys_root).as_deref())
}

/// Reads one discovered turbo/boost control path.
#[must_use]
pub fn read_cpu_turbo_path(path: Option<&Path>) -> Option<bool> {
    let path = path?;
    let enabled_value = if path.file_name().is_some_and(|name| name == "no_turbo") {
        "0"
    } else {
        "1"
    };
    fs::read_to_string(path)
        .ok()
        .map(|text| text.trim() == enabled_value)
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

fn usage_from_diff(previous: &[u64], current: &[u64]) -> Option<i32> {
    if previous.is_empty() || previous.len() != current.len() || current.len() <= 4 {
        return None;
    }
    let total_current = current
        .iter()
        .try_fold(0_u64, |sum, value| sum.checked_add(*value))?;
    let total_previous = previous
        .iter()
        .try_fold(0_u64, |sum, value| sum.checked_add(*value))?;
    let total_delta = total_current.checked_sub(total_previous)?;
    if total_delta == 0 {
        return None;
    }
    let idle_current = current[3].checked_add(current[4])?;
    let idle_previous = previous[3].checked_add(previous[4])?;
    let idle_delta = idle_current.checked_sub(idle_previous)?;
    if idle_delta > total_delta {
        return None;
    }
    let idle_percent = idle_delta.checked_mul(100)? / total_delta;
    Some(100_u64.saturating_sub(idle_percent).min(99) as i32)
}

fn history_interval(cfg: &Config) -> Duration {
    cfg.display.history_interval.duration()
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
mod tests;
