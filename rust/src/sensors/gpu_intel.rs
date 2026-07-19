//! Intel iGPU detection and DRM fdinfo-based usage readings.
//!
//! Ports the Intel-owned half of `src/sensors.py` (the PROCESS lane):
//!
//! - [`detect_intel_gpu`] walks `/sys/class/drm/card[0-9]*` for a vendor
//!   `0x8086` / display-class card and returns the gt frequency sysfs path
//!   plus the PCI address used to attribute fdinfo counters.
//! - [`read_intel_gpu_engine_times`] scans `/proc/*/fd/*` for DRM client fds
//!   and reads their `drm-engine-*` ns counters, keyed by `drm-client-id`.
//! - [`read_intel_gpu_metrics`] diffs two snapshots into per-engine
//!   utilization percentages (capped at 99), summed across clients.
//! - [`read_intel_gpu_metrics_cached`] adds the 30s TTL the panel relies on.
//!
//! All readers take explicit proc/sys roots and clock snapshots so tests never
//! touch the host filesystem or sleep. Symlink creation under tests uses
//! [`std::os::unix::fs::symlink`], which is safe Rust on Unix.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::domain::boundary::ClockSnapshot;

/// Intel DRM engine names tracked for the panel/graphs page.
pub const INTEL_GPU_ENGINES: &[&str] = &["render", "copy", "video", "video-enhance"];
/// TTL for the cached Intel GPU usage reading — matches hd_temp/fan_speed.
pub const INTEL_GPU_USAGE_TTL: Duration = Duration::from_secs(30);

/// Intel iGPU discovery result: matches Python's `_detect_intel_gpu` dict.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntelGpuPaths {
    /// `/sys/class/drm/cardN/gt_act_freq_mhz` path when it exists.
    pub freq_path: Option<PathBuf>,
    /// PCI address (e.g. `0000:00:02.0`) used to match fdinfo's `drm-pdev:`.
    pub pci: Option<String>,
}

/// Per-engine utilization percentages keyed by engine name.
pub type IntelGpuMetrics = BTreeMap<String, i32>;

/// Mutable Intel GPU diff/cache state that persists between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntelGpuState {
    /// Previous per-client engine ns counters keyed by DRM client id.
    pub engine_prev: BTreeMap<u32, BTreeMap<String, u64>>,
    /// Monotonic instant of the previous sample.
    pub prev_sample_at: Option<Duration>,
    /// TTL-cached per-engine utilization percentages.
    pub usage_cache: IntelGpuMetrics,
    /// Monotonic instant of the cached sample.
    pub usage_cache_sample_at: Option<Duration>,
}

/// Detects the first Intel iGPU DRM card under `sys_root`.
///
/// Mirrors `src/sensors.py::_detect_intel_gpu`: a card qualifies when its
/// `device/vendor` reads `0x8086` and `device/class` starts with `0x03`
/// (display). The PCI address comes from resolving the `device` symlink (its
/// basename matches the `drm-pdev:` fdinfo field). When `gt_act_freq_mhz`
/// exists on the card it's exposed; otherwise `freq_path` is `None` but `pci`
/// is still returned so the fdinfo path remains usable.
#[must_use]
pub fn detect_intel_gpu(sys_root: &Path) -> IntelGpuPaths {
    let Some(cards) = list_intel_cards(sys_root) else {
        return IntelGpuPaths::default();
    };
    for card in cards {
        let device = card.join("device");
        let Ok(vendor) = fs::read_to_string(device.join("vendor")) else {
            continue;
        };
        if vendor.trim() != "0x8086" {
            continue;
        }
        let Ok(class) = fs::read_to_string(device.join("class")) else {
            continue;
        };
        if !class.trim().starts_with("0x03") {
            continue;
        }
        let freq_path = card.join("gt_act_freq_mhz");
        let freq_path = freq_path.exists().then_some(freq_path);
        // Resolve the device symlink (e.g. ../../devices/.../0000:00:02.0)
        // and take its basename — matches Python's `device.resolve().name`.
        let pci = fs::canonicalize(&device).ok().and_then(|resolved| {
            resolved
                .file_name()
                .and_then(|name| name.to_str())
                .map(ToString::to_string)
        });
        return IntelGpuPaths { freq_path, pci };
    }
    IntelGpuPaths::default()
}

