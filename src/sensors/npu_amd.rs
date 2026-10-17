//! AMD NPU telemetry from the APU's `amdgpu` `gpu_metrics` v3.0 table.
//!
//! Reading the table never opens the `amdxdna` accel device, so polling cannot wake a suspended NPU. A suspended NPU reports zero busy and power. This is the one place PlasmaTop parses binary `gpu_metrics`; AMDGPU items keep their text sources.

use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

use crate::config::Config;
use crate::domain::readings::{AmdNpuSource, RetainedMetricSample};

use super::gpu_amd::{entry_exists, invalid_data, numbered_entries, optional_file, pci_identity};

const FORMAT_REVISION: u8 = 3;
const CONTENT_REVISION: u8 = 0;
const MIN_STRUCTURE_SIZE: usize = 264;
const HEADER_SIZE: usize = 4;
const IPU_ACTIVITY_OFFSET: usize = 46;
const IPU_COLUMNS: usize = 8;
const IPU_POWER_OFFSET: usize = 116;
const IPU_CLOCK_OFFSET: usize = 180;
const UNAVAILABLE: u16 = 0xFFFF;

/// Samples and attempt times, each metric retained independently.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AmdNpuSamples {
    /// Mean column busy, percent.
    pub usage: RetainedMetricSample<i32>,
    /// NPU power, milliwatts.
    pub power_mw: RetainedMetricSample<u16>,
    /// NPU clock, MHz.
    pub frequency: RetainedMetricSample<u16>,
}

/// Whether the graphs page is configured with its NPU chart.
///
/// Unlike the GPU and network charts, NPU history needs both conditions, so a graphs page without the chart costs no NPU work.
pub(crate) fn graph_enabled(cfg: &Config) -> bool {
    cfg.pages.order.iter().any(|page| page == "graphs")
        && cfg.pages.graph_order.iter().any(|chart| chart == "npu")
}

/// Detected source, retained samples, and busy history. No timers or freshness policy.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AmdNpuState {
    source: Option<AmdNpuSource>,
    samples: AmdNpuSamples,
    usage_history: Vec<i32>,
    history_sample_at: Option<Duration>,
}

impl AmdNpuState {
    /// Current retained samples and attempt outcomes.
    #[must_use]
    pub const fn samples(&self) -> &AmdNpuSamples {
        &self.samples
    }

    /// Busy history for the graphs page, oldest first.
    #[must_use]
    pub fn usage_history(&self) -> &[i32] {
        &self.usage_history
    }

    /// Capture time of the newest history point.
    #[must_use]
    pub const fn history_sample_at(&self) -> Option<Duration> {
        self.history_sample_at
    }

    /// Clears samples and history when the source appears, disappears, or changes identity.
    pub fn reconcile_source(&mut self, source: Option<&AmdNpuSource>) {
        if self.source.as_ref() != source {
            *self = Self {
                source: source.cloned(),
                ..Self::default()
            };
        }
    }

    /// Appends one busy point, keeping at most `max_len` points.
    pub fn append_history(&mut self, usage: i32, captured_at: Duration, max_len: usize) {
        self.history_sample_at = Some(captured_at);
        self.usage_history.push(usage);
        let excess = self.usage_history.len().saturating_sub(max_len);
        self.usage_history.drain(..excess);
    }

    /// Clears the busy history.
    pub fn reset_history(&mut self) {
        self.usage_history.clear();
        self.history_sample_at = None;
    }

    /// Clears every retained sample and attempt time.
    pub fn invalidate(&mut self) {
        self.samples = AmdNpuSamples::default();
    }

    /// Reads the table once and records one attempt for every metric.
    pub fn sample(&mut self, attempted_at: Duration) {
        let Some(source) = &self.source else {
            self.record_absence(attempted_at);
            return;
        };
        match fs::read(&source.metrics_path).map(|bytes| parse(&bytes)) {
            Ok(Table::Supported(fields)) => self.record(fields, attempted_at),
            // A changed layout must never mix with earlier values; rediscovery decides whether the source still qualifies.
            Ok(Table::Unsupported) => {
                self.invalidate();
                self.record_absence(attempted_at);
            }
            Ok(Table::Incomplete) | Err(_) => {
                self.samples.usage.record_failure(attempted_at);
                self.samples.power_mw.record_failure(attempted_at);
                self.samples.frequency.record_failure(attempted_at);
            }
        }
    }

    fn record(&mut self, fields: Fields, attempted_at: Duration) {
        match busy_percent(&fields.columns) {
            Busy::Valid(value) => self.samples.usage.record_value(value, attempted_at),
            Busy::Unavailable => self.samples.usage.record_absence(attempted_at),
            Busy::Invalid => self.samples.usage.record_failure(attempted_at),
        }
        record_scalar(&mut self.samples.power_mw, fields.power_mw, attempted_at);
        record_scalar(&mut self.samples.frequency, fields.clock_mhz, attempted_at);
    }

