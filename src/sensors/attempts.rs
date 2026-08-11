use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::Config;
use crate::domain::boundary::{ClockSnapshot, DbusFacade};
use crate::domain::metric::Capability;
use crate::domain::readings::{
    HardwareInventory, LoadAverage, MetricSample, SmartDisk, TopProcessDetails, TopProcessSummary,
};

use super::gpu_intel::IntelGpuState;
use super::power::{BoltBatteryFacade, PowerState};
use super::{
    cpu, disk, external::ExternalState, gpu_intel, hwmon, memory, network, power, process,
};

mod network_identity;
mod nvidia;

pub(crate) use network_identity::attempt_network_info;
#[cfg(test)]
pub(crate) use nvidia::NvidiaResult;
pub(crate) use nvidia::{attempt_nvidia_fallback, attempt_nvml_nvidia, nvidia_result};

/// Accumulated per-section wall-clock elapsed, keyed by section name.
///
/// Populated only when the profiling subcommand gives the serial executor `Some(&mut Timings)`. Deterministic tests pass `None`, so no wall clock is read.
pub type Timings = BTreeMap<String, Duration>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptStatus {
    Captured,
    Baseline,
    Absent,
    Failed,
    Cached,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptResult<T> {
    pub(crate) sample: Option<MetricSample<T>>,
    pub(crate) attempted_at: Option<Duration>,
    pub(crate) failed_at: Option<Duration>,
    pub(crate) latest_attempt_failed: bool,
    pub(crate) status: AttemptStatus,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ReadOutcome<T> {
    Value(T),
    Baseline,
    Absent,
    Failed,
}
fn commit_attempt<T: Clone>(
    retained: &mut crate::domain::readings::RetainedMetricSample<T>,
    outcome: ReadOutcome<T>,
    attempted_at: Duration,
) -> AttemptResult<T> {
    let status = match outcome {
        ReadOutcome::Value(value) => {
            retained.record_value(value, attempted_at);
            AttemptStatus::Captured
        }
        ReadOutcome::Baseline => {
            retained.record_baseline(attempted_at);
            AttemptStatus::Baseline
        }
        ReadOutcome::Absent => {
            retained.record_absence(attempted_at);
            AttemptStatus::Absent
        }
        ReadOutcome::Failed => {
            retained.record_failure(attempted_at);
            AttemptStatus::Failed
        }
    };
    AttemptResult {
        sample: retained.latest.clone(),
        attempted_at: retained.attempted_at,
        failed_at: retained.failed_at,
        latest_attempt_failed: retained.latest_attempt_failed,
        status,
    }
}

pub(super) fn cached_attempt<T: Clone>(
    retained: &crate::domain::readings::RetainedMetricSample<T>,
) -> AttemptResult<T> {
    AttemptResult {
        sample: retained.latest.clone(),
        attempted_at: retained.attempted_at,
        failed_at: retained.failed_at,
        latest_attempt_failed: retained.latest_attempt_failed,
        status: AttemptStatus::Cached,
    }
}

#[cfg(test)]
pub(super) fn cached_attempt_or_empty<T: Clone>(
    retained: Option<&crate::domain::readings::RetainedMetricSample<T>>,
) -> AttemptResult<T> {
    retained.map_or(
        AttemptResult {
            sample: None,
            attempted_at: None,
            failed_at: None,
            latest_attempt_failed: false,
            status: AttemptStatus::Cached,
        },
        cached_attempt,
    )
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CpuResult {
    pub(crate) usage: AttemptResult<i32>,
    pub(crate) temperature: AttemptResult<i32>,
    pub(crate) frequency_mhz: AttemptResult<f64>,
    pub(crate) turbo: AttemptResult<bool>,
    pub(crate) history: Option<MetricSample<Vec<i32>>>,
    pub(crate) uptime_seconds: AttemptResult<i64>,
    pub(crate) load_average: AttemptResult<LoadAverage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CpuCoreResult {
    pub(crate) usage: AttemptResult<Vec<i32>>,
    pub(crate) history: Option<MetricSample<Vec<Vec<i32>>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProcessReadings {
    pub(crate) summary: Vec<TopProcessSummary>,
    pub(crate) full: Vec<TopProcessDetails>,
}
/// Typed result produced by one process-owner attempt.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProcessResult {
    pub(crate) reading: AttemptResult<ProcessReadings>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MemoryResult {
    pub(crate) usage: AttemptResult<memory::MemoryUsage>,
    pub(crate) swap_usage: AttemptResult<i32>,
    pub(crate) history: Option<MetricSample<Vec<i32>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NetworkSpeedResult {
    pub(crate) reading: AttemptResult<(u64, u64)>,
    pub(crate) up_history: Option<MetricSample<Vec<u64>>>,
    pub(crate) down_history: Option<MetricSample<Vec<u64>>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NetworkInfoResult {
    pub(crate) reading: AttemptResult<network::NetInfo>,
    pub(crate) wifi: AttemptResult<network::WifiInfo>,
    pub(crate) route_status: AttemptStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntelFrequencyResult {
    pub(crate) reading: AttemptResult<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct IntelUsageResult {
    pub(crate) reading: AttemptResult<gpu_intel::IntelGpuMetrics>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExternalResult {
    pub(crate) brightness: AttemptResult<i32>,
    pub(crate) updates: AttemptResult<i32>,
    pub(crate) server: AttemptResult<bool>,
}
/// Result of one cadence-free disk-temperature owner attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiskTemperatureResult {
    pub(crate) label: String,
    pub(crate) reading: AttemptResult<i32>,
}
/// Result of one cadence-free fan-speed owner attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FanSpeedResult {
    pub(crate) label: String,
    pub(crate) reading: AttemptResult<i32>,
}
/// Result of one cadence-free SMART owner attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SmartResult {
    pub(crate) label: String,
    pub(crate) reading: AttemptResult<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SystemBatteryResult {
    pub(crate) reading: AttemptResult<crate::domain::readings::BatterySystemReading>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PeripheralPowerResult {
    pub(crate) reading: AttemptResult<crate::domain::readings::BatteryPeripheralReading>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiskIoResult {
    pub(crate) reading: AttemptResult<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DiskUsageResult {
    pub(crate) mount: String,
    pub(crate) reading: AttemptResult<disk::DiskUsage>,
}

/// Performs one real disk-temperature attempt with no cadence policy.
pub(crate) fn attempt_disk_temperature(
    state: &mut disk::DiskState,
    label: &str,
    path: &Path,
    captured_at: Duration,
) -> DiskTemperatureResult {
    if state.hd_temp_sources.get(label).map(PathBuf::as_path) != Some(path) {
        state.hd_temp_cache.remove(label);
        state
            .hd_temp_sources
            .insert(label.to_owned(), path.to_owned());
    }
    let outcome = disk::read_hd_temp(path).map_or(ReadOutcome::Failed, ReadOutcome::Value);
    let reading = commit_attempt(
        state.hd_temp_cache.entry(label.to_owned()).or_default(),
        outcome,
        captured_at,
    );
    DiskTemperatureResult {
        label: label.to_owned(),
        reading,
    }
}

/// Performs one real fan-speed attempt with no cadence policy.
pub(crate) fn attempt_fan_speed(
    state: &mut disk::DiskState,
    label: &str,
    path: &Path,
    captured_at: Duration,
) -> FanSpeedResult {
    if state.fan_speed_sources.get(label).map(PathBuf::as_path) != Some(path) {
        state.fan_speed_cache.remove(label);
        state
            .fan_speed_sources
            .insert(label.to_owned(), path.to_owned());
    }
    let outcome = disk::read_fan_speed(path).map_or(ReadOutcome::Failed, ReadOutcome::Value);
    let reading = commit_attempt(
        state.fan_speed_cache.entry(label.to_owned()).or_default(),
        outcome,
        captured_at,
    );
    FanSpeedResult {
        label: label.to_owned(),
        reading,
    }
}

/// Performs one real SMART attempt with no cadence policy.
pub(crate) fn attempt_smart(
    state: &mut disk::DiskState,
    dbus: &mut dyn DbusFacade,
    label: &str,
    drive: &SmartDisk,
    captured_at: Duration,
) -> SmartResult {
    if state.smart_sources.get(label) != Some(drive) {
        state.smart_cache.remove(label);
        state.smart_sources.insert(label.to_owned(), drive.clone());
    }
    let outcome = power::read_disk_smart(dbus, &drive.object_path, drive.interface)
        .map_or(ReadOutcome::Failed, ReadOutcome::Value);
    let reading = commit_attempt(
        state.smart_cache.entry(label.to_owned()).or_default(),
        outcome,
        captured_at,
    );
    SmartResult {
        label: label.to_owned(),
        reading,
    }
}

/// Performs one disk-I/O counter attempt without cadence policy.
pub(crate) fn attempt_disk_io(
    state: &mut disk::DiskState,
    proc_root: &Path,
    device: &str,
    clock: ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> DiskIoResult {
    let outcome = timed(timings, "disk_io", || {
        disk::read_disk_io_once(proc_root, state, device, clock)
    });
    let outcome = match outcome {
        disk::DiskIoReadOutcome::Value(read, write) => ReadOutcome::Value((read, write)),
        disk::DiskIoReadOutcome::Baseline | disk::DiskIoReadOutcome::NoDelta => {
            ReadOutcome::Baseline
        }
        disk::DiskIoReadOutcome::Failed => ReadOutcome::Failed,
    };
    DiskIoResult {
        reading: commit_attempt(&mut state.io, outcome, clock.monotonic),
    }
}

/// Performs one mount-usage attempt without cadence policy.
pub(crate) fn attempt_disk_usage(
    state: &mut disk::DiskState,
    mount: &str,
    clock: ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> DiskUsageResult {
    let value = timed(timings, &format!("disk_usage[{mount}]"), || {
        disk::read_disk_usage(Path::new(mount))
    });
    let outcome = value.map_or(ReadOutcome::Failed, ReadOutcome::Value);
    DiskUsageResult {
        mount: mount.to_owned(),
        reading: commit_attempt(
            state.usage.entry(mount.to_owned()).or_default(),
            outcome,
            clock.monotonic,
        ),
    }
}

/// Performs one system-battery attempt without cadence policy.
pub(crate) fn attempt_system_battery(
    state: &mut PowerState,
    dbus: &mut dyn DbusFacade,
    id: &str,
    sys_root: &Path,
    clock: ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> SystemBatteryResult {
    let cache = state.battery_sys_cache.entry(id.to_owned()).or_default();
    let succeeded = timed(timings, "battery_sys", || {
        power::refresh_battery_sys(cache, dbus, id, sys_root, clock)
    });
    let sample = cache.sampled_at.and_then(|captured_at| {
        power::battery_sys_from_cache(id, cache)
            .map(|reading| MetricSample::new(reading, captured_at))
    });
    let status = if succeeded {
        if sample.is_some() {
            AttemptStatus::Captured
        } else {
            AttemptStatus::Absent
        }
    } else {
        AttemptStatus::Failed
    };
    SystemBatteryResult {
        reading: AttemptResult {
            sample,
            attempted_at: cache.attempted_at,
            failed_at: cache.failed_at,
            latest_attempt_failed: status == AttemptStatus::Failed,
            status,
        },
    }
}

#[cfg(test)]
pub(super) fn cached_system_battery(
    id: &str,
    cache: Option<&power::BatterySystemCache>,
) -> SystemBatteryResult {
    SystemBatteryResult {
        reading: AttemptResult {
            sample: cache.and_then(|cache| {
                cache.sampled_at.and_then(|captured_at| {
                    power::battery_sys_from_cache(id, cache)
                        .map(|reading| MetricSample::new(reading, captured_at))
                })
            }),
            attempted_at: cache.and_then(|cache| cache.attempted_at),
            failed_at: cache.and_then(|cache| cache.failed_at),
            latest_attempt_failed: cache.is_some_and(|cache| cache.failed_at.is_some()),
            status: AttemptStatus::Cached,
        },
    }
}

/// Performs one UPower peripheral attempt without cadence policy.
pub(crate) fn attempt_upower_peripheral(
    cache: &mut power::BatteryPeripheralCache,
    dbus: &mut dyn DbusFacade,
    id: &str,
    name: Option<&str>,
    clock: ClockSnapshot,
    timing_key: &str,
    timings: &mut Option<&mut Timings>,
) -> PeripheralPowerResult {
    let (reading, succeeded) = timed(timings, timing_key, || {
        power::attempt_battery_periph_once(cache, dbus, id, name, clock)
    });
    let status = if succeeded {
        if reading.is_some() {
            AttemptStatus::Captured
        } else {
            AttemptStatus::Absent
        }
    } else {
        AttemptStatus::Failed
    };
    peripheral_result(cache, reading, status)
}

/// Performs one Bolt peripheral attempt without cadence policy.
pub(crate) fn attempt_bolt_peripheral(
    cache: &mut power::BatteryPeripheralCache,
    bolt: &mut dyn BoltBatteryFacade,
    index: i32,
    name: Option<&str>,
    clock: ClockSnapshot,
    timing_key: &str,
    timings: &mut Option<&mut Timings>,
) -> PeripheralPowerResult {
    let (reading, outcome) = timed(timings, timing_key, || {
        power::attempt_battery_bolt_once(cache, bolt, index, name, clock)
    });
    let status = match outcome {
        power::BoltAttemptOutcome::Captured => AttemptStatus::Captured,
        power::BoltAttemptOutcome::Unsupported => AttemptStatus::Absent,
        power::BoltAttemptOutcome::Failed => AttemptStatus::Failed,
    };
    peripheral_result(cache, reading, status)
}

fn peripheral_result(
    cache: &power::BatteryPeripheralCache,
    reading: Option<crate::domain::readings::BatteryPeripheralReading>,
    status: AttemptStatus,
) -> PeripheralPowerResult {
    PeripheralPowerResult {
        reading: AttemptResult {
            sample: reading.and_then(|reading| {
                cache
                    .sampled_at
                    .map(|captured_at| MetricSample::new(reading, captured_at))
            }),
            attempted_at: cache.attempted_at,
            failed_at: cache.failed_at,
            latest_attempt_failed: status == AttemptStatus::Failed,
            status,
        },
    }
}

#[cfg(test)]
pub(super) fn cached_peripheral(
    cache: &power::BatteryPeripheralCache,
    name: Option<&str>,
) -> PeripheralPowerResult {
    PeripheralPowerResult {
        reading: AttemptResult {
            sample: power::battery_periph_from_cache(cache, name).and_then(|reading| {
                cache
                    .sampled_at
                    .map(|captured_at| MetricSample::new(reading, captured_at))
            }),
            attempted_at: cache.attempted_at,
            failed_at: cache.failed_at,
            latest_attempt_failed: cache.failed_at.is_some(),
            status: AttemptStatus::Cached,
        },
    }
}

/// Performs one CPU-owner source attempt with no cadence policy of its own.
pub(crate) fn attempt_cpu(
    state: &mut cpu::CpuState,
    proc_root: &Path,
    _sys_root: &Path,
    hw: &HardwareInventory,
    caps: &BTreeSet<Capability>,
    clock: &mut dyn FnMut() -> ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> CpuResult {
    let usage_at = clock().monotonic;
    let usage = timed(timings, "cpu_usage", || {
        cpu::read_cpu_usage_once(proc_root, state)
    });
    let usage = match usage {
        cpu::CpuUsageReadOutcome::Value(value) => ReadOutcome::Value(value),
        cpu::CpuUsageReadOutcome::Baseline | cpu::CpuUsageReadOutcome::InvalidDelta => {
            ReadOutcome::Baseline
        }
        cpu::CpuUsageReadOutcome::Failed => ReadOutcome::Failed,
    };
    let usage = commit_attempt(&mut state.usage, usage, usage_at);
    let temperature = if caps.contains(&Capability::CpuTemperature) && hw.cpu_temp_path.is_some() {
        if state.temperature_source != hw.cpu_temp_path {
            state.temperature = Default::default();
            state.temperature_source.clone_from(&hw.cpu_temp_path);
        }
        let captured_at = clock().monotonic;
        let value = timed(timings, "cpu_temp", || {
            hwmon::read_path_millidegrees_celsius(hw.cpu_temp_path.as_deref())
        });
        commit_attempt(
            &mut state.temperature,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.temperature.invalidate();
        state.temperature_source = None;
        cached_attempt(&state.temperature)
    };
    let frequency_mhz = if caps.contains(&Capability::CpuFrequency) {
        if state.frequency_source != hw.cpu_freq_path {
            state.frequency_mhz = Default::default();
            state.frequency_source.clone_from(&hw.cpu_freq_path);
        }
        let captured_at = clock().monotonic;
        let value = timed(timings, "cpu_freq", || {
            cpu::read_cpu_frequency_mhz(proc_root, hw.cpu_freq_path.as_deref())
        });
        commit_attempt(
            &mut state.frequency_mhz,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.frequency_mhz.invalidate();
        state.frequency_source = None;
        cached_attempt(&state.frequency_mhz)
    };
    let turbo = if caps.contains(&Capability::CpuTurbo)
        && hw.cpu_turbo_supported
        && hw.cpu_turbo_path.is_some()
    {
        if state.turbo_source != hw.cpu_turbo_path {
            state.turbo.invalidate();
            state.turbo_source.clone_from(&hw.cpu_turbo_path);
        }
        let captured_at = clock().monotonic;
        let value = timed(timings, "cpu_turbo", || {
            cpu::read_cpu_turbo_path(hw.cpu_turbo_path.as_deref())
        });
        commit_attempt(
            &mut state.turbo,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.turbo.invalidate();
        state.turbo_source = None;
        cached_attempt(&state.turbo)
    };
    let uptime_seconds = if caps.contains(&Capability::Uptime) {
        let captured_at = clock().monotonic;
        let value = timed(timings, "uptime", || cpu::read_uptime_seconds(proc_root));
        commit_attempt(
            &mut state.uptime_seconds,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.uptime_seconds.invalidate();
        cached_attempt(&state.uptime_seconds)
    };
    let load_average = if caps.contains(&Capability::LoadAverage) {
        let captured_at = clock().monotonic;
        let value = timed(timings, "load_avg", || cpu::read_load_average(proc_root))
            .map(|(one, five, fifteen)| LoadAverage { one, five, fifteen });
        commit_attempt(
            &mut state.load_average,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.load_average.invalidate();
        cached_attempt(&state.load_average)
    };

    CpuResult {
        usage,
        temperature,
        frequency_mhz,
        turbo,
        history: state
            .cpu_history_sample_at
            .map(|captured_at| MetricSample::new(state.cpu_history.clone(), captured_at)),
        uptime_seconds,
        load_average,
    }
}

/// Performs one per-core CPU owner attempt without cadence policy.
pub(crate) fn attempt_cpu_cores(
    state: &mut cpu::CpuState,
    proc_root: &Path,
    clock: ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> CpuCoreResult {
    let attempted_at = clock.monotonic;
    let value = timed(timings, "cpu_cores", || {
        cpu::read_cpu_cores_once(proc_root, state)
    });
    let value = match value {
        cpu::CpuCoreReadOutcome::Value(value) => ReadOutcome::Value(value),
        cpu::CpuCoreReadOutcome::Baseline | cpu::CpuCoreReadOutcome::InvalidDelta => {
            ReadOutcome::Baseline
        }
        cpu::CpuCoreReadOutcome::Failed => ReadOutcome::Failed,
    };
    let usage = commit_attempt(&mut state.core_usage, value, attempted_at);
    let history = state
        .cpu_core_history
        .iter()
        .any(|values| !values.is_empty())
        .then(|| {
            MetricSample::new(
                state.cpu_core_history.clone(),
                state.cpu_core_history_sample_at.unwrap_or(attempted_at),
            )
        });
    CpuCoreResult { usage, history }
}

/// Performs one memory-owner source attempt with no cadence policy of its own.
pub(crate) fn attempt_memory(
    state: &mut memory::MemoryState,
    proc_root: &Path,
    wants_swap: bool,
    clock: &mut dyn FnMut() -> ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> MemoryResult {
    let usage_at = clock().monotonic;
    let usage = timed(timings, "mem_usage", || {
        memory::read_memory_usage_once(proc_root)
    });
    let usage = commit_attempt(
        &mut state.usage,
        usage.map_or(ReadOutcome::Failed, ReadOutcome::Value),
        usage_at,
    );
    let swap_usage = if wants_swap {
        let captured_at = clock().monotonic;
        let outcome = timed(timings, "swap_usage", || {
            memory::read_swap_usage_once(proc_root)
        });
        let outcome = match outcome {
            memory::SwapUsageOutcome::Present(value) => ReadOutcome::Value(value),
            memory::SwapUsageOutcome::Absent => ReadOutcome::Absent,
            memory::SwapUsageOutcome::Failed => ReadOutcome::Failed,
        };
        commit_attempt(&mut state.swap_usage, outcome, captured_at)
    } else {
        state.swap_usage.invalidate();
        cached_attempt(&state.swap_usage)
    };
    MemoryResult {
        usage,
        swap_usage,
        history: state
            .mem_history_sample_at
            .map(|captured_at| MetricSample::new(state.mem_history.clone(), captured_at)),
    }
}

/// Performs one network-counter attempt without cadence policy.
pub(crate) fn attempt_network_speed(
    state: &mut network::NetworkState,
    sys_root: &Path,
    device: &str,
    clock: ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> NetworkSpeedResult {
    let outcome = timed(timings, "net_speed", || {
        network::read_net_speed_once(sys_root, state, device, clock)
    });
    let outcome = match outcome {
        network::RateReadOutcome::Value(up, down) => ReadOutcome::Value((up, down)),
        network::RateReadOutcome::Baseline | network::RateReadOutcome::NoDelta => {
            ReadOutcome::Baseline
        }
        network::RateReadOutcome::Failed => ReadOutcome::Failed,
    };
    let reading = commit_attempt(&mut state.rate, outcome, clock.monotonic);
    let history_at = network::net_history_sample_at(state);
    NetworkSpeedResult {
        reading,
        up_history: history_at
            .map(|captured_at| MetricSample::new(state.net_up_history().to_vec(), captured_at)),
        down_history: history_at
            .map(|captured_at| MetricSample::new(state.net_down_history().to_vec(), captured_at)),
    }
}

/// Performs one Intel GPU frequency attempt without cadence policy.
pub(crate) fn attempt_intel_frequency(
    state: &mut IntelGpuState,
    path: &Path,
    clock: ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> IntelFrequencyResult {
    let value = timed(timings, "gpu_intel_freq", || {
        hwmon::read_path_int(Some(path))
    });
    IntelFrequencyResult {
        reading: commit_attempt(
            &mut state.frequency,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            clock.monotonic,
        ),
    }
}

/// Performs one Intel GPU usage attempt without cadence policy.
pub(crate) fn attempt_intel_usage(
    state: &mut IntelGpuState,
    proc_root: &Path,
    pci: &str,
    clock: ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> IntelUsageResult {
    let value = timed(timings, "gpu_intel_usage", || {
        gpu_intel::read_intel_gpu_metrics_once(proc_root, state, pci, clock)
    });
    match &value {
        gpu_intel::IntelGpuReadOutcome::Value(_) => state.usage_needs_comparable = false,
        gpu_intel::IntelGpuReadOutcome::Baseline => state.usage_needs_comparable = true,
        gpu_intel::IntelGpuReadOutcome::Failed => {}
    }
    let value = match value {
        gpu_intel::IntelGpuReadOutcome::Value(value) => ReadOutcome::Value(value),
        gpu_intel::IntelGpuReadOutcome::Baseline => ReadOutcome::Baseline,
        gpu_intel::IntelGpuReadOutcome::Failed => ReadOutcome::Failed,
    };
    let reading = commit_attempt(&mut state.usage, value, clock.monotonic);
    if reading.status == AttemptStatus::Captured
        && let Some(sample) = reading.sample.as_ref()
    {
        state.usage_cache.clone_from(&sample.value);
        state.usage_cache_sample_at = Some(sample.captured_at);
    }
    IntelUsageResult { reading }
}

/// Performs requested external-file and backlight attempts without cadence policy.
pub(crate) fn attempt_external(
    state: &mut ExternalState,
    sys_root: &Path,
    cfg: &Config,
    caps: &BTreeSet<Capability>,
    clock: &mut dyn FnMut() -> ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> ExternalResult {
    let brightness = if caps.contains(&Capability::ScreenBrightness) {
        let attempted_at = clock().monotonic;
        let value = timed(timings, "screen_brightness", || read_brightness(sys_root));
        commit_attempt(
            &mut state.brightness,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            attempted_at,
        )
    } else {
        cached_attempt(&state.brightness)
    };
    let updates =
        if caps.contains(&Capability::SystemUpdates) && !cfg.system_updates.file.is_empty() {
            let attempted_at = clock().monotonic;
            let path = PathBuf::from(&cfg.system_updates.file);
            let value = timed(timings, "system_updates", || read_count_file(&path));
            commit_attempt(
                &mut state.updates,
                value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
                attempted_at,
            )
        } else {
            cached_attempt(&state.updates)
        };
    let server = if caps.contains(&Capability::ServerCheck) && !cfg.server_check.file.is_empty() {
        let attempted_at = clock().monotonic;
        let path = PathBuf::from(&cfg.server_check.file);
        let value = timed(timings, "server_check", || read_server_file(&path));
        commit_attempt(
            &mut state.server,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            attempted_at,
        )
    } else {
        cached_attempt(&state.server)
    };
    ExternalResult {
        brightness,
        updates,
        server,
    }
}

/// Performs one real process-owner attempt with no cadence policy.
pub(crate) fn attempt_process(
    state: &mut process::ProcessState,
    proc_root: &Path,
    clock: ClockSnapshot,
) -> ProcessResult {
    let outcome = match process::read_top_process_once(proc_root, state, clock) {
        process::ProcessReadOutcome::Rows(rows) => ReadOutcome::Value(rows),
        process::ProcessReadOutcome::Baseline => ReadOutcome::Baseline,
        process::ProcessReadOutcome::Empty => ReadOutcome::Absent,
        process::ProcessReadOutcome::Failed => ReadOutcome::Failed,
    };
    let committed = commit_attempt(&mut state.panel, outcome, clock.monotonic);
    process_result(state, committed.status)
}

pub(super) fn process_result(
    state: &process::ProcessState,
    status: AttemptStatus,
) -> ProcessResult {
    let sample = state.panel.latest.as_ref().map(|sample| {
        MetricSample::new(
            ProcessReadings {
                summary: sample
                    .value
                    .iter()
                    .take(process::TOP_PROCESS_COUNT)
                    .map(|row| TopProcessSummary {
                        command: row.command.clone(),
                        cpu_percent: row.cpu_percent,
                    })
                    .collect(),
                full: sample.value.clone(),
            },
            sample.captured_at,
        )
    });
    ProcessResult {
        reading: AttemptResult {
            sample,
            attempted_at: state.panel.attempted_at,
            failed_at: state.panel.failed_at,
            latest_attempt_failed: state.panel.latest_attempt_failed,
            status,
        },
    }
}

/// No-op when `timings` is `None`; otherwise records accumulated wall-clock
/// elapsed under `key`. Mirrors `timed_section` in `src/sensors.py`.
///
/// Deterministic tests pass `None`, so [`std::time::Instant::now`] is never
/// consulted in the deterministic path. The profiling subcommand passes
/// `Some(&mut Timings)` and reads the accumulated per-section durations.
pub(super) fn timed<R>(
    timings: &mut Option<&mut Timings>,
    key: &str,
    work: impl FnOnce() -> R,
) -> R {
    match timings.as_mut() {
        Some(map) => {
            let start = std::time::Instant::now();
            let result = work();
            let elapsed = start.elapsed();
            map.entry(key.to_owned())
                .and_modify(|acc| *acc += elapsed)
                .or_insert(elapsed);
            result
        }
        None => work(),
    }
}

/// Reads the screen brightness percentage from the first usable backlight.
///
/// Mirrors `_read_brightness` in `src/sensors.py`: `cur * 100 // max` over the
/// first backlight with both files and a non-zero max. A missing
/// `/sys/class/backlight` directory degrades to `None` (the contract's
/// "absent hardware maps to None/empty"; Python would raise on the missing
/// directory).
fn read_brightness(sys_root: &Path) -> Option<i32> {
    let entries = std::fs::read_dir(sys_root.join("class/backlight")).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let cur_f = path.join("brightness");
        let max_f = path.join("max_brightness");
        if !cur_f.exists() || !max_f.exists() {
            continue;
        }
        let cur = read_trimmed_i64(&cur_f);
        let max = read_trimmed_i64(&max_f);
        if let (Some(cur), Some(max)) = (cur, max)
            && max > 0
        {
            return Some(((cur * 100) / max) as i32);
        }
    }
    None
}

/// Reads an integer (e.g. a pending-updates count) from an externally-written
/// file. Mirrors `_read_count_file` in `src/sensors.py`: `None` when the file is
/// missing/unreadable/empty/not a valid integer.
fn read_count_file(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()
}

/// Reads a server-reachability flag from an externally-written file.
///
/// Mirrors `_read_server_file` in `src/sensors.py`: `"1"` → `true`,
/// `"0"` → `false`, anything else (or missing/unreadable) → `None`.
fn read_server_file(path: &Path) -> Option<bool> {
    let value = std::fs::read_to_string(path).ok()?;
    match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// Parses a sysfs file as a signed integer, tolerating surrounding whitespace.
fn read_trimmed_i64(path: &Path) -> Option<i64> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()
}
