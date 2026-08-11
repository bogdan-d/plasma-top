//! Intel iGPU detection and DRM fdinfo-based usage readings.
//!
//! Owns Intel GPU behavior formerly grouped inside `src/sensors.py`:
//!
//! - [`detect_intel_gpu`] walks `/sys/class/drm/card[0-9]*` for a vendor
//!   `0x8086` / display-class card and returns the gt frequency sysfs path
//!   plus the PCI address used to attribute fdinfo counters.
//! - [`read_intel_gpu_engine_times`] scans `/proc/*/fd/*` for DRM client fds
//!   and reads their `drm-engine-*` ns counters, keyed by `drm-client-id`.
//! - [`read_intel_gpu_metrics`] diffs two snapshots into per-engine
//!   utilization percentages (capped at 99), summed across clients.
//! - Synchronous collection applies the 30-second freshness budget.
//!
//! All readers take explicit proc/sys roots and clock snapshots so tests never
//! touch the host filesystem or sleep. Symlink creation under tests uses
//! [`std::os::unix::fs::symlink`], which is safe Rust on Unix.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::domain::boundary::ClockSnapshot;
use crate::domain::readings::RetainedMetricSample;

/// Intel DRM engine names tracked for the panel/graphs page.
pub const INTEL_GPU_ENGINES: &[&str] = &["render", "copy", "video", "video-enhance"];
/// Intel GPU usage freshness budget, matching disk-temperature and fan samples.
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

/// Outcome of one Intel GPU counter read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum IntelGpuReadOutcome {
    Value(IntelGpuMetrics),
    Baseline,
    Failed,
}

/// Mutable Intel GPU diff and retained sample state that persists between sampling passes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntelGpuState {
    /// PCI source identity for retained counters and usage.
    pub(super) source_pci: Option<String>,
    pub(super) frequency: RetainedMetricSample<i32>,
    pub(super) frequency_source: Option<PathBuf>,
    pub(super) usage: RetainedMetricSample<IntelGpuMetrics>,
    /// Whether a baseline/reset still needs a later comparable read.
    pub(super) usage_needs_comparable: bool,
    /// Previous per-client engine ns counters keyed by DRM client id.
    pub engine_prev: BTreeMap<u32, BTreeMap<String, u64>>,
    /// Monotonic instant of the previous sample.
    pub prev_sample_at: Option<Duration>,
    /// Latest retained per-engine utilization percentages.
    pub usage_cache: IntelGpuMetrics,
    /// Monotonic instant of the retained utilization sample.
    pub usage_cache_sample_at: Option<Duration>,
}

impl IntelGpuState {
    pub(crate) fn reconcile_sources(
        &mut self,
        pci: Option<&str>,
        frequency_path: Option<&PathBuf>,
        wants_usage: bool,
        wants_frequency: bool,
    ) {
        if self.source_pci.as_deref() != pci {
            *self = Self {
                source_pci: pci.map(str::to_owned),
                ..Self::default()
            };
        }
        if !wants_usage {
            self.usage.invalidate();
            self.engine_prev.clear();
            self.prev_sample_at = None;
            self.usage_needs_comparable = false;
            self.usage_cache.clear();
            self.usage_cache_sample_at = None;
        }
        if !wants_frequency {
            self.frequency.invalidate();
            self.frequency_source = None;
        } else if self.frequency_source.as_ref() != frequency_path {
            self.frequency.invalidate();
            self.frequency_source = frequency_path.cloned();
        }
    }
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
    detect_intel_gpu_outcome(sys_root).unwrap_or_default()
}

/// Detects Intel GPU paths without flattening incomplete DRM enumeration.
pub(crate) fn detect_intel_gpu_outcome(sys_root: &Path) -> io::Result<IntelGpuPaths> {
    let cards = list_intel_cards_outcome(sys_root)?;
    for card in cards {
        let device = card.join("device");
        let vendor = fs::read_to_string(device.join("vendor"))?;
        parse_pci_value(&vendor)?;
        if vendor.trim() != "0x8086" {
            continue;
        }
        let class = fs::read_to_string(device.join("class"))?;
        parse_pci_value(&class)?;
        if !class.trim().starts_with("0x03") {
            continue;
        }
        let freq_path = card.join("gt_act_freq_mhz");
        let freq_path = freq_path.try_exists()?.then_some(freq_path);
        // Resolve the device symlink (e.g. ../../devices/.../0000:00:02.0)
        // and take its basename — matches Python's `device.resolve().name`.
        let pci = fs::canonicalize(&device)?
            .file_name()
            .and_then(|name| name.to_str())
            .map(ToString::to_string)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid PCI path"))?;
        return Ok(IntelGpuPaths {
            freq_path,
            pci: Some(pci),
        });
    }
    Ok(IntelGpuPaths::default())
}

