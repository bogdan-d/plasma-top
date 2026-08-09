//! NVIDIA GPU discovery, metric sampling, and fallback state.
//!
//! NVML itself stays behind [`NvmlFacade`]. The production adapter owns lazy loading of the optional library and GPU-0 handle, this module owns NVIDIA sample and source-selection state, and synchronous collection applies freshness decisions.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

use crate::domain::boundary::{ClockSnapshot, CommandRunner, CommandStatus};
use crate::domain::readings::RetainedMetricSample;

#[cfg(feature = "nvml")]
use nvml_wrapper::{
    Nvml, enum_wrappers::device::TemperatureSensor, error::NvmlError as WrapperNvmlError,
};

/// `nvidia-smi` executable token used by the Python backend.
pub const NVIDIA_SMI_PROGRAM: &str = "nvidia-smi";
/// Timeout for the `nvidia-smi` fallback.
pub const NVIDIA_SMI_TIMEOUT: Duration = Duration::from_secs(5);
/// Freshness budget for the process-backed fallback sample.
pub const GPU_CACHE_TTL: Duration = Duration::from_secs(3);
/// NVML samples are due on every requested synchronous pass.
pub const GPU_CACHE_TTL_NVML: Duration = Duration::ZERO;

const NVIDIA_VENDOR: &str = "0x10de";
const DISPLAY_CLASS_PREFIX: &str = "0x03";
const NVIDIA_SMI_QUERY: &str =
    "--query-gpu=temperature.gpu,utilization.gpu,utilization.memory,fan.speed,utilization.decoder";
const NVIDIA_SMI_FORMAT: &str = "--format=csv,noheader,nounits";

/// One NVIDIA reading in formatter order: temp, usage, memory, decoder, fan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NvidiaMetrics {
    /// GPU temperature in °C.
    pub temp_celsius: Option<i32>,
    /// GPU utilization percentage.
    pub usage_percent: Option<i32>,
    /// GPU memory-controller utilization percentage.
    pub memory_percent: Option<i32>,
    /// Decoder utilization percentage.
    pub decoder_percent: Option<i32>,
    /// Fan-speed percentage.
    pub fan_percent: Option<i32>,
}

/// Outcome of one optional NVML metric read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmlOptionalMetric {
    /// The metric was captured.
    Value(i32),
    /// NVML confirmed that this device does not support the metric.
    NotSupported,
    /// The operational read failed transiently.
    Failed,
}

/// One NVML device read with typed optional metric outcomes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NvmlMetrics {
    /// GPU temperature in °C.
    pub temp_celsius: i32,
    /// GPU utilization percentage.
    pub usage_percent: i32,
    /// GPU memory-controller utilization percentage.
    pub memory_percent: i32,
    /// Decoder read outcome.
    pub decoder: NvmlOptionalMetric,
    /// Fan read outcome.
    pub fan: NvmlOptionalMetric,
}

impl From<NvidiaMetrics> for NvmlMetrics {
    fn from(metrics: NvidiaMetrics) -> Self {
        Self {
            temp_celsius: metrics.temp_celsius.unwrap_or_default(),
            usage_percent: metrics.usage_percent.unwrap_or_default(),
            memory_percent: metrics.memory_percent.unwrap_or_default(),
            decoder: metrics
                .decoder_percent
                .map_or(NvmlOptionalMetric::NotSupported, NvmlOptionalMetric::Value),
            fan: metrics
                .fan_percent
                .map_or(NvmlOptionalMetric::NotSupported, NvmlOptionalMetric::Value),
        }
    }
}

