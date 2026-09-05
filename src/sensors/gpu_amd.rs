//! Direct AMDGPU sysfs discovery and independently retained, demand-driven samples.
//!
//! The caller owns cadence and supplies confirmed inventory. Failed discovery must retain that inventory before reconciling this owner.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::domain::Metric;
use crate::domain::readings::{
    AmdGpuMemoryPaths, AmdGpuMemoryReading, AmdGpuSource, RetainedMetricSample,
};

/// Samples and attempt times in display units, independent of sibling failures.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AmdGpuSamples {
    /// Device-wide utilization, percent.
    pub usage: RetainedMetricSample<i32>,
    /// Combined VCN activity, percent.
    pub codec_usage: RetainedMetricSample<i32>,
    /// Driver VRAM allocation counters, bytes.
    pub memory: RetainedMetricSample<AmdGpuMemoryReading>,
    /// Graphics-core clock, MHz.
    pub frequency: RetainedMetricSample<u32>,
    /// Edge temperature, Celsius.
    pub temperature: RetainedMetricSample<i32>,
    /// Average package power, watts.
    pub power: RetainedMetricSample<u32>,
    /// Measured fan speed, RPM.
    pub fan_speed: RetainedMetricSample<u32>,
}

/// Selected source and retained samples. No timers, command dependencies, or freshness policy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AmdGpuState {
    source: Option<AmdGpuSource>,
    samples: AmdGpuSamples,
}

impl AmdGpuState {
    /// Current retained samples and per-metric attempt outcomes.
    #[must_use]
    pub const fn samples(&self) -> &AmdGpuSamples {
        &self.samples
    }

    pub(crate) fn codec_outcome(&self) -> super::gpu_history::DecoderOutcome {
        use super::gpu_history::DecoderOutcome;
        if self
            .source
            .as_ref()
            .is_none_or(|source| source.codec_usage_path.is_none())
        {
            DecoderOutcome::ConfirmedAbsent
        } else if let Some(sample) = &self.samples.codec_usage.latest {
            DecoderOutcome::Value(sample.value)
        } else if self.samples.codec_usage.latest_attempt_failed {
            DecoderOutcome::TransientFailure
        } else {
            DecoderOutcome::Unmeasured
        }
    }

    pub(crate) fn latest_history_point(
        &self,
    ) -> Option<crate::domain::readings::MetricSample<(Option<i32>, Option<i32>)>> {
        // The pair becomes observable after its latest graph-field attempt; a failed sibling retains its older value.
        let at = self
            .samples
            .usage
            .attempted_at
            .max(self.samples.codec_usage.attempted_at)?;
        Some(crate::domain::readings::MetricSample::new(
            (
                self.samples
                    .usage
                    .latest
                    .as_ref()
                    .map(|sample| sample.value),
                self.samples
                    .codec_usage
                    .latest
                    .as_ref()
                    .map(|sample| sample.value),
            ),
            at,
        ))
    }

    /// Clears all samples on device replacement, or only affected samples on capability/path changes.
    pub fn reconcile_source(&mut self, source: Option<&AmdGpuSource>) {
        if self.source.as_ref() == source {
            return;
        }
        if let (Some(old), Some(new)) = (&self.source, source) {
            self.invalidate(old.changed_metrics(new));
        } else {
            self.samples = AmdGpuSamples::default();
        }
        self.source = source.cloned();
    }

    /// Attempts only the requested metrics once. Call with each job's demanded subset.
    ///
    /// A missing discovered path is confirmed absence; failure to read an existing source retains its last valid sample, including when the file vanished between discovery and sampling.
    pub fn sample(&mut self, metrics: &BTreeSet<Metric>, attempted_at: Duration) {
        let source = self.source.as_ref();
        for metric in metrics {
            match metric {
                Metric::GpuAmdUsage => sample_path(
                    &mut self.samples.usage,
                    source.and_then(|s| s.usage_path.as_deref()),
                    read_percent,
                    attempted_at,
                ),
                Metric::GpuAmdCodecUsage => sample_path(
                    &mut self.samples.codec_usage,
                    source.and_then(|s| s.codec_usage_path.as_deref()),
                    read_percent,
                    attempted_at,
                ),
                Metric::GpuAmdMemUsage => sample_path(
                    &mut self.samples.memory,
                    source.and_then(|s| s.memory_paths.as_ref()),
                    read_memory,
                    attempted_at,
                ),
                Metric::GpuAmdFreq => sample_path(
                    &mut self.samples.frequency,
                    source.and_then(|s| s.freq_path.as_deref()),
                    |path| read_scaled_u32(path, 1_000_000),
                    attempted_at,
                ),
                Metric::GpuAmdTemp => sample_path(
                    &mut self.samples.temperature,
                    source.and_then(|s| s.temp_path.as_deref()),
                    read_temperature,
                    attempted_at,
                ),
                Metric::GpuAmdPower => sample_path(
                    &mut self.samples.power,
                    source.and_then(|s| s.power_path.as_deref()),
                    |path| read_scaled_u32(path, 1_000_000),
                    attempted_at,
                ),
                Metric::GpuAmdFanSpeed => sample_path(
                    &mut self.samples.fan_speed,
                    source.and_then(|s| s.fan_speed_path.as_deref()),
                    |path| read_scaled_u32(path, 1),
                    attempted_at,
                ),
                _ => {}
            }
        }
    }
}

