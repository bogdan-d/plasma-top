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
    cfg.display.history_interval =
        crate::domain::Cadence::from_millis(Duration::from_secs_f64(seconds).as_millis() as u64);
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
            std::env::temp_dir().join(format!("plasma-top-memory-{}-{unique}", std::process::id()));
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