/// Returns the sorted `card[0-9]*` entries under `/sys/class/drm`, or `None`
/// when the directory cannot be read.
fn list_intel_cards(sys_root: &Path) -> Option<Vec<PathBuf>> {
    let drm = sys_root.join("class/drm");
    let Ok(entries) = fs::read_dir(&drm) else {
        return None;
    };
    let mut cards: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| {
                    name.starts_with("card")
                        && name.len() > 4
                        && name[4..].chars().all(|c| c.is_ascii_digit())
                })
                .unwrap_or(false)
        })
        .collect();
    cards.sort();
    Some(cards)
}

/// Scans `/proc/*/fd/*` for DRM client fds and reads their `drm-engine-*` ns.
///
/// Mirrors `src/sensors.py::_read_intel_gpu_engine_times`: for every numeric
/// pid directory, every fd whose readlink target contains `/dri/` and whose
/// `fdinfo` contains `drm-pdev:\t<pci_addr>` contributes its engine counters
/// keyed by `drm-client-id`. Clients sharing a DRM file dedupe to the last fd
/// scanned (matches Python's dict-overwrite). Per-process/per-fd errors are
/// skipped; the result is keyed by client id, deduping shared fds.
#[must_use]
pub fn read_intel_gpu_engine_times(
    proc_root: &Path,
    pci_addr: &str,
) -> BTreeMap<u32, BTreeMap<String, u64>> {
    let mut result: BTreeMap<u32, BTreeMap<String, u64>> = BTreeMap::new();
    let needle = format!("drm-pdev:\t{pci_addr}");
    let Ok(pids) = fs::read_dir(proc_root) else {
        return result;
    };
    for pid_entry in pids.flatten() {
        let file_name = pid_entry.file_name();
        let Some(pid_name) = file_name.to_str() else {
            continue;
        };
        if !pid_name.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let fd_dir = pid_entry.path().join("fd");
        let Ok(fds) = fs::read_dir(&fd_dir) else {
            continue;
        };
        for fd_entry in fds.flatten() {
            let fd_path = fd_entry.path();
            let Ok(link) = fs::read_link(&fd_path) else {
                continue;
            };
            if !link.to_string_lossy().contains("/dri/") {
                continue;
            }
            let fdinfo_path = pid_entry.path().join("fdinfo").join(fd_entry.file_name());
            let Ok(text) = fs::read_to_string(&fdinfo_path) else {
                continue;
            };
            if !text.contains(&needle) {
                continue;
            }
            let Some((client_id, engines)) = parse_fdinfo(&text) else {
                continue;
            };
            result.insert(client_id, engines);
        }
    }
    result
}

/// Parses a single fdinfo buffer into `(client_id, engines)`.
///
/// Returns `None` when `drm-client-id` is missing or unparseable. Malformed
/// `drm-engine-*` values are skipped (Rust is more defensive than Python,
/// which would raise; in practice the kernel always emits well-formed ints).
fn parse_fdinfo(text: &str) -> Option<(u32, BTreeMap<String, u64>)> {
    let mut client_id: Option<u32> = None;
    let mut engines: BTreeMap<String, u64> = BTreeMap::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("drm-client-id:") {
            if let Some(token) = value.split_whitespace().next() {
                if let Ok(parsed) = token.parse::<u32>() {
                    client_id = Some(parsed);
                }
            }
        } else if let Some(rest) = line.strip_prefix("drm-engine-") {
            let Some((name, value)) = rest.split_once(':') else {
                continue;
            };
            let engine_name = name.trim();
            let Some(token) = value.split_whitespace().next() else {
                continue;
            };
            let Ok(ns) = token.parse::<u64>() else {
                continue;
            };
            engines.insert(engine_name.to_string(), ns);
        }
    }
    client_id.map(|id| (id, engines))
}

