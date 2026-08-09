//! Memory and swap readings from `/proc/meminfo`.
//!
//! RAM usage,
//! rounded used/total GiB values for the tooltip, swap usage, and bounded
//! shared memory history. The production Python code delegates to
//! `psutil.virtual_memory()` / `psutil.swap_memory()`. Here we mirror the
//! relevant Linux semantics directly from `/proc/meminfo` so the API stays
//! deterministic and fixture-friendly.

use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::config::{BRAILLE_LENGTH_MULTIPLIER, Config};
use crate::domain::boundary::ClockSnapshot;
use crate::domain::readings::RetainedMetricSample;

const BYTES_PER_KIB: u64 = 1024;
const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;
const DEFAULT_PAGE_SIZE: u64 = 4096;

/// Mutable memory-history state that persists between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryState {
    pub(super) usage: RetainedMetricSample<MemoryUsage>,
    pub(super) swap_usage: RetainedMetricSample<i32>,
    /// Shared memory-usage history for spark/braille/graphs.
    pub mem_history: Vec<i32>,
    /// Monotonic timestamp of the last history sample.
    pub mem_history_sample_at: Option<Duration>,
}

/// Point-in-time RAM usage derived from `/proc/meminfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryUsage {
    /// Visible usage percentage, matching `int(psutil.virtual_memory().percent)`.
    pub percent: i32,
    /// Tooltip `used` column in GiB, rounded like Python's `round()`.
    pub used_gib: u64,
    /// Tooltip `total` column in GiB, rounded like Python's `round()`.
    pub total_gib: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SwapUsageOutcome {
    Present(i32),
    Absent,
    Failed,
}

/// Reads total RAM bytes from `/proc/meminfo`.
///
/// Deterministic counterpart to Python's `_mem_total_bytes()` helper; longer-lived total-memory state belongs to the process owner.
#[must_use]
pub fn read_mem_total_bytes(proc_root: &Path) -> Option<u64> {
    load_meminfo(proc_root)?.mem_total
}

/// Reads RAM usage from `/proc/meminfo` and updates shared history.
///
/// Mirrors `src/sensors.py::_read_mem_usage`: percentage uses psutil's Linux
/// semantics (`used = total - available`, where `available` prefers
/// `MemAvailable:` and otherwise falls back to the procps-style estimate), and
/// the history buffer samples on `display.history_interval` with the longest
/// configured consumer deciding the retained length.
#[must_use]
pub fn read_memory_usage(
    proc_root: &Path,
    state: &mut MemoryState,
    cfg: &Config,
    clock: ClockSnapshot,
) -> Option<MemoryUsage> {
    let usage = read_memory_usage_once(proc_root)?;
    maybe_append_history(
        &mut state.mem_history,
        &mut state.mem_history_sample_at,
        clock.monotonic,
        history_interval(cfg),
        memory_history_len(cfg),
        usage.percent,
    );
    Some(usage)
}

/// Performs one memory source attempt without history cadence policy.
#[must_use]
pub(crate) fn read_memory_usage_once(proc_root: &Path) -> Option<MemoryUsage> {
    let meminfo = load_meminfo(proc_root)?;
    let total = meminfo.mem_total?;
    let free = meminfo.mem_free?;
    let available = resolved_available_bytes(&meminfo, proc_root, default_page_size());
    let available = clamp_available_bytes(available, total, free);
    let used = total.saturating_sub(available);
    let percent = rounded_percent_int(used, total);
    Some(MemoryUsage {
        percent,
        used_gib: round_half_even_div(used, BYTES_PER_GIB),
        total_gib: round_half_even_div(total, BYTES_PER_GIB),
    })
}

/// Appends one memory history point after synchronous collection marks it due.
pub(crate) fn append_memory_history(
    state: &mut MemoryState,
    cfg: &Config,
    captured_at: Duration,
    percent: i32,
) {
    state.mem_history_sample_at = Some(captured_at);
    state.mem_history.push(percent);
    trim_to_len(&mut state.mem_history, memory_history_len(cfg));
}

/// Reads swap usage from `/proc/meminfo`.
///
/// Mirrors `src/sensors.py::_read_swap_usage`: `None` when swap is absent and
/// otherwise the integer-truncated percentage derived from psutil's one-decimal
/// Linux percent calculation.
#[must_use]
pub fn read_swap_usage(proc_root: &Path) -> Option<i32> {
    match read_swap_usage_once(proc_root) {
        SwapUsageOutcome::Present(value) => Some(value),
        SwapUsageOutcome::Absent | SwapUsageOutcome::Failed => None,
    }
}

