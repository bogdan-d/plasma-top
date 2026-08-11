//! Disk mounts, usage, hwmon, identity, and byte-rate readings.
//!
//! Mountpoint
//! selection, disk-usage reads, root-disk device resolution, disk byte-rate
//! diffs, hwmon-backed disk temperature and fan discovery, and a deterministic
//! disk-identity view for later SMART work. All host I/O goes through explicit
//! proc/sys roots or a concrete mount path so tests can stay fixture-driven.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nix::sys::statvfs::statvfs;

use crate::config::{Config, Mounts, SensorOverrides};
use crate::domain::boundary::ClockSnapshot;
use crate::domain::metric::Capability;
use crate::domain::readings::{HardwareInventory, RetainedMetricSample, SmartDisk};

use super::hwmon::{
    hwmon_dirs_matching, read_path_int, read_path_millidegrees_celsius, resolve_sensor_spec,
};

pub(super) const HD_TEMP_CACHE_TTL: Duration = Duration::from_secs(30);
pub(super) const FAN_SPEED_CACHE_TTL: Duration = Duration::from_secs(30);
const DISKSTAT_SECTOR_BYTES: u64 = 512;
const BYTES_PER_GIB: u64 = 1024 * 1024 * 1024;
const DISK_TEMPERATURE_CHIPS: [&str; 2] = ["nvme", "drivetemp"];

/// Stable disk identity class used by SMART collection and formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiskKind {
    /// NVMe namespace-backed disk.
    Nvme,
    /// ATA/SATA/SCSI-style block disk.
    Ata,
}

/// One discovered whole-disk identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DiskIdentity {
    /// Kernel block-device label, e.g. `nvme0n1` or `sda`.
    pub label: String,
    /// Device family used by later SMART logic.
    pub kind: DiskKind,
    /// Whether the kernel reports a rotational queue.
    pub rotational: bool,
}

/// Rounded disk-usage reading for one mountpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskUsage {
    /// Visible percent, matching `int(psutil.disk_usage(...).percent)`.
    pub percent: i32,
    /// Rounded used space in GiB.
    pub used_gb: u64,
    /// Rounded total space in GiB.
    pub total_gb: u64,
}

/// Mutable disk sample and diff state that persists between sampling passes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiskState {
    pub(super) io: RetainedMetricSample<(u64, u64)>,
    pub(super) usage: BTreeMap<String, RetainedMetricSample<DiskUsage>>,
    pub(super) hd_temp_cache: BTreeMap<String, RetainedMetricSample<i32>>,
    pub(super) fan_speed_cache: BTreeMap<String, RetainedMetricSample<i32>>,
    pub(super) hd_temp_sources: BTreeMap<String, PathBuf>,
    pub(super) fan_speed_sources: BTreeMap<String, PathBuf>,
    pub(super) smart_cache: BTreeMap<String, RetainedMetricSample<bool>>,
    pub(super) smart_sources: BTreeMap<String, SmartDisk>,
    pub(super) rate_device: Option<String>,
    prev_read_bytes: u64,
    prev_write_bytes: u64,
    rate_sample_at: Option<Duration>,
}

impl DiskState {
    pub(crate) fn reset_io(&mut self) {
        self.io.invalidate();
        self.rate_device = None;
        self.prev_read_bytes = 0;
        self.prev_write_bytes = 0;
        self.rate_sample_at = None;
    }

    pub(crate) fn invalidate_io_baseline(&mut self) {
        self.prev_read_bytes = 0;
        self.prev_write_bytes = 0;
        self.rate_sample_at = None;
    }

    #[cfg(test)]
    pub(crate) fn reconcile_mounts(&mut self, mounts: &[String]) {
        self.usage
            .retain(|mount, _| mounts.iter().any(|current| current == mount));
    }