fn list_intel_cards_outcome(sys_root: &Path) -> io::Result<Vec<PathBuf>> {
    let drm = sys_root.join("class/drm");
    let entries = fs::read_dir(&drm)?;
    let mut cards: Vec<PathBuf> = entries
        .collect::<io::Result<Vec<_>>>()?
        .into_iter()
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
    Ok(cards)
}

fn parse_pci_value(value: &str) -> io::Result<u32> {
    value
        .trim()
        .strip_prefix("0x")
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed PCI value"))
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
    try_read_intel_gpu_engine_times(proc_root, pci_addr).unwrap_or_default()
}

fn try_read_intel_gpu_engine_times(
    proc_root: &Path,
    pci_addr: &str,
) -> Option<BTreeMap<u32, BTreeMap<String, u64>>> {
    let mut result: BTreeMap<u32, BTreeMap<String, u64>> = BTreeMap::new();
    let needle = format!("drm-pdev:\t{pci_addr}");
    let Ok(pids) = fs::read_dir(proc_root) else {
        return None;
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
    Some(result)
}

/// Parses a single fdinfo buffer into `(client_id, engines)`.
///
/// Returns `None` when `drm-client-id` is missing or unparseable. Malformed
/// `drm-engine-*` values are skipped (Rust is more defensive than Python,
/// which would raise; in practice the kernel always emits well-formed ints).
#[expect(
    clippy::collapsible_if,
    reason = "prefix, token, and integer parsing remain separate input stages"
)]
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
/// elapsed wall time (in ns) and capped at 99. The first read returns `None` after establishing a
/// baseline. A measured map always carries the four [`INTEL_GPU_ENGINES`] keys initialized to 0,
/// plus any extra engine names observed in fdinfo (Python's dict grows the same way).
#[must_use]
pub fn read_intel_gpu_metrics(
    proc_root: &Path,
    state: &mut IntelGpuState,
    pci_addr: &str,
    clock: ClockSnapshot,
) -> Option<IntelGpuMetrics> {
    match read_intel_gpu_metrics_once(proc_root, state, pci_addr, clock) {
        IntelGpuReadOutcome::Value(metrics) => Some(metrics),
        IntelGpuReadOutcome::Baseline | IntelGpuReadOutcome::Failed => None,
    }
}

/// Performs one Intel GPU usage attempt without replacing state on root-enumeration failure.
#[must_use]
pub(crate) fn read_intel_gpu_metrics_once(
    proc_root: &Path,
    state: &mut IntelGpuState,
    pci_addr: &str,
    clock: ClockSnapshot,
) -> IntelGpuReadOutcome {
    let Some(current) = try_read_intel_gpu_engine_times(proc_root, pci_addr) else {
        return IntelGpuReadOutcome::Failed;
    };
    let prev = &state.engine_prev;
    let Some(previous_at) = state.prev_sample_at else {
        state.engine_prev = current;
        state.prev_sample_at = Some(clock.monotonic);
        return IntelGpuReadOutcome::Baseline;
    };
    let dt = clock.monotonic.as_secs_f64() - previous_at.as_secs_f64();
    if dt <= 0.0 {
        return IntelGpuReadOutcome::Baseline;
    }
    if engine_counters_rolled_back(&current, &state.engine_prev) {
        state.engine_prev = current;
        state.prev_sample_at = Some(clock.monotonic);
        return IntelGpuReadOutcome::Baseline;
    }

    let mut sums: BTreeMap<String, u64> = BTreeMap::new();
    for engine in INTEL_GPU_ENGINES {
        sums.insert((*engine).to_string(), 0);
    }
    for (client_id, engines) in &current {
        let Some(prev_engines) = prev.get(client_id) else {
            continue;
        };
        for (engine, &ns) in engines {
            let prev_ns = prev_engines.get(engine).copied().unwrap_or(ns);
            if let Some(delta) = ns.checked_sub(prev_ns)
                && delta > 0
            {
                sums.entry(engine.clone())
                    .or_insert(0)
                    .saturating_add_assign_u64(delta);
            }
        }
    }

    let dt_ns = dt * 1_000_000_000_f64;
    let pct = sums
        .iter()
        .map(|(engine, ns_sum)| {
            let raw = (*ns_sum as f64 / dt_ns * 100.0) as i32;
            (engine.clone(), raw.min(99))
        })
        .collect();

    state.engine_prev = current;
    state.prev_sample_at = Some(clock.monotonic);
    IntelGpuReadOutcome::Value(pct)
}

fn engine_counters_rolled_back(
    current: &BTreeMap<u32, BTreeMap<String, u64>>,
    previous: &BTreeMap<u32, BTreeMap<String, u64>>,
) -> bool {
    current.iter().any(|(client, engines)| {
        previous.get(client).is_some_and(|previous_engines| {
            engines.iter().any(|(engine, value)| {
                previous_engines
                    .get(engine)
                    .is_some_and(|previous| value < previous)
            })
        })
    })
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
mod tests;
