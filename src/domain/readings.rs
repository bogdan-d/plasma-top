//! Typed aggregate hardware and readings contracts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use crate::domain::boundary::ClockSnapshot;
use crate::domain::metric::{Capability, Metric};

/// Latest completed value for one independently captured metric.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricSample<T> {
    /// Captured value.
    pub value: T,
    /// Monotonic instant captured with the source read.
    pub captured_at: Duration,
}

impl<T> MetricSample<T> {
    /// Creates one completed metric sample.
    #[must_use]
    pub const fn new(value: T, captured_at: Duration) -> Self {
        Self { value, captured_at }
    }
}

/// Latest valid sample plus attempt and failure times.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetainedMetricSample<T> {
    /// Latest successful sample, retained across failed attempts.
    pub latest: Option<MetricSample<T>>,
    /// Monotonic instant of the most recent attempt, successful or not.
    pub attempted_at: Option<Duration>,
    /// Monotonic instant of the most recent failed attempt.
    pub failed_at: Option<Duration>,
    /// Whether the most recent attempt failed.
    pub latest_attempt_failed: bool,
}

impl<T> RetainedMetricSample<T> {
    /// Records one attempt, replacing the latest sample only when it succeeded.
    pub fn record(&mut self, value: Option<T>, captured_at: Duration) {
        if let Some(value) = value {
            self.record_value(value, captured_at);
        } else {
            self.record_failure(captured_at);
        }
    }

    /// Records a successful value.
    pub fn record_value(&mut self, value: T, captured_at: Duration) {
        self.attempted_at = Some(captured_at);
        self.latest = Some(MetricSample::new(value, captured_at));
        self.latest_attempt_failed = false;
    }

    /// Records a successful attempt that produced no comparable value.
    pub fn record_baseline(&mut self, attempted_at: Duration) {
        self.attempted_at = Some(attempted_at);
        self.latest_attempt_failed = false;
    }

    /// Records a failed attempt without replacing the latest valid sample.
    pub fn record_failure(&mut self, attempted_at: Duration) {
        self.attempted_at = Some(attempted_at);
        self.failed_at = Some(attempted_at);
        self.latest_attempt_failed = true;
    }

    /// Records a confirmed valid absence and clears the retained sample.
    pub fn record_absence(&mut self, attempted_at: Duration) {
        self.attempted_at = Some(attempted_at);
        self.latest = None;
        self.latest_attempt_failed = false;
    }

    /// Invalidates both the retained sample and attempt time.
    pub fn invalidate(&mut self) {
        self.latest = None;
        self.attempted_at = None;
        self.failed_at = None;
        self.latest_attempt_failed = false;
    }
}

impl<T> Default for RetainedMetricSample<T> {
    fn default() -> Self {
        Self {
            latest: None,
            attempted_at: None,
            failed_at: None,
            latest_attempt_failed: false,
        }
    }
}

/// System-battery charge state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BatteryState {
    /// No charging state is currently known.
    #[default]
    Unknown,
    /// The battery is charging.
    Charging,
    /// The battery is discharging.
    Discharging,
    /// The battery reports itself fully charged.
    FullyCharged,
}

impl BatteryState {
    /// Returns the stable lowercase token used by the Python formatter.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "",
            Self::Charging => "charging",
            Self::Discharging => "discharging",
            Self::FullyCharged => "fully-charged",
        }
    }
}

/// One system battery reading.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatterySystemReading {
    /// UPower object path or comparable stable battery identifier.
    pub id: String,
    /// Visible charge percentage.
    pub charge_percent: i32,
    /// Rounded power rate in watts.
    pub rate_watts: i32,
    /// Current charging state.
    pub state: BatteryState,
    /// Configured charge limit when the hardware exposes one.
    pub charge_limit_percent: Option<i32>,
}

/// One peripheral battery reading.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatteryPeripheralReading {
    /// Human-readable peripheral model name.
    pub name: String,
    /// Visible charge percentage.
    pub charge_percent: i32,
}

/// Rounded disk-usage reading for one mountpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DiskUsageReading {
    /// Visible usage percentage.
    pub percent: i32,
    /// Rounded used space in GiB.
    pub used_gib: u64,
    /// Rounded total space in GiB.
    pub total_gib: u64,
}

/// One load-average triple.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LoadAverage {
    /// One-minute load average.
    pub one: f64,
    /// Five-minute load average.
    pub five: f64,
    /// Fifteen-minute load average.
    pub fifteen: f64,
}