    pub(crate) fn reconcile_sources(
        &mut self,
        hw: &HardwareInventory,
        cfg: &Config,
        capabilities: &BTreeSet<Capability>,
    ) {
        if !capabilities.contains(&Capability::DiskIo)
            || hw.disk_io_device.is_none()
            || self.rate_device.as_deref() != hw.disk_io_device.as_deref()
        {
            self.reset_io();
        }
        if !capabilities.contains(&Capability::DiskUsage) {
            self.usage.clear();
        }

        reconcile_path_samples(
            &mut self.hd_temp_cache,
            &mut self.hd_temp_sources,
            capabilities.contains(&Capability::DiskTemperature),
            &hw.hd_temp_paths,
        );
        reconcile_path_samples(
            &mut self.fan_speed_cache,
            &mut self.fan_speed_sources,
            capabilities.contains(&Capability::FanSpeed),
            &hw.fan_paths,
        );

        if capabilities.contains(&Capability::DiskSmart) && cfg.disks.smart {
            self.smart_cache
                .retain(|label, _| hw.disk_smart_drives.contains_key(label));
            self.smart_sources
                .retain(|label, _| hw.disk_smart_drives.contains_key(label));
            for (label, drive) in &hw.disk_smart_drives {
                if self.smart_sources.get(label) != Some(drive) {
                    self.smart_cache.remove(label);
                    self.smart_sources.insert(label.clone(), drive.clone());
                }
            }
        } else {
            self.smart_cache.clear();
            self.smart_sources.clear();
        }
    }
}

fn reconcile_path_samples(
    samples: &mut BTreeMap<String, RetainedMetricSample<i32>>,
    sources: &mut BTreeMap<String, PathBuf>,
    demanded: bool,
    current: &BTreeMap<String, PathBuf>,
) {
    if !demanded {
        samples.clear();
        sources.clear();
        return;
    }
    samples.retain(|label, _| current.contains_key(label));
    sources.retain(|label, _| current.contains_key(label));
    for (label, path) in current {
        if sources.get(label) != Some(path) {
            samples.remove(label);
            sources.insert(label.clone(), path.clone());
        }
    }
}

/// Resolves disk temperature hwmon paths, honoring manual overrides first.
#[must_use]
pub fn find_hd_temp_paths(
    sys_root: &Path,
    overrides: &SensorOverrides,
) -> BTreeMap<String, PathBuf> {
    let mut result = BTreeMap::new();
    for spec in [
        overrides.hd1_temp.as_deref(),
        overrides.hd2_temp.as_deref(),
        overrides.hd3_temp.as_deref(),
        overrides.hd4_temp.as_deref(),
    ] {
        let Some(spec) = spec else {
            continue;
        };
        let Some(path) = resolve_sensor_spec(sys_root, spec) else {
            continue;
        };
        let Some(hwmon) = path.parent() else {
            continue;
        };
        result.insert(hwmon_device_label(sys_root, hwmon), path);
    }
    if !result.is_empty() {
        return result;
    }

    for chip in DISK_TEMPERATURE_CHIPS {
        for hwmon in hwmon_dirs_matching(sys_root, chip) {
            let path = hwmon.join("temp1_input");
            if path.exists() {
                result.insert(hwmon_device_label(sys_root, &hwmon), path);
            }
        }
    }
    result
}

/// Resolves manual fan-speed hwmon paths.
#[must_use]
pub fn find_fan_speed_paths(
    sys_root: &Path,
    overrides: &SensorOverrides,
) -> BTreeMap<String, PathBuf> {
    let mut result = BTreeMap::new();
    for (index, spec) in [
        (1_u8, overrides.fan1_speed.as_deref()),
        (2_u8, overrides.fan2_speed.as_deref()),
        (3_u8, overrides.fan3_speed.as_deref()),
        (4_u8, overrides.fan4_speed.as_deref()),
    ] {
        let Some(spec) = spec else {
            break;
        };
        let Some(path) = resolve_sensor_spec(sys_root, spec) else {
            continue;
        };
        result.insert(index.to_string(), path);
    }
    result
}

/// Performs one disk-temperature source read.
#[must_use]
pub fn read_hd_temp(path: &Path) -> Option<i32> {
    read_path_millidegrees_celsius(Some(path))
}

/// Performs one fan-speed source read.
#[must_use]
pub fn read_fan_speed(path: &Path) -> Option<i32> {
    read_path_int(Some(path))
}

/// Resolves the configured mountpoint list.
///
/// Explicit lists are used as-is. `Mounts::Auto` returns `/` first plus every
/// real mount currently present under `disks.auto_roots`, sorted
/// alphabetically.
#[must_use]
pub fn resolve_mounts(proc_root: &Path, cfg: &Config) -> Vec<String> {
    try_resolve_mounts(proc_root, cfg).unwrap_or_else(|_| match &cfg.disks.mounts {
        Mounts::Explicit(mounts) => mounts.clone(),
        Mounts::Auto => vec![String::from("/")],
    })
}