/// Performs one swap source attempt and distinguishes absence from failure.
#[must_use]
pub(crate) fn read_swap_usage_once(proc_root: &Path) -> SwapUsageOutcome {
    let Some(meminfo) = load_meminfo(proc_root) else {
        return SwapUsageOutcome::Failed;
    };
    let Some(total) = meminfo.swap_total else {
        return SwapUsageOutcome::Failed;
    };
    if total == 0 {
        return SwapUsageOutcome::Absent;
    }
    let Some(free) = meminfo.swap_free else {
        return SwapUsageOutcome::Failed;
    };
    let used = total.saturating_sub(free);
    SwapUsageOutcome::Present(rounded_percent_int(used, total))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct MemInfo {
    mem_total: Option<u64>,
    mem_free: Option<u64>,
    mem_available: Option<u64>,
    cached: Option<u64>,
    s_reclaimable: Option<u64>,
    active_file: Option<u64>,
    inactive_file: Option<u64>,
    swap_total: Option<u64>,
    swap_free: Option<u64>,
}

fn load_meminfo(proc_root: &Path) -> Option<MemInfo> {
    parse_meminfo(&fs::read_to_string(proc_root.join("meminfo")).ok()?)
}

fn parse_meminfo(text: &str) -> Option<MemInfo> {
    let mut meminfo = MemInfo::default();
    let mut saw_entry = false;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(key) = fields.next() else {
            continue;
        };
        let Some(value) = fields.next() else {
            continue;
        };
        let Ok(value_kib) = value.parse::<u64>() else {
            continue;
        };
        let value_bytes = value_kib.checked_mul(BYTES_PER_KIB)?;
        saw_entry = true;
        match key {
            "MemTotal:" => meminfo.mem_total = Some(value_bytes),
            "MemFree:" => meminfo.mem_free = Some(value_bytes),
            "MemAvailable:" => meminfo.mem_available = Some(value_bytes),
            "Cached:" => meminfo.cached = Some(value_bytes),
            "SReclaimable:" => meminfo.s_reclaimable = Some(value_bytes),
            "Active(file):" => meminfo.active_file = Some(value_bytes),
            "Inactive(file):" => meminfo.inactive_file = Some(value_bytes),
            "SwapTotal:" => meminfo.swap_total = Some(value_bytes),
            "SwapFree:" => meminfo.swap_free = Some(value_bytes),
            _ => {}
        }
    }
    saw_entry.then_some(meminfo)
}

fn resolved_available_bytes(meminfo: &MemInfo, proc_root: &Path, page_size: u64) -> i128 {
    match meminfo.mem_available {
        Some(0) | None => {
            let zoneinfo_path = proc_root.join("zoneinfo");
            calculate_available_bytes(meminfo, &zoneinfo_path, page_size)
        }
        Some(available) => i128::from(available),
    }
}

fn clamp_available_bytes(available: i128, total: u64, free: u64) -> u64 {
    if available < 0 {
        0
    } else {
        let available = available as u64;
        if available > total { free } else { available }
    }
}

fn calculate_available_bytes(meminfo: &MemInfo, zoneinfo_path: &Path, page_size: u64) -> i128 {
    let Some(free) = meminfo.mem_free else {
        return 0;
    };
    let fallback = i128::from(free.saturating_add(meminfo.cached.unwrap_or(0)));
    let (Some(active_file), Some(inactive_file), Some(s_reclaimable)) = (
        meminfo.active_file,
        meminfo.inactive_file,
        meminfo.s_reclaimable,
    ) else {
        return fallback;
    };
    let Ok(zoneinfo) = fs::read_to_string(zoneinfo_path) else {
        return fallback;
    };
    let Some(low_pages) = parse_zoneinfo_low_pages(&zoneinfo) else {
        return fallback;
    };
    let watermark_low = i128::from(low_pages.saturating_mul(page_size));
    let pagecache = i128::from(active_file.saturating_add(inactive_file));
    let reclaimable = i128::from(s_reclaimable);

    let numerator = i128::from(free)
        .saturating_mul(2)
        .saturating_sub(watermark_low.saturating_mul(2))
        .saturating_add(half_scaled_contribution(pagecache, watermark_low))
        .saturating_add(half_scaled_contribution(reclaimable, watermark_low));

    trunc_div2(numerator)
}

fn parse_zoneinfo_low_pages(text: &str) -> Option<u64> {
    let mut total = 0_u64;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(key) = fields.next() else {
            continue;
        };
        if key != "low" {
            continue;
        }
        let value = fields.next()?;
        let Ok(value) = value.parse::<u64>() else {
            return None;
        };
        total = total.saturating_add(value);
    }
    Some(total)
}

fn half_scaled_contribution(value: i128, watermark_low: i128) -> i128 {
    if value <= watermark_low.saturating_mul(2) {
        value
    } else {
        value
            .saturating_mul(2)
            .saturating_sub(watermark_low.saturating_mul(2))
    }
}

fn trunc_div2(value: i128) -> i128 {
    if value >= 0 {
        value / 2
    } else {
        -((-value) / 2)
    }
}

fn rounded_percent_int(used: u64, total: u64) -> i32 {
    if total == 0 {
        return 0;
    }
    let tenths = round_half_even_ratio(u128::from(used).saturating_mul(1000), u128::from(total));
    (tenths / 10) as i32
}

fn round_half_even_div(numerator: u64, denominator: u64) -> u64 {
    round_half_even_ratio(u128::from(numerator), u128::from(denominator)) as u64
}

fn round_half_even_ratio(numerator: u128, denominator: u128) -> u128 {
    if denominator == 0 {
        return 0;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let doubled = remainder.saturating_mul(2);
    if doubled > denominator {
        quotient.saturating_add(1)
    } else if doubled < denominator || quotient % 2 == 0 {
        quotient
    } else {
        quotient.saturating_add(1)
    }
}

fn history_interval(cfg: &Config) -> Duration {
    cfg.display.history_interval.duration()
}

fn memory_history_len(cfg: &Config) -> usize {
    let graph_len = if cfg.pages.order.iter().any(|page| page == "graphs") {
        cfg.pages.graph_history_length
    } else {
        0
    };
    [
        cfg.spark_panel.mem_spark_length,
        cfg.spark_tooltip.mem_spark_length,
        cfg.braille_panel
            .mem_braille_length
            .saturating_mul(BRAILLE_LENGTH_MULTIPLIER),
        cfg.braille_tooltip
            .mem_braille_length
            .saturating_mul(BRAILLE_LENGTH_MULTIPLIER),
        graph_len,
    ]
    .into_iter()
    .map(|value| value.max(0) as usize)
    .max()
    .unwrap_or(0)
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

fn default_page_size() -> u64 {
    DEFAULT_PAGE_SIZE
}

#[cfg(test)]
mod tests;