fn sample_path<T, P: ?Sized>(
    sample: &mut RetainedMetricSample<T>,
    path: Option<&P>,
    read: impl FnOnce(&P) -> Option<T>,
    attempted_at: Duration,
) {
    if let Some(path) = path {
        sample.record(read(path), attempted_at);
    } else {
        sample.record_absence(attempted_at);
    }
}

fn read_number<T: std::str::FromStr>(path: &Path) -> Option<T> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_percent(path: &Path) -> Option<i32> {
    read_number(path).filter(|value| (0..=100).contains(value))
}

// Integer display units truncate fractional MHz, watts, and Celsius toward zero.
fn read_scaled_u32(path: &Path, divisor: u64) -> Option<u32> {
    u32::try_from(read_number::<u64>(path)? / divisor).ok()
}

fn read_temperature(path: &Path) -> Option<i32> {
    let millidegrees = read_number::<i64>(path)?;
    if !(-273_150..1_000_000).contains(&millidegrees) {
        return None;
    }
    i32::try_from(millidegrees / 1_000).ok()
}

fn read_memory(paths: &AmdGpuMemoryPaths) -> Option<AmdGpuMemoryReading> {
    let reading = AmdGpuMemoryReading {
        used_bytes: read_number(&paths.used)?,
        total_bytes: read_number(&paths.total)?,
    };
    reading.percent()?;
    Some(reading)
}

/// Selects the lowest canonical PCI identity after completely inspecting AMDGPU candidates.
///
/// # Errors
/// Returns an I/O or invalid-data error for incomplete enumeration, malformed identity, or broken source paths. `Ok(None)` means a complete scan found no qualifying device.
pub fn detect_amd_gpu(sys_root: &Path) -> io::Result<Option<AmdGpuSource>> {
    let mut selected: Option<AmdGpuSource> = None;
    for card in numbered_entries(&sys_root.join("class/drm"), "card")? {
        let device = fs::canonicalize(card.join("device"))?;
        let driver = device.join("driver");
        if !entry_exists(&driver)? {
            continue;
        }
        if fs::canonicalize(driver)?
            .file_name()
            .and_then(|name| name.to_str())
            != Some("amdgpu")
        {
            continue;
        }
        let vendor = read_pci_value(&device.join("vendor"), 0xffff)?;
        if vendor != 0x1002 {
            continue;
        }
        if read_pci_value(&device.join("class"), 0xffffff)? >> 16 != 0x03 {
            continue;
        }
        let pci_identity = pci_identity(&device)?;
        let used = optional_file(device.join("mem_info_vram_used"))?;
        let total = optional_file(device.join("mem_info_vram_total"))?;
        let mut source = AmdGpuSource {
            pci_identity,
            usage_path: optional_file(device.join("gpu_busy_percent"))?,
            codec_usage_path: optional_file(device.join("vcn_busy_percent"))?,
            memory_paths: used
                .zip(total)
                .map(|(used, total)| AmdGpuMemoryPaths { used, total }),
            ..AmdGpuSource::default()
        };
        discover_hwmon(&device, &mut source)?;
        if selected
            .as_ref()
            .is_none_or(|old| source.pci_identity < old.pci_identity)
        {
            selected = Some(source);
        }
    }
    Ok(selected)
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn read_pci_value(path: &Path, maximum: u32) -> io::Result<u32> {
    fs::read_to_string(path)?
        .trim()
        .strip_prefix("0x")
        .and_then(|value| u32::from_str_radix(value, 16).ok())
        .filter(|value| *value <= maximum)
        .ok_or_else(|| invalid_data("malformed PCI value"))
}

fn pci_identity(device: &Path) -> io::Result<String> {
    let name = device
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid_data("invalid PCI path"))?;
    let bytes = name.as_bytes();
    if bytes.len() != 12
        || bytes[4] != b':'
        || bytes[7] != b':'
        || bytes[10] != b'.'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| matches!(i, 4 | 7 | 10) || b.is_ascii_hexdigit())
        || u8::from_str_radix(&name[8..10], 16).map_or(true, |slot| slot > 31)
        || !(b'0'..=b'7').contains(&bytes[11])
    {
        return Err(invalid_data("malformed PCI identity"));
    }
    Ok(name.to_ascii_lowercase())
}

fn numbered(name: &str, prefix: &str) -> bool {
    name.strip_prefix(prefix)
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}

fn numbered_entries(root: &Path, prefix: &str) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| numbered(name, prefix))
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