/// Resolves the configured mountpoint list while preserving automatic enumeration failures.
///
/// Explicit lists never inspect procfs and are therefore deterministic. Automatic enumeration
/// returns an error when the mounts file is unavailable, unreadable, or malformed, allowing a
/// long-lived caller to retain its last confirmed inventory.
///
/// # Errors
///
/// Returns an I/O error when automatic procfs enumeration cannot be confirmed.
pub fn try_resolve_mounts(proc_root: &Path, cfg: &Config) -> io::Result<Vec<String>> {
    match &cfg.disks.mounts {
        Mounts::Explicit(mounts) => Ok(mounts.clone()),
        Mounts::Auto => {
            let roots: Vec<String> = cfg
                .disks
                .auto_roots
                .iter()
                .map(|root| format!("{}/", root.trim_end_matches('/')))
                .collect();
            let mut found = BTreeSet::new();
            let Some(path) = mounts_path_outcome(proc_root)? else {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "mount enumeration is unavailable",
                ));
            };
            let text = fs::read_to_string(path)?;
            for mount in parse_mounts_outcome(&text)? {
                if mount.mountpoint != "/"
                    && roots.iter().any(|root| mount.mountpoint.starts_with(root))
                {
                    found.insert(mount.mountpoint);
                }
            }
            let mut ordered = vec![String::from("/")];
            ordered.extend(found);
            Ok(ordered)
        }
    }
}

/// Resolves one mountpoint to the whole-disk device used for byte-rate reads.
#[must_use]
pub fn detect_disk_io_device(proc_root: &Path, sys_root: &Path, mount: &str) -> Option<String> {
    detect_disk_io_device_outcome(proc_root, sys_root, mount)
        .ok()
        .flatten()
}

/// Resolves disk-I/O identity while preserving procfs/sysfs boundary failures.
pub(crate) fn detect_disk_io_device_outcome(
    proc_root: &Path,
    sys_root: &Path,
    mount: &str,
) -> io::Result<Option<String>> {
    let Some(path) = mounts_path_outcome(proc_root)? else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "mount enumeration is unavailable",
        ));
    };
    let text = fs::read_to_string(path)?;
    let source = parse_mounts_outcome(&text)?
        .into_iter()
        .find(|entry| entry.mountpoint == mount)
        .map(|entry| entry.source);
    source
        .map(|source| {
            let allow_missing = source.starts_with("/dev/mapper/");
            whole_disk_of_outcome(sys_root, &device_basename(&source), allow_missing)
        })
        .transpose()
}

/// Discovers supported whole-disk identities from sysfs.
#[must_use]
pub fn detect_disks(sys_root: &Path) -> Vec<DiskIdentity> {
    let root = sys_root.join("block");
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };

    let mut disks = Vec::new();
    for entry in entries.flatten() {
        let label = entry.file_name().to_string_lossy().into_owned();
        if label.starts_with("sr") {
            continue;
        }
        let Some(kind) = disk_kind_for_label(&label) else {
            continue;
        };
        disks.push(DiskIdentity {
            rotational: is_rotational(sys_root, &label),
            label,
            kind,
        });
    }
    disks.sort();
    disks
}

/// Reads disk usage for one mountpoint via `statvfs(2)` semantics.
#[must_use]
pub fn read_disk_usage(mount: &Path) -> Option<DiskUsage> {
    let stats = statvfs(mount).ok()?;
    let fragment_size = stats.fragment_size();
    let total_bytes = stats.blocks().saturating_mul(fragment_size);
    let free_bytes = stats.blocks_free().saturating_mul(fragment_size);
    let available_bytes = stats.blocks_available().saturating_mul(fragment_size);
    Some(disk_usage_from_bytes(
        total_bytes,
        free_bytes,
        available_bytes,
    ))
}