/// Latest NVIDIA metrics and fallback-selection state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GpuCache {
    /// GPU temperature in °C.
    pub temp_celsius: Option<i32>,
    /// GPU usage percentage.
    pub usage_percent: Option<i32>,
    /// GPU memory usage percentage.
    pub memory_percent: Option<i32>,
    /// GPU decoder usage percentage.
    pub decoder_percent: Option<i32>,
    /// GPU fan percentage.
    pub fan_percent: Option<i32>,
    /// Whether NVML initialization failed for the current confirmed device presence.
    pub nvml_init_failed: bool,
    /// Monotonic instant of the latest successful metric sample.
    pub sampled_at: Option<Duration>,
    /// Monotonic instant of the latest attempt, successful or not.
    pub attempted_at: Option<Duration>,
    /// Monotonic instant of the latest source failure.
    pub failed_at: Option<Duration>,
    /// Monotonic instant of the latest NVML attempt.
    pub nvml_attempted_at: Option<Duration>,
    /// Monotonic instant of the latest NVML failure.
    pub nvml_failed_at: Option<Duration>,
    /// Whether the latest NVML attempt failed.
    pub nvml_latest_attempt_failed: bool,
    /// Monotonic instant of the latest command-fallback attempt.
    pub fallback_attempted_at: Option<Duration>,
    /// Monotonic instant of the latest command-fallback failure.
    pub fallback_failed_at: Option<Duration>,
    /// Whether the latest command-fallback attempt failed.
    pub fallback_latest_attempt_failed: bool,
    pub(super) decoder: RetainedMetricSample<i32>,
    pub(super) fan: RetainedMetricSample<i32>,
}

/// Mutable sample state owned by the NVIDIA domain.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NvidiaState {
    /// Latest NVIDIA reading.
    pub cache: GpuCache,
}

impl NvidiaState {
    pub(crate) fn reconcile_source(&mut self, active: bool) {
        if !active {
            self.cache = GpuCache::default();
        }
    }
}

/// NVML failure class needed by the fallback source-selection state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmlError {
    /// Library initialization or GPU-0 handle lookup failed for this presence generation.
    Init,
    /// A metric read failed; retry NVML next poll.
    Read,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NvidiaSourceOutcome {
    Captured,
    Failed,
}

/// Narrow NVML boundary consumed by NVIDIA orchestration.
pub trait NvmlFacade {
    /// Reads GPU 0. Optional fan/decoder values remain `None` when unsupported.
    ///
    /// # Errors
    ///
    /// Returns [`NvmlError::Init`] for initialization/handle failure and
    /// [`NvmlError::Read`] when mandatory metric reads fail.
    fn read_device_zero(&mut self) -> Result<NvmlMetrics, NvmlError>;
}

/// Runtime-loaded production NVML adapter for NVIDIA GPU 0.
///
/// Initialization is lazy so the daemon's fast first paint does not load the
/// driver library. Backend suppression belongs to [`NvidiaState`], allowing a
/// confirmed device removal to clear it before a later re-add.
#[cfg(feature = "nvml")]
#[derive(Debug, Default)]
pub struct ProductionNvml {
    nvml: Option<Nvml>,
}

#[cfg(feature = "nvml")]
impl ProductionNvml {
    /// Creates an uninitialized adapter. NVML is loaded on the first metric
    /// read, not during daemon construction.
    #[must_use]
    pub const fn new() -> Self {
        Self { nvml: None }
    }
}

#[cfg(feature = "nvml")]
impl NvmlFacade for ProductionNvml {
    fn read_device_zero(&mut self) -> Result<NvmlMetrics, NvmlError> {
        if self.nvml.is_none() {
            match Nvml::init() {
                Ok(nvml) => self.nvml = Some(nvml),
                Err(_) => return Err(NvmlError::Init),
            }
        }

        let Some(nvml) = self.nvml.as_ref() else {
            return Err(NvmlError::Init);
        };
        let device = nvml.device_by_index(0).map_err(|_| NvmlError::Init)?;
        let temp = device
            .temperature(TemperatureSensor::Gpu)
            .map_err(|_| NvmlError::Read)?;
        let usage = device.utilization_rates().map_err(|_| NvmlError::Read)?;

        Ok(NvmlMetrics {
            temp_celsius: u32_to_i32(temp).ok_or(NvmlError::Read)?,
            usage_percent: u32_to_i32(usage.gpu).ok_or(NvmlError::Read)?,
            memory_percent: u32_to_i32(usage.memory).ok_or(NvmlError::Read)?,
            decoder: optional_nvml(device.decoder_utilization().map(|value| value.utilization)),
            fan: optional_nvml(device.fan_speed(0)),
        })
    }
}

#[cfg(feature = "nvml")]
fn u32_to_i32(value: u32) -> Option<i32> {
    i32::try_from(value).ok()
}

#[cfg(feature = "nvml")]
fn optional_nvml(result: Result<u32, WrapperNvmlError>) -> NvmlOptionalMetric {
    match result {
        Ok(value) => {
            u32_to_i32(value).map_or(NvmlOptionalMetric::Failed, NvmlOptionalMetric::Value)
        }
        Err(WrapperNvmlError::NotSupported) => NvmlOptionalMetric::NotSupported,
        Err(_) => NvmlOptionalMetric::Failed,
    }
}