    fn record_absence(&mut self, attempted_at: Duration) {
        self.samples.usage.record_absence(attempted_at);
        self.samples.power_mw.record_absence(attempted_at);
        self.samples.frequency.record_absence(attempted_at);
    }
}

fn record_scalar(sample: &mut RetainedMetricSample<u16>, value: u16, attempted_at: Duration) {
    if value == UNAVAILABLE {
        sample.record_absence(attempted_at);
    } else {
        sample.record_value(value, attempted_at);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Fields {
    columns: [u16; IPU_COLUMNS],
    power_mw: u16,
    clock_mhz: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Table {
    Supported(Fields),
    /// Complete table with another layout.
    Unsupported,
    /// Fewer bytes than the header or the declared structure size.
    Incomplete,
}

enum Busy {
    Valid(i32),
    Unavailable,
    Invalid,
}

fn busy_percent(columns: &[u16; IPU_COLUMNS]) -> Busy {
    if columns.contains(&UNAVAILABLE) {
        return Busy::Unavailable;
    }
    if columns.iter().any(|column| *column > 100) {
        return Busy::Invalid;
    }
    let sum: u32 = columns.iter().map(|column| u32::from(*column)).sum();
    // Rounded mean of bounded columns, so the result stays within 0..=100.
    Busy::Valid(i32::try_from((sum + 4) / 8).unwrap_or(100))
}

// Bounds come from the bytes actually read and the declared size, never the sysfs file size.
fn parse(bytes: &[u8]) -> Table {
    let Some(declared) = read_u16(bytes, 0) else {
        return Table::Incomplete;
    };
    let declared = usize::from(declared);
    if bytes.len() < HEADER_SIZE || bytes.len() < declared {
        return Table::Incomplete;
    }
    if bytes[2] != FORMAT_REVISION || bytes[3] != CONTENT_REVISION || declared < MIN_STRUCTURE_SIZE
    {
        return Table::Unsupported;
    }
    let field = |offset| read_u16(bytes, offset).unwrap_or(UNAVAILABLE);
    let mut columns = [0; IPU_COLUMNS];
    for (index, column) in columns.iter_mut().enumerate() {
        *column = field(IPU_ACTIVITY_OFFSET + index * 2);
    }
    Table::Supported(Fields {
        columns,
        power_mw: field(IPU_POWER_OFFSET),
        clock_mhz: field(IPU_CLOCK_OFFSET),
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let pair = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([pair[0], pair[1]]))
}

/// Finds the `amdxdna` accel device and the `amdgpu` device whose table carries NPU fields, each by lowest PCI identity.
///
/// # Errors
/// Returns an I/O or invalid-data error for incomplete enumeration, malformed identity, or an unreadable or truncated table. `Ok(None)` means a complete scan found no qualifying pair.
pub fn detect_amd_npu(sys_root: &Path) -> io::Result<Option<AmdNpuSource>> {
    let accel_root = sys_root.join("class/accel");
    if !entry_exists(&accel_root)? {
        return Ok(None);
    }
    let mut accel_pci: Option<String> = None;
    for accel in numbered_entries(&accel_root, "accel")? {
        let device = fs::canonicalize(accel.join("device"))?;
        if driver_name(&device)?.as_deref() != Some("amdxdna") {
            continue;
        }
        let pci = pci_identity(&device)?;
        if accel_pci.as_ref().is_none_or(|old| pci < *old) {
            accel_pci = Some(pci);
        }
    }
    let Some(accel_pci) = accel_pci else {
        return Ok(None);
    };
    let mut selected: Option<AmdNpuSource> = None;
    for card in numbered_entries(&sys_root.join("class/drm"), "card")? {
        let device = fs::canonicalize(card.join("device"))?;
        if driver_name(&device)?.as_deref() != Some("amdgpu") {
            continue;
        }
        let Some(metrics_path) = optional_file(device.join("gpu_metrics"))? else {
            continue;
        };
        match parse(&fs::read(&metrics_path)?) {
            Table::Supported(_) => {}
            Table::Unsupported => continue,
            Table::Incomplete => return Err(invalid_data("truncated gpu_metrics table")),
        }
        let metrics_pci = pci_identity(&device)?;
        if selected
            .as_ref()
            .is_none_or(|old| metrics_pci < old.metrics_pci)
        {
            selected = Some(AmdNpuSource {
                accel_pci: accel_pci.clone(),
                metrics_pci,
                metrics_path,
            });
        }
    }
    Ok(selected)
}

fn driver_name(device: &Path) -> io::Result<Option<String>> {
    let driver = device.join("driver");
    if !entry_exists(&driver)? {
        return Ok(None);
    }
    Ok(fs::canonicalize(driver)?
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned))
}

#[cfg(test)]
mod tests;