/// Reads whole-disk read/write byte rates from `/proc/diskstats`.
///
/// Returns `(None, None)` on the first sample, on device changes, on counter
/// rollback, or when the elapsed monotonic time is zero.
#[must_use]
pub fn read_disk_io(
    proc_root: &Path,
    state: &mut DiskState,
    device: &str,
    clock: ClockSnapshot,
) -> (Option<u64>, Option<u64>) {
    match read_disk_io_once(proc_root, state, device, clock) {
        DiskIoReadOutcome::Value(read, write) => (Some(read), Some(write)),
        DiskIoReadOutcome::Baseline | DiskIoReadOutcome::NoDelta | DiskIoReadOutcome::Failed => {
            (None, None)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiskIoReadOutcome {
    Value(u64, u64),
    Baseline,
    NoDelta,
    Failed,
}

/// Performs one disk-counter attempt and distinguishes a new baseline from an invalid same-source delta.
pub(crate) fn read_disk_io_once(
    proc_root: &Path,
    state: &mut DiskState,
    device: &str,
    clock: ClockSnapshot,
) -> DiskIoReadOutcome {
    let Some((read_bytes, write_bytes)) = read_disk_bytes(proc_root, device) else {
        return DiskIoReadOutcome::Failed;
    };

    let same_device = state.rate_device.as_deref() == Some(device);
    if !same_device {
        state.rate_device = Some(device.to_owned());
        state.prev_read_bytes = read_bytes;
        state.prev_write_bytes = write_bytes;
        state.rate_sample_at = Some(clock.monotonic);
        return DiskIoReadOutcome::Baseline;
    }
    let Some(previous_sample_at) = state.rate_sample_at else {
        state.prev_read_bytes = read_bytes;
        state.prev_write_bytes = write_bytes;
        state.rate_sample_at = Some(clock.monotonic);
        return DiskIoReadOutcome::Baseline;
    };
    if read_bytes < state.prev_read_bytes || write_bytes < state.prev_write_bytes {
        state.prev_read_bytes = read_bytes;
        state.prev_write_bytes = write_bytes;
        state.rate_sample_at = Some(clock.monotonic);
        return DiskIoReadOutcome::Baseline;
    }
    let Some(elapsed) = clock.monotonic.checked_sub(previous_sample_at) else {
        return DiskIoReadOutcome::NoDelta;
    };
    let elapsed_nanos = elapsed.as_nanos();
    if elapsed_nanos == 0 {
        return DiskIoReadOutcome::NoDelta;
    }

    let read_bps = rate_per_second(read_bytes - state.prev_read_bytes, elapsed_nanos);
    let write_bps = rate_per_second(write_bytes - state.prev_write_bytes, elapsed_nanos);
    state.prev_read_bytes = read_bytes;
    state.prev_write_bytes = write_bytes;
    state.rate_sample_at = Some(clock.monotonic);
    DiskIoReadOutcome::Value(read_bps, write_bps)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MountEntry {
    source: String,
    mountpoint: String,
}

fn parse_mounts_outcome(text: &str) -> io::Result<Vec<MountEntry>> {
    let mut mounts = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(source) = fields.next() else {
            continue;
        };
        let Some(mountpoint) = fields.next() else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "malformed mount entry",
            ));
        };
        if fields.count() != 4 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "malformed mount entry",
            ));
        }
        mounts.push(MountEntry {
            source: source.to_owned(),
            mountpoint: decode_mount_field(mountpoint),
        });
    }
    Ok(mounts)
}

fn decode_mount_field(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let octal = &value[index + 1..index + 4];
            if octal.as_bytes().iter().all(u8::is_ascii_digit) {
                if let Ok(code) = u8::from_str_radix(octal, 8) {
                    decoded.push(char::from(code));
                    index += 4;
                    continue;
                }
            }
        }
        decoded.push(char::from(bytes[index]));
        index += 1;
    }
    decoded
}

fn mounts_path_outcome(proc_root: &Path) -> io::Result<Option<PathBuf>> {
    let mounts = proc_root.join("mounts");
    if mounts.try_exists()? {
        return Ok(Some(mounts));
    }
    let self_mounts = proc_root.join("self/mounts");
    Ok(self_mounts.try_exists()?.then_some(self_mounts))
}

fn device_basename(source: &str) -> String {
    Path::new(source)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_owned())
}

fn whole_disk_of_outcome(sys_root: &Path, device: &str, allow_missing: bool) -> io::Result<String> {
    let node = sys_root.join("class/block").join(device);
    if !node.try_exists()? {
        return if allow_missing {
            Ok(device.to_owned())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "block device is unavailable in sysfs",
            ))
        };
    }
    if !node.join("partition").try_exists()? {
        return Ok(device.to_owned());
    }
    let real = fs::canonicalize(node)?;
    real.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid block parent"))
}

fn disk_kind_for_label(label: &str) -> Option<DiskKind> {
    if label.starts_with("nvme") {
        Some(DiskKind::Nvme)
    } else if label.starts_with("sd") || label.starts_with("hd") {
        Some(DiskKind::Ata)
    } else {
        None
    }
}

pub(crate) fn is_rotational(sys_root: &Path, label: &str) -> bool {
    is_rotational_outcome(sys_root, label).unwrap_or(false)
}