/// Detects an NVIDIA display-class PCI device below `sys_root`.
#[must_use]
pub fn detect_nvidia(sys_root: &Path) -> bool {
    detect_nvidia_outcome(sys_root).unwrap_or(false)
}

/// Detects NVIDIA presence without flattening an incomplete PCI enumeration.
pub(crate) fn detect_nvidia_outcome(sys_root: &Path) -> io::Result<bool> {
    let devices = fs::read_dir(sys_root.join("bus/pci/devices"))?;
    let mut incomplete = false;
    for entry in devices {
        let Ok(entry) = entry else {
            incomplete = true;
            continue;
        };
        let device = entry.path();
        let Ok(vendor) = fs::read_to_string(device.join("vendor")) else {
            incomplete = true;
            continue;
        };
        if parse_pci_value(&vendor).is_err() {
            incomplete = true;
            continue;
        }
        if vendor.trim() != NVIDIA_VENDOR {
            continue;
        }
        let Ok(class) = fs::read_to_string(device.join("class")) else {
            incomplete = true;
            continue;
        };
        if parse_pci_value(&class).is_err() {
            incomplete = true;
            continue;
        }
        if class.trim().starts_with(DISPLAY_CLASS_PREFIX) {
            return Ok(true);
        }
    }
    if incomplete {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "incomplete PCI enumeration",
        ))
    } else {
        Ok(false)
    }
}

fn parse_pci_value(value: &str) -> io::Result<u32> {
    value
        .trim()
        .strip_prefix("0x")
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed PCI value"))
}

/// Caps a metric at 99 while preserving absence and negative values.
#[must_use]
pub fn nvidia_cap(value: Option<i32>) -> Option<i32> {
    value.map(|value| value.min(99))
}

/// Compatibility wrapper that performs one NVML read and, when needed, one fallback read; production synchronous collection keeps the two source freshness decisions independent.
pub fn read_nvidia(
    state: &mut GpuCache,
    nvml: Option<&mut dyn NvmlFacade>,
    runner: &mut dyn CommandRunner,
    clock: ClockSnapshot,
) -> NvidiaMetrics {
    let nvml_outcome = if state.nvml_init_failed {
        None
    } else {
        nvml.map(|facade| attempt_nvml(state, facade, clock.monotonic))
    };
    if nvml_outcome != Some(NvidiaSourceOutcome::Captured) {
        let _ = attempt_fallback(state, runner, clock.monotonic);
    }
    metrics_from_cache(state)
}

/// Performs one NVML source read and records NVML-specific diagnostics.
pub(super) fn attempt_nvml(
    state: &mut GpuCache,
    nvml: &mut dyn NvmlFacade,
    captured_at: Duration,
) -> NvidiaSourceOutcome {
    state.attempted_at = Some(captured_at);
    state.nvml_attempted_at = Some(captured_at);
    match nvml.read_device_zero() {
        Ok(metrics) => {
            commit_optional(&mut state.decoder, metrics.decoder, captured_at);
            commit_optional(&mut state.fan, metrics.fan, captured_at);
            let metrics = NvidiaMetrics {
                temp_celsius: nvidia_cap(Some(metrics.temp_celsius)),
                usage_percent: nvidia_cap(Some(metrics.usage_percent)),
                memory_percent: nvidia_cap(Some(metrics.memory_percent)),
                decoder_percent: state.decoder.latest.as_ref().map(|sample| sample.value),
                fan_percent: state.fan.latest.as_ref().map(|sample| sample.value),
            };
            store_metrics(state, metrics, captured_at);
            state.nvml_latest_attempt_failed = false;
            NvidiaSourceOutcome::Captured
        }
        Err(error) => {
            if error == NvmlError::Init {
                state.nvml_init_failed = true;
            }
            state.failed_at = Some(captured_at);
            state.nvml_failed_at = Some(captured_at);
            state.nvml_latest_attempt_failed = true;
            NvidiaSourceOutcome::Failed
        }
    }
}