/// Panel top-process summary row.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TopProcessSummary {
    /// Process command name.
    pub command: String,
    /// Instantaneous CPU usage percentage normalized to one core.
    pub cpu_percent: i32,
}

/// Tooltip/process-page process row.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TopProcessDetails {
    /// Process id.
    pub pid: u32,
    /// Process command name or truncated cmdline.
    pub command: String,
    /// Instantaneous CPU usage percentage normalized to one core.
    pub cpu_percent: i32,
    /// Resident-memory percentage.
    pub memory_percent: f64,
}

/// SMART interface kind exposed by a disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiskSmartInterface {
    /// ATA SMART via UDisks2's `Drive.Ata` interface.
    Ata,
    /// NVMe SMART via UDisks2's `NVMe.Controller` interface.
    Nvme,
}

impl DiskSmartInterface {
    /// Returns the stable lowercase token used by the Python sensor layer.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ata => "ata",
            Self::Nvme => "nvme",
        }
    }
}

/// One discoverable SMART-capable disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartDisk {
    /// D-Bus object path for the drive.
    pub object_path: String,
    /// SMART interface family used to query health.
    pub interface: DiskSmartInterface,
    /// Whether the kernel reports the disk as rotational.
    pub rotational: bool,
}

/// Latest known hardware that can provide PlasmaTop metrics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareInventory {
    /// Capabilities discovered on the current host.
    pub capabilities: BTreeSet<Capability>,
    /// Metrics that collection can potentially populate on this host.
    pub metrics: BTreeSet<Metric>,
    /// Supported CPU temperature sensor path.
    pub cpu_temp_path: Option<PathBuf>,
    /// `cpu0` cpufreq fast-path sysfs file.
    pub cpu_freq_path: Option<PathBuf>,
    /// Active turbo/boost control file.
    pub cpu_turbo_path: Option<PathBuf>,
    /// Disk temperature sensor paths keyed by disk label.
    pub hd_temp_paths: BTreeMap<String, PathBuf>,
    /// Fan speed sensor paths keyed by configured fan label.
    pub fan_paths: BTreeMap<String, PathBuf>,
    /// Stable ids for system batteries.
    pub battery_sys_ids: Vec<String>,
    /// Whether an NVIDIA GPU is present.
    pub has_nvidia: bool,
    /// Intel GPU active-frequency sysfs file.
    pub intel_gpu_freq_path: Option<PathBuf>,
    /// PCI address used to attribute Intel DRM fdinfo counters.
    pub intel_gpu_pci: Option<String>,
    /// Current default-route network device when known.
    pub net_device: Option<String>,
    /// Whole-disk device label used for disk I/O rates.
    pub disk_io_device: Option<String>,
    /// Logical CPU count used for load/process normalization.
    pub cpu_count: usize,
    /// Whether a turbo/boost control exists in sysfs.
    pub cpu_turbo_supported: bool,
    /// Whether a readable backlight device exists.
    pub has_backlight: bool,
    /// Whether any wireless interface exists.
    pub has_wifi: bool,
    /// Retried UPower peripheral id for the mouse battery.
    pub battery_mouse_id: Option<String>,
    /// Retried UPower peripheral id for the keyboard battery.
    pub battery_kbd_id: Option<String>,
    /// SMART-capable disks keyed by disk label.
    pub disk_smart_drives: BTreeMap<String, SmartDisk>,
}

impl Default for HardwareInventory {
    fn default() -> Self {
        Self {
            capabilities: BTreeSet::new(),
            metrics: BTreeSet::new(),
            cpu_temp_path: None,
            cpu_freq_path: None,
            cpu_turbo_path: None,
            hd_temp_paths: BTreeMap::new(),
            fan_paths: BTreeMap::new(),
            battery_sys_ids: Vec::new(),
            has_nvidia: false,
            intel_gpu_freq_path: None,
            intel_gpu_pci: None,
            net_device: None,
            disk_io_device: None,
            cpu_count: 1,
            cpu_turbo_supported: false,
            has_backlight: false,
            has_wifi: false,
            battery_mouse_id: None,
            battery_kbd_id: None,
            disk_smart_drives: BTreeMap::new(),
        }
    }
}