/// Per-engine utilization % since the previous sample, summed across clients.
///
/// Mirrors `src/sensors.py::_read_intel_gpu_metrics`: deltas are summed per
/// engine across all clients present in both samples, then divided by the
/// elapsed wall time (in ns) and capped at 99. The returned map always carries
/// the four [`INTEL_GPU_ENGINES`] keys initialized to 0, plus any extra engine
/// names observed in fdinfo (Python's dict grows the same way).
#[must_use]
pub fn read_intel_gpu_metrics(
    proc_root: &Path,
    state: &mut IntelGpuState,
    pci_addr: &str,
    clock: ClockSnapshot,
) -> IntelGpuMetrics {
    let current = read_intel_gpu_engine_times(proc_root, pci_addr);
    let prev = &state.engine_prev;
    let dt = match state.prev_sample_at {
        Some(previous) => clock.monotonic.as_secs_f64() - previous.as_secs_f64(),
        None => 0.0,
    };

    let mut sums: BTreeMap<String, u64> = BTreeMap::new();
    for engine in INTEL_GPU_ENGINES {
        sums.insert((*engine).to_string(), 0);
    }
    if !prev.is_empty() && dt > 0.0 {
        for (client_id, engines) in &current {
            let Some(prev_engines) = prev.get(client_id) else {
                continue;
            };
            for (engine, &ns) in engines {
                let prev_ns = prev_engines.get(engine).copied().unwrap_or(ns);
                if let Some(delta) = ns.checked_sub(prev_ns) {
                    if delta > 0 {
                        sums.entry(engine.clone())
                            .or_insert(0)
                            .saturating_add_assign_u64(delta);
                    }
                }
            }
        }
    }

    let dt_ns = dt * 1_000_000_000_f64;
    let pct = if dt > 0.0 {
        sums.iter()
            .map(|(engine, ns_sum)| {
                let raw = (*ns_sum as f64 / dt_ns * 100.0) as i32;
                (engine.clone(), raw.min(99))
            })
            .collect()
    } else {
        sums.keys().map(|engine| (engine.clone(), 0)).collect()
    };

    state.engine_prev = current;
    state.prev_sample_at = Some(clock.monotonic);
    pct
}

/// TTL-cached wrapper around [`read_intel_gpu_metrics`].
///
/// Mirrors `src/sensors.py::_read_intel_gpu_metrics_cached`: a cached value is
/// served within `INTEL_GPU_USAGE_TTL`, then refreshed.
#[must_use]
pub fn read_intel_gpu_metrics_cached(
    proc_root: &Path,
    state: &mut IntelGpuState,
    pci_addr: &str,
    clock: ClockSnapshot,
) -> IntelGpuMetrics {
    let fresh = state
        .usage_cache_sample_at
        .map(|prev| clock.monotonic.saturating_sub(prev) < INTEL_GPU_USAGE_TTL)
        .unwrap_or(false);
    if fresh {
        return state.usage_cache.clone();
    }
    let metrics = read_intel_gpu_metrics(proc_root, state, pci_addr, clock);
    state.usage_cache = metrics.clone();
    state.usage_cache_sample_at = Some(clock.monotonic);
    metrics
}

/// Helper trait to keep `saturating_add_assign` readable without pulling a
/// wider dependency. Specialized to `u64` since that's the only type we sum.
trait SaturatingAddAssignU64 {
    fn saturating_add_assign_u64(&mut self, other: u64);
}