/// Performs one `nvidia-smi` source read and records fallback-specific diagnostics.
pub(super) fn attempt_fallback(
    state: &mut GpuCache,
    runner: &mut dyn CommandRunner,
    captured_at: Duration,
) -> NvidiaSourceOutcome {
    state.attempted_at = Some(captured_at);
    state.fallback_attempted_at = Some(captured_at);
    if let Some(mut metrics) = read_nvidia_smi_attempt(runner) {
        commit_fallback_optional(&mut state.decoder, metrics.decoder_percent, captured_at);
        commit_fallback_optional(&mut state.fan, metrics.fan_percent, captured_at);
        metrics.decoder_percent = state.decoder.latest.as_ref().map(|sample| sample.value);
        metrics.fan_percent = state.fan.latest.as_ref().map(|sample| sample.value);
        store_metrics(state, metrics, captured_at);
        state.fallback_latest_attempt_failed = false;
        NvidiaSourceOutcome::Captured
    } else {
        state.decoder.record_failure(captured_at);
        state.fan.record_failure(captured_at);
        state.failed_at = Some(captured_at);
        state.fallback_failed_at = Some(captured_at);
        state.fallback_latest_attempt_failed = true;
        NvidiaSourceOutcome::Failed
    }
}

fn commit_fallback_optional(
    retained: &mut RetainedMetricSample<i32>,
    value: Option<i32>,
    attempted_at: Duration,
) {
    if let Some(value) = value {
        retained.record_value(value, attempted_at);
    } else {
        retained.record_absence(attempted_at);
    }
}

fn commit_optional(
    retained: &mut RetainedMetricSample<i32>,
    outcome: NvmlOptionalMetric,
    attempted_at: Duration,
) {
    match outcome {
        NvmlOptionalMetric::Value(value) => {
            retained.record_value(nvidia_cap(Some(value)).unwrap_or(value), attempted_at);
        }
        NvmlOptionalMetric::NotSupported => retained.record_absence(attempted_at),
        NvmlOptionalMetric::Failed => retained.record_failure(attempted_at),
    }
}

/// Reads and parses the `nvidia-smi` CSV fallback.
#[must_use]
pub fn read_nvidia_smi(runner: &mut dyn CommandRunner) -> NvidiaMetrics {
    read_nvidia_smi_attempt(runner).unwrap_or_default()
}

fn read_nvidia_smi_attempt(runner: &mut dyn CommandRunner) -> Option<NvidiaMetrics> {
    let args = [
        OsString::from(NVIDIA_SMI_QUERY),
        OsString::from(NVIDIA_SMI_FORMAT),
    ];
    let Ok(output) = runner.run(Path::new(NVIDIA_SMI_PROGRAM), &args, NVIDIA_SMI_TIMEOUT) else {
        return None;
    };
    if output.status != CommandStatus::Exit(0) {
        return None;
    }
    let Ok(stdout) = std::str::from_utf8(&output.stdout) else {
        return None;
    };
    parse_nvidia_smi(stdout)
        .filter(|metrics| metrics.temp_celsius.is_some() && metrics.usage_percent.is_some())
}

fn parse_nvidia_smi(stdout: &str) -> Option<NvidiaMetrics> {
    let parts: Vec<&str> = stdout.split(',').map(str::trim).collect();
    Some(NvidiaMetrics {
        temp_celsius: parse_metric(parts.first()?),
        usage_percent: parse_metric(parts.get(1)?),
        memory_percent: parse_metric(parts.get(2)?),
        decoder_percent: parse_metric(parts.get(4)?),
        fan_percent: parse_metric(parts.get(3)?),
    })
}

fn parse_metric(value: &str) -> Option<i32> {
    nvidia_cap(value.parse::<i32>().ok())
}

pub(super) fn metrics_from_cache(cache: &GpuCache) -> NvidiaMetrics {
    NvidiaMetrics {
        temp_celsius: cache.temp_celsius,
        usage_percent: cache.usage_percent,
        memory_percent: cache.memory_percent,
        decoder_percent: cache.decoder_percent,
        fan_percent: cache.fan_percent,
    }
}

fn store_metrics(cache: &mut GpuCache, metrics: NvidiaMetrics, sampled_at: Duration) {
    cache.temp_celsius = metrics.temp_celsius;
    cache.usage_percent = metrics.usage_percent;
    cache.memory_percent = metrics.memory_percent;
    cache.decoder_percent = metrics.decoder_percent;
    cache.fan_percent = metrics.fan_percent;
    cache.sampled_at = Some(sampled_at);
}

#[cfg(all(test, feature = "test-support"))]
mod tests;
