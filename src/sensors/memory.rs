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

const BYTES_PER_KIB: u64 = 1024;
const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;
const DEFAULT_PAGE_SIZE: u64 = 4096;

/// Mutable memory-history state that persists between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MemoryState {
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

/// Reads total RAM bytes from `/proc/meminfo`.
///
/// Deterministic counterpart to Python's `_mem_total_bytes()` helper, without a
/// global process cache. The collector owns any longer-lived value.
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
    let meminfo = load_meminfo(proc_root)?;
    let total = meminfo.mem_total?;
    let free = meminfo.mem_free?;
    let available = resolved_available_bytes(&meminfo, proc_root, default_page_size());
    let available = clamp_available_bytes(available, total, free);
    let used = total.saturating_sub(available);
    let percent = rounded_percent_int(used, total);
    let usage = MemoryUsage {
        percent,
        used_gib: round_half_even_div(used, BYTES_PER_GIB),
        total_gib: round_half_even_div(total, BYTES_PER_GIB),
    };

    maybe_append_history(
        &mut state.mem_history,
        &mut state.mem_history_sample_at,
        clock.monotonic,
        history_interval(cfg),
        memory_history_len(cfg),
        percent,
    );

    Some(usage)
}

/// Reads swap usage from `/proc/meminfo`.
///
/// Mirrors `src/sensors.py::_read_swap_usage`: `None` when swap is absent and
/// otherwise the integer-truncated percentage derived from psutil's one-decimal
/// Linux percent calculation.
#[must_use]
pub fn read_swap_usage(proc_root: &Path) -> Option<i32> {
    let meminfo = load_meminfo(proc_root)?;
    let total = meminfo.swap_total?;
    if total == 0 {
        return None;
    }
    let free = meminfo.swap_free?;
    let used = total.saturating_sub(free);
    Some(rounded_percent_int(used, total))
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
    if cfg.display.history_interval <= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(cfg.display.history_interval)
    }
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
mod tests {
    use super::*;

    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

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
            let root = std::env::temp_dir()
                .join(format!("plasma-top-memory-{}-{unique}", std::process::id()));
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
    fn read_memory_usage_prefers_memavailable_and_seeds_history() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/meminfo",
            "MemTotal:       8388608 kB\n\
             MemFree:        1048576 kB\n\
             MemAvailable:   6291456 kB\n\
             Cached:         1572864 kB\n\
             SReclaimable:    262144 kB\n\
             Active(file):    786432 kB\n\
             Inactive(file):  786432 kB\n\
             SwapTotal:      2097152 kB\n\
             SwapFree:       1048576 kB\n",
        );

        let mut state = MemoryState::default();
        let usage = read_memory_usage(
            &tmp.path().join("proc"),
            &mut state,
            &Config::default(),
            clock_at(0),
        );

        assert_eq!(
            usage,
            Some(MemoryUsage {
                percent: 25,
                used_gib: 2,
                total_gib: 8,
            })
        );
        assert_eq!(state.mem_history, vec![25]);
    }

    #[test]
    fn read_mem_total_bytes_reads_total_from_meminfo() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/meminfo",
            "MemTotal:        6291456 kB\n\
             MemFree:          524288 kB\n",
        );

        assert_eq!(
            read_mem_total_bytes(&tmp.path().join("proc")),
            Some(6 * BYTES_PER_GIB)
        );
    }

    #[test]
    fn read_mem_total_bytes_returns_none_when_total_is_missing() {
        let tmp = TempTree::new();
        tmp.write("proc/meminfo", "MemFree: 1024 kB\n");

        assert_eq!(read_mem_total_bytes(&tmp.path().join("proc")), None);
    }

    #[test]
    fn read_memory_usage_uses_procps_fallback_when_memavailable_missing() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/meminfo",
            "MemTotal:       10485760 kB\n\
             MemFree:         1048576 kB\n\
             Cached:          2097152 kB\n\
             SReclaimable:     524288 kB\n\
             Active(file):    1048576 kB\n\
             Inactive(file):  1048576 kB\n\
             SwapTotal:       2097152 kB\n\
             SwapFree:        1048576 kB\n",
        );
        tmp.write(
            "proc/zoneinfo",
            "Node 0, zone      DMA\n\
                   low      1024\n\
             Node 0, zone    DMA32\n\
                   low      1024\n",
        );

        let usage = read_memory_usage(
            &tmp.path().join("proc"),
            &mut MemoryState::default(),
            &Config::default(),
            clock_at(0),
        );

        assert_eq!(
            usage,
            Some(MemoryUsage {
                percent: 65,
                used_gib: 7,
                total_gib: 10,
            })
        );
    }

    #[test]
    fn read_memory_usage_uses_fallback_when_memavailable_is_zero() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/meminfo",
            "MemTotal:        6291456 kB\n\
             MemFree:          524288 kB\n\
             MemAvailable:          0 kB\n\
             Cached:          1048576 kB\n\
             SReclaimable:     262144 kB\n\
             Active(file):     524288 kB\n\
             Inactive(file):   524288 kB\n",
        );
        tmp.write("proc/zoneinfo", "low 512\nlow 512\n");

        let usage = read_memory_usage(
            &tmp.path().join("proc"),
            &mut MemoryState::default(),
            &Config::default(),
            clock_at(0),
        );

        assert_eq!(
            usage,
            Some(MemoryUsage {
                percent: 71,
                used_gib: 4,
                total_gib: 6,
            })
        );
    }

    #[test]
    fn read_memory_usage_clamps_available_over_total_back_to_free() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/meminfo",
            "MemTotal:        2097152 kB\n\
             MemFree:          524288 kB\n\
             MemAvailable:    3145728 kB\n",
        );

        let usage = read_memory_usage(
            &tmp.path().join("proc"),
            &mut MemoryState::default(),
            &Config::default(),
            clock_at(0),
        );

        assert_eq!(
            usage,
            Some(MemoryUsage {
                percent: 75,
                used_gib: 2,
                total_gib: 2,
            })
        );
    }

    #[test]
    fn read_memory_usage_falls_back_to_free_plus_cached_without_zoneinfo_inputs() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/meminfo",
            "MemTotal:        4194304 kB\n\
             MemFree:         1048576 kB\n\
             Cached:           524288 kB\n\
             MemAvailable:          0 kB\n",
        );

        let usage = read_memory_usage(
            &tmp.path().join("proc"),
            &mut MemoryState::default(),
            &Config::default(),
            clock_at(0),
        );

        assert_eq!(
            usage,
            Some(MemoryUsage {
                percent: 62,
                used_gib: 2,
                total_gib: 4,
            })
        );
    }

    #[test]
    fn read_memory_usage_respects_history_interval_and_trims_to_largest_consumer() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/meminfo",
            "MemTotal:        4194304 kB\n\
             MemFree:         1048576 kB\n\
             MemAvailable:    3145728 kB\n",
        );

        let mut cfg = config_with_history_interval(2.0);
        cfg.spark_panel.mem_spark_length = 1;
        cfg.spark_tooltip.mem_spark_length = 1;
        cfg.braille_panel.mem_braille_length = 1;
        cfg.braille_tooltip.mem_braille_length = 1;
        cfg.pages.order = vec![String::from("graphs")];
        cfg.pages.graph_history_length = 2;

        let proc_root = tmp.path().join("proc");
        let mut state = MemoryState::default();
        let first = read_memory_usage(&proc_root, &mut state, &cfg, clock_at(0));
        assert_eq!(first.map(|usage| usage.percent), Some(25));

        tmp.write(
            "proc/meminfo",
            "MemTotal:        4194304 kB\n\
             MemFree:         1048576 kB\n\
             MemAvailable:    2097152 kB\n",
        );
        let second = read_memory_usage(&proc_root, &mut state, &cfg, clock_at(1));
        assert_eq!(second.map(|usage| usage.percent), Some(50));
        assert_eq!(state.mem_history, vec![25]);

        tmp.write(
            "proc/meminfo",
            "MemTotal:        4194304 kB\n\
             MemFree:         1048576 kB\n\
             MemAvailable:    1048576 kB\n",
        );
        let third = read_memory_usage(&proc_root, &mut state, &cfg, clock_at(2));
        assert_eq!(third.map(|usage| usage.percent), Some(75));
        assert_eq!(state.mem_history, vec![25, 75]);

        tmp.write(
            "proc/meminfo",
            "MemTotal:        4194304 kB\n\
             MemFree:          524288 kB\n\
             MemAvailable:     524288 kB\n",
        );
        let fourth = read_memory_usage(&proc_root, &mut state, &cfg, clock_at(4));
        assert_eq!(fourth.map(|usage| usage.percent), Some(87));
        assert_eq!(state.mem_history, vec![75, 87]);
    }

    #[test]
    fn read_memory_usage_returns_none_for_malformed_or_missing_meminfo() {
        let tmp = TempTree::new();
        tmp.write("proc/meminfo", "not meminfo\n");

        assert_eq!(
            read_memory_usage(
                &tmp.path().join("proc"),
                &mut MemoryState::default(),
                &Config::default(),
                clock_at(0)
            ),
            None
        );

        assert_eq!(
            read_memory_usage(
                &tmp.path().join("missing"),
                &mut MemoryState::default(),
                &Config::default(),
                clock_at(0)
            ),
            None
        );
    }

    #[test]
    fn read_swap_usage_returns_none_when_swap_is_absent() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/meminfo",
            "MemTotal:        4194304 kB\n\
             MemFree:         1048576 kB\n\
             MemAvailable:    3145728 kB\n\
             SwapTotal:             0 kB\n\
             SwapFree:              0 kB\n",
        );

        assert_eq!(read_swap_usage(&tmp.path().join("proc")), None);
    }

    #[test]
    fn read_swap_usage_computes_percent_from_meminfo() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/meminfo",
            "MemTotal:        4194304 kB\n\
             MemFree:         1048576 kB\n\
             MemAvailable:    3145728 kB\n\
             SwapTotal:       1048576 kB\n\
             SwapFree:         262144 kB\n",
        );

        assert_eq!(read_swap_usage(&tmp.path().join("proc")), Some(75));
    }

    #[test]
    fn round_half_even_matches_python_style_ties() {
        assert_eq!(round_half_even_div(5, 2), 2);
        assert_eq!(round_half_even_div(3, 2), 2);
        assert_eq!(round_half_even_div(BYTES_PER_GIB * 5, 2 * BYTES_PER_GIB), 2);
        assert_eq!(rounded_percent_int(9995, 10_000), 100);
        assert_eq!(rounded_percent_int(9945, 10_000), 99);
    }
}