/// Latest completed metric values assembled for one display publication.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DisplaySnapshot {
    /// Display assembly timestamp.
    pub assembled_at: ClockSnapshot,
    /// Metrics populated in this sample.
    pub metrics: BTreeSet<Metric>,
    /// Aggregate CPU usage percentage.
    pub cpu_usage: Option<i32>,
    /// CPU temperature in °C.
    pub cpu_temp: Option<i32>,
    /// CPU frequency in MHz.
    pub cpu_freq_mhz: Option<f64>,
    /// Whether turbo/boost is enabled.
    pub cpu_turbo: Option<bool>,
    /// Shared CPU history used by traces and graphs.
    pub cpu_history: Vec<i32>,
    /// Shared memory history used by traces and graphs.
    pub mem_history: Vec<i32>,
    /// System uptime in seconds.
    pub uptime_seconds: Option<i64>,
    /// 1/5/15-minute load averages.
    pub load_average: Option<LoadAverage>,
    /// Panel top-process summary rows.
    pub top_process: Option<Vec<TopProcessSummary>>,
    /// Tooltip/process-page rows.
    pub top_process_full: Option<Vec<TopProcessDetails>>,
    /// Current per-core CPU percentages.
    pub cpu_core_usage: Option<Vec<i32>>,
    /// Per-core CPU histories.
    pub cpu_core_history: Option<Vec<Vec<i32>>>,
    /// Memory usage percentage.
    pub mem_usage: Option<i32>,
    /// Used memory in GiB.
    pub mem_used_gib: Option<u64>,
    /// Total memory in GiB.
    pub mem_total_gib: Option<u64>,
    /// Swap usage percentage.
    pub swap_usage: Option<i32>,
    /// Upload rate in bytes per second.
    pub net_up_bps: Option<u64>,
    /// Download rate in bytes per second.
    pub net_down_bps: Option<u64>,
    /// Active network device name.
    pub net_device: Option<String>,
    /// Active route IP address.
    pub ip_address: Option<String>,
    /// Active wifi SSID.
    pub wifi_ssid: Option<String>,
    /// Active wifi signal quality percentage.
    pub wifi_signal_percent: Option<i32>,
    /// Disk read rate in bytes per second.
    pub disk_read_bps: Option<u64>,
    /// Disk write rate in bytes per second.
    pub disk_write_bps: Option<u64>,
    /// Mountpoint disk-usage readings.
    pub disk_usage: BTreeMap<String, Option<DiskUsageReading>>,
    /// SMART health keyed by disk label.
    pub disk_smart: BTreeMap<String, Option<bool>>,
    /// Disk temperatures keyed by disk label.
    pub hd_temps: BTreeMap<String, Option<i32>>,
    /// Fan speeds keyed by fan label.
    pub fan_speeds: BTreeMap<String, Option<i32>>,
    /// System battery readings.
    pub battery_sys: Vec<BatterySystemReading>,
    /// Mouse battery reading when present.
    pub battery_mouse: Option<BatteryPeripheralReading>,
    /// Keyboard battery reading when present.
    pub battery_kbd: Option<BatteryPeripheralReading>,
    /// NVIDIA GPU temperature in °C.
    pub gpu_temp: Option<i32>,
    /// NVIDIA GPU usage percentage.
    pub gpu_usage: Option<i32>,
    /// NVIDIA GPU memory usage percentage.
    pub gpu_mem: Option<i32>,
    /// NVIDIA decoder usage percentage.
    pub gpu_dec: Option<i32>,
    /// NVIDIA GPU fan percentage.
    pub gpu_fan: Option<i32>,
    /// Intel GPU active frequency in MHz.
    pub gpu_intel_freq: Option<i32>,
    /// Intel GPU render usage percentage.
    pub gpu_intel_usage: Option<i32>,
    /// Intel GPU video/decode usage percentage.
    pub gpu_intel_dec_usage: Option<i32>,
    /// Active GPU usage history for the graphs page.
    pub gpu_usage_history: Vec<i32>,
    /// Active GPU decoder history for the graphs page.
    pub gpu_dec_history: Vec<i32>,
    /// Network upload history for the graphs page.
    pub net_up_history: Vec<u64>,
    /// Network download history for the graphs page.
    pub net_down_history: Vec<u64>,
    /// Screen brightness percentage.
    pub screen_brightness: Option<i32>,
    /// Pending package-update count.
    pub system_updates: Option<i32>,
    /// External server health flag.
    pub server_ok: Option<bool>,
}

#[cfg(test)]
mod tests;