impl SaturatingAddAssignU64 for u64 {
    fn saturating_add_assign_u64(&mut self, other: u64) {
        *self = self.saturating_add(other);
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;

    use std::fs;
    use std::os::unix::fs::symlink;
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
                "pirostats-intel-gpu-{}-{unique}",
                std::process::id()
            ));
            if let Err(error) = fs::create_dir_all(&root) {
                panic!("failed to create temp root {}: {error}", root.display());
            }
            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn write_str(&self, relative: &str, content: &str) {
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

        fn symlink(&self, original: &str, link_relative: &str) {
            let link = self.root.join(link_relative);
            if let Some(parent) = link.parent()
                && let Err(error) = fs::create_dir_all(parent)
            {
                panic!("failed to create {}: {error}", parent.display());
            }
            if let Err(error) = symlink(original, &link) {
                panic!(
                    "failed to symlink {} -> {}: {error}",
                    link.display(),
                    original
                );
            }
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn detect_intel_gpu_returns_default_when_drm_dir_missing() {
        let tmp = TempTree::new();

        let paths = detect_intel_gpu(&tmp.path().join("sys"));

        assert_eq!(paths.freq_path, None);
        assert_eq!(paths.pci, None);
    }

    #[test]
    fn detect_intel_gpu_skips_non_intel_and_non_display_cards() {
        let tmp = TempTree::new();
        // NVIDIA vendor, display class — skipped.
        tmp.write_str("sys/class/drm/card0/device/vendor", "0x10de\n");
        tmp.write_str("sys/class/drm/card0/device/class", "0x030000\n");
        // Intel vendor but not display — skipped.
        tmp.write_str("sys/class/drm/card1/device/vendor", "0x8086\n");
        tmp.write_str("sys/class/drm/card1/device/class", "0x088000\n");

        let paths = detect_intel_gpu(&tmp.path().join("sys"));

        assert_eq!(paths.freq_path, None);
        assert_eq!(paths.pci, None);
    }

    #[test]
    fn detect_intel_gpu_picks_intel_display_card_and_freq_path() {
        let tmp = TempTree::new();
        // Place vendor/class on the resolved PCI device directory and make
        // `card0/device` a symlink to it so canonicalize() yields the PCI addr.
        tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/vendor", "0x8086\n");
        tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/class", "0x030000\n");
        tmp.write_str("sys/class/drm/card0/gt_act_freq_mhz", "1300\n");
        tmp.symlink(
            "../../../devices/pci0000:00/0000:00:02.0",
            "sys/class/drm/card0/device",
        );

        let paths = detect_intel_gpu(&tmp.path().join("sys"));

        assert_eq!(
            paths.freq_path,
            Some(tmp.path().join("sys/class/drm/card0/gt_act_freq_mhz"))
        );
        assert_eq!(paths.pci.as_deref(), Some("0000:00:02.0"));
    }

    #[test]
    fn detect_intel_gpu_omits_freq_path_when_absent_but_returns_pci() {
        let tmp = TempTree::new();
        tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/vendor", "0x8086\n");
        tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/class", "0x030000\n");
        tmp.symlink(
            "../../../devices/pci0000:00/0000:00:02.0",
            "sys/class/drm/card0/device",
        );

        let paths = detect_intel_gpu(&tmp.path().join("sys"));

        assert_eq!(paths.freq_path, None);
        assert_eq!(paths.pci.as_deref(), Some("0000:00:02.0"));
    }

    #[test]
    fn detect_intel_gpu_returns_first_card_in_sorted_order() {
        let tmp = TempTree::new();
        // Two Intel display cards; card0 wins because it sorts first.
        tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/vendor", "0x8086\n");
        tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/class", "0x030000\n");
        tmp.write_str("sys/devices/pci0000:00/0000:01:00.0/vendor", "0x8086\n");
        tmp.write_str("sys/devices/pci0000:00/0000:01:00.0/class", "0x030000\n");
        tmp.symlink(
            "../../../devices/pci0000:00/0000:00:02.0",
            "sys/class/drm/card0/device",
        );
        tmp.symlink(
            "../../../devices/pci0000:00/0000:01:00.0",
            "sys/class/drm/card1/device",
        );

        let paths = detect_intel_gpu(&tmp.path().join("sys"));

        assert_eq!(paths.pci.as_deref(), Some("0000:00:02.0"));
    }

    #[test]
    fn read_intel_gpu_engine_times_returns_empty_when_no_clients_match() {
        let tmp = TempTree::new();

        let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

        assert!(result.is_empty());
    }

    #[test]
    fn read_intel_gpu_engine_times_collects_engine_counters_keyed_by_client_id() {
        let tmp = TempTree::new();
        // pid 100 has a fd whose readlink points at /dev/dri/renderD128.
        tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
        tmp.write_str(
            "proc/100/fdinfo/3",
            "pos:\t0\n\
             flags:\t02\n\
             drm-pdev:\t0000:00:02.0\n\
             drm-client-id:\t5\n\
             drm-engine-render:\t1000000000 ns\n\
             drm-engine-copy:\t500000000 ns\n\
             drm-engine-video:\t0 ns\n\
             drm-engine-video-enhance:\t0 ns\n",
        );

        let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

        let engines = result.get(&5).expect("client 5 present");
        assert_eq!(engines.get("render").copied(), Some(1_000_000_000));
        assert_eq!(engines.get("copy").copied(), Some(500_000_000));
        assert_eq!(engines.get("video").copied(), Some(0));
    }

    #[test]
    fn read_intel_gpu_engine_times_skips_fds_without_dri_link() {
        let tmp = TempTree::new();
        tmp.symlink("/dev/null", "proc/100/fd/3");
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\n",
        );

        let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

        assert!(result.is_empty());
    }

    #[test]
    fn read_intel_gpu_engine_times_skips_fds_with_mismatched_pdev() {
        let tmp = TempTree::new();
        tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:07.0\ndrm-client-id:\t5\n",
        );

        let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

        assert!(result.is_empty());
    }

    #[test]
    fn read_intel_gpu_engine_times_skips_non_numeric_pid_dirs() {
        let tmp = TempTree::new();
        tmp.symlink("/dev/dri/renderD128", "proc/self/fd/3");
        tmp.write_str(
            "proc/self/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\n",
        );

        let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

        assert!(result.is_empty());
    }

    #[test]
    fn read_intel_gpu_engine_times_dedupes_shared_fds_by_client_id() {
        let tmp = TempTree::new();
        // Two fds in one process pointing at the same DRM file (dup'd fd),
        // both reporting the same client id and engine counter. The scan
        // order from readdir is unspecified, so assert only the dedup: one
        // entry for client 5, not two.
        tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
        tmp.symlink("/dev/dri/renderD128", "proc/100/fd/4");
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t200 ns\n",
        );
        tmp.write_str(
            "proc/100/fdinfo/4",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t200 ns\n",
        );

        let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

        // Exactly one client entry, regardless of which fd was scanned last.
        assert_eq!(result.len(), 1);
        let engines = result.get(&5).expect("client 5 present");
        assert_eq!(engines.get("render").copied(), Some(200));
    }

    #[test]
    fn read_intel_gpu_metrics_first_sample_seeds_prev_and_returns_zeros() {
        let tmp = TempTree::new();
        tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
        );

        let mut state = IntelGpuState::default();
        let metrics = read_intel_gpu_metrics(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(0),
        );

        // No prev yet → all engines report 0.
        assert_eq!(metrics.get("render").copied(), Some(0));
        assert_eq!(metrics.get("copy").copied(), Some(0));
        assert_eq!(metrics.get("video").copied(), Some(0));
        assert_eq!(metrics.get("video-enhance").copied(), Some(0));
        // Prev is seeded for the next diff.
        assert!(state.engine_prev.contains_key(&5));
        assert_eq!(state.prev_sample_at, Some(Duration::ZERO));
    }

    #[test]
    fn read_intel_gpu_metrics_diffs_per_engine_and_caps_at_99() {
        let tmp = TempTree::new();
        tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");

        let mut state = IntelGpuState::default();
        // First sample: 0 ns everywhere (seeds prev).
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\n\
             drm-engine-render:\t0 ns\ndrm-engine-video:\t0 ns\n",
        );
        let _ = read_intel_gpu_metrics(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(0),
        );

        // Second sample after 1s: render advanced by 1.5s of ns (150% → capped
        // at 99), video advanced by 0.5s of ns (50%).
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\n\
             drm-engine-render:\t1500000000 ns\ndrm-engine-video:\t500000000 ns\n",
        );
        let metrics = read_intel_gpu_metrics(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(1),
        );

        assert_eq!(metrics.get("render").copied(), Some(99));
        assert_eq!(metrics.get("video").copied(), Some(50));
    }

    #[test]
    fn read_intel_gpu_metrics_sums_engines_across_clients() {
        let tmp = TempTree::new();
        tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
        tmp.symlink("/dev/dri/renderD129", "proc/200/fd/7");

        let mut state = IntelGpuState::default();
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
        );
        tmp.write_str(
            "proc/200/fdinfo/7",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t7\ndrm-engine-render:\t0 ns\n",
        );
        let _ = read_intel_gpu_metrics(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(0),
        );

        // Each client adds 0.4s of render over 1s → sum 0.8s/1s = 80%.
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t400000000 ns\n",
        );
        tmp.write_str(
            "proc/200/fdinfo/7",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t7\ndrm-engine-render:\t400000000 ns\n",
        );
        let metrics = read_intel_gpu_metrics(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(1),
        );

        assert_eq!(metrics.get("render").copied(), Some(80));
    }

    #[test]
    fn read_intel_gpu_metrics_skips_clients_absent_from_prev() {
        let tmp = TempTree::new();
        tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
        let mut state = IntelGpuState::default();
        // Seed prev with only client 5.
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
        );
        let _ = read_intel_gpu_metrics(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(0),
        );

        // Now point the same fd at a different client (new DRM client id).
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t9\ndrm-engine-render:\t1000000000 ns\n",
        );
        let metrics = read_intel_gpu_metrics(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(1),
        );

        // Client 9 had no prev → contributes 0.
        assert_eq!(metrics.get("render").copied(), Some(0));
    }

    #[test]
    fn read_intel_gpu_metrics_cached_serves_within_ttl_and_refreshes() {
        let tmp = TempTree::new();
        tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");

        let mut state = IntelGpuState::default();
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
        );
        let first = read_intel_gpu_metrics_cached(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(0),
        );

        // Change fdinfo immediately; within TTL the cache is served unchanged.
        tmp.write_str(
            "proc/100/fdinfo/3",
            "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t1000000000 ns\n",
        );
        let served = read_intel_gpu_metrics_cached(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(1),
        );
        assert_eq!(served, first);

        // Past TTL: refresh recomputes (prev seeded at t=0; current is 1e9 ns
        // of render over 31s elapsed → ~3%). The cached zero is replaced.
        let refreshed = read_intel_gpu_metrics_cached(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(31),
        );
        assert_ne!(
            refreshed.get("render").copied(),
            first.get("render").copied()
        );
        assert_eq!(refreshed.get("render").copied(), Some(3));
    }
}