pub(crate) fn is_rotational_outcome(sys_root: &Path, label: &str) -> io::Result<bool> {
    match fs::read_to_string(sys_root.join("block").join(label).join("queue/rotational"))?.trim() {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "malformed rotational flag",
        )),
    }
}

fn hwmon_device_label(sys_root: &Path, hwmon: &Path) -> String {
    let real = fs::canonicalize(hwmon).unwrap_or_else(|_| hwmon.to_path_buf());
    let parts: Vec<String> = real
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();

    for part in parts.iter().rev() {
        if part.starts_with("nvme") {
            return resolve_nvme_namespace(sys_root, part);
        }
        if part.starts_with("sd") || part.starts_with("hd") {
            return part.clone();
        }
    }

    if let Some(scsi_address) = parts.iter().find(|part| is_scsi_address(part)) {
        let block_root = sys_root.join("class/block");
        if let Ok(entries) = fs::read_dir(block_root) {
            let mut block_paths: Vec<PathBuf> =
                entries.flatten().map(|entry| entry.path()).collect();
            block_paths.sort();
            for block in block_paths {
                if let Ok(real_block) = fs::canonicalize(&block) {
                    let components: Vec<String> = real_block
                        .components()
                        .map(|component| component.as_os_str().to_string_lossy().into_owned())
                        .collect();
                    if components.iter().any(|part| part == scsi_address) {
                        if let Some(name) = block.file_name() {
                            return name.to_string_lossy().into_owned();
                        }
                    }
                }
            }
        }
    }

    hwmon
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| String::from("hwmon"))
}

fn resolve_nvme_namespace(sys_root: &Path, controller: &str) -> String {
    let path = sys_root.join("class/nvme").join(controller);
    let Ok(entries) = fs::read_dir(path) else {
        return controller.to_owned();
    };
    let prefix = format!("{controller}n");
    let mut namespaces: Vec<String> = entries
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.starts_with(&prefix))
        .collect();
    namespaces.sort();
    namespaces
        .into_iter()
        .next()
        .unwrap_or_else(|| controller.to_owned())
}

fn is_scsi_address(part: &str) -> bool {
    let mut fields = part.split(':');
    let Some(a) = fields.next() else {
        return false;
    };
    let Some(b) = fields.next() else {
        return false;
    };
    let Some(c) = fields.next() else {
        return false;
    };
    let Some(d) = fields.next() else {
        return false;
    };
    fields.next().is_none()
        && !a.is_empty()
        && !b.is_empty()
        && !c.is_empty()
        && !d.is_empty()
        && [a, b, c, d]
            .into_iter()
            .all(|field| field.chars().all(|ch| ch.is_ascii_digit()))
}

fn read_disk_bytes(proc_root: &Path, device: &str) -> Option<(u64, u64)> {
    let text = fs::read_to_string(proc_root.join("diskstats")).ok()?;
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() <= 9 || fields[2] != device {
            continue;
        }
        let sectors_read = fields[5].parse::<u64>().ok()?;
        let sectors_written = fields[9].parse::<u64>().ok()?;
        return Some((
            sectors_read.saturating_mul(DISKSTAT_SECTOR_BYTES),
            sectors_written.saturating_mul(DISKSTAT_SECTOR_BYTES),
        ));
    }
    None
}

fn rate_per_second(delta: u64, elapsed_nanos: u128) -> u64 {
    let scaled = u128::from(delta).saturating_mul(1_000_000_000) / elapsed_nanos;
    u64::try_from(scaled).unwrap_or(u64::MAX)
}

fn disk_usage_from_bytes(total_bytes: u64, free_bytes: u64, available_bytes: u64) -> DiskUsage {
    let used_bytes = total_bytes.saturating_sub(free_bytes);
    let visible_total = used_bytes.saturating_add(available_bytes);
    let percent = if visible_total == 0 {
        0
    } else {
        ((u128::from(used_bytes).saturating_mul(1000)) / u128::from(visible_total) / 10) as i32
    };
    DiskUsage {
        percent,
        used_gb: round_half_even_div(used_bytes, BYTES_PER_GIB),
        total_gb: round_half_even_div(total_bytes, BYTES_PER_GIB),
    }
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
    } else if doubled < denominator || quotient.is_multiple_of(2) {
        quotient
    } else {
        quotient.saturating_add(1)
    }
}

#[cfg(test)]
mod tests;