// Unlike Path::exists, a dangling symlink is an incomplete inspection, not confirmed absence.
fn entry_exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn optional_file(path: PathBuf) -> io::Result<Option<PathBuf>> {
    if !entry_exists(&path)? {
        return Ok(None);
    }
    if !fs::metadata(&path)?.is_file() {
        return Err(invalid_data("expected a sysfs file"));
    }
    Ok(Some(path))
}

fn discover_hwmon(device: &Path, source: &mut AmdGpuSource) -> io::Result<()> {
    let root = device.join("hwmon");
    if !entry_exists(&root)? {
        return Ok(());
    }
    for hwmon in numbered_entries(&root, "hwmon")? {
        let hwmon = fs::canonicalize(hwmon)?;
        if fs::read_to_string(hwmon.join("name"))?.trim() != "amdgpu" {
            continue;
        }
        let temp = labeled_input(&hwmon, "temp", "edge")?;
        let freq = labeled_input(&hwmon, "freq", "sclk")?;
        let power = optional_file(hwmon.join("power1_average"))?;
        let fan = optional_file(hwmon.join("fan1_input"))?;
        source.temp_path = source.temp_path.take().or(temp);
        source.freq_path = source.freq_path.take().or(freq);
        source.power_path = source.power_path.take().or(power);
        source.fan_speed_path = source.fan_speed_path.take().or(fan);
    }
    Ok(())
}

fn labeled_input(hwmon: &Path, prefix: &str, label: &str) -> io::Result<Option<PathBuf>> {
    let mut inputs = Vec::new();
    for entry in fs::read_dir(hwmon)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(stem) = name.to_str().and_then(|name| name.strip_suffix("_label")) else {
            continue;
        };
        if numbered(stem, prefix)
            && fs::read_to_string(entry.path())?
                .trim()
                .eq_ignore_ascii_case(label)
            && let Some(input) = optional_file(hwmon.join(format!("{stem}_input")))?
        {
            inputs.push(input);
        }
    }
    inputs.sort();
    Ok(inputs.into_iter().next())
}

#[cfg(test)]
mod tests;

pub(crate) const FAST_METRICS: &[Metric] = &[
    Metric::GpuAmdUsage,
    Metric::GpuAmdCodecUsage,
    Metric::GpuAmdMemUsage,
    Metric::GpuAmdFreq,
];
pub(crate) const SLOW_METRICS: &[Metric] = &[
    Metric::GpuAmdTemp,
    Metric::GpuAmdPower,
    Metric::GpuAmdFanSpeed,
];

pub(crate) fn supported_metrics(source: &AmdGpuSource) -> BTreeSet<Metric> {
    [
        (Metric::GpuAmdUsage, source.usage_path.is_some()),
        (Metric::GpuAmdCodecUsage, source.codec_usage_path.is_some()),
        (Metric::GpuAmdMemUsage, source.memory_paths.is_some()),
        (Metric::GpuAmdFreq, source.freq_path.is_some()),
        (Metric::GpuAmdTemp, source.temp_path.is_some()),
        (Metric::GpuAmdPower, source.power_path.is_some()),
        (Metric::GpuAmdFanSpeed, source.fan_speed_path.is_some()),
    ]
    .into_iter()
    .filter_map(|(metric, present)| present.then_some(metric))
    .collect()
}

pub(crate) fn source_for_metrics(
    source: &AmdGpuSource,
    metrics: &BTreeSet<Metric>,
) -> AmdGpuSource {
    let mut source = source.clone();
    if !metrics.contains(&Metric::GpuAmdUsage) {
        source.usage_path = None;
    }
    if !metrics.contains(&Metric::GpuAmdCodecUsage) {
        source.codec_usage_path = None;
    }
    if !metrics.contains(&Metric::GpuAmdMemUsage) {
        source.memory_paths = None;
    }
    if !metrics.contains(&Metric::GpuAmdFreq) {
        source.freq_path = None;
    }
    if !metrics.contains(&Metric::GpuAmdTemp) {
        source.temp_path = None;
    }
    if !metrics.contains(&Metric::GpuAmdPower) {
        source.power_path = None;
    }
    if !metrics.contains(&Metric::GpuAmdFanSpeed) {
        source.fan_speed_path = None;
    }
    source
}

impl AmdGpuState {
    pub(crate) fn invalidate(&mut self, metrics: impl IntoIterator<Item = Metric>) {
        for metric in metrics {
            match metric {
                Metric::GpuAmdUsage => self.samples.usage.invalidate(),
                Metric::GpuAmdCodecUsage => self.samples.codec_usage.invalidate(),
                Metric::GpuAmdMemUsage => self.samples.memory.invalidate(),
                Metric::GpuAmdFreq => self.samples.frequency.invalidate(),
                Metric::GpuAmdTemp => self.samples.temperature.invalidate(),
                Metric::GpuAmdPower => self.samples.power.invalidate(),
                Metric::GpuAmdFanSpeed => self.samples.fan_speed.invalidate(),
                _ => {}
            }
        }
    }
}
