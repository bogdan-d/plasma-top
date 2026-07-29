//! Disk mounts, usage, hwmon, identity, and byte-rate readings.
//!
//! Mountpoint
//! selection, disk-usage reads, root-disk device resolution, disk byte-rate
//! diffs, hwmon-backed disk temperature and fan discovery, and a deterministic
//! disk-identity view for later SMART work. All host I/O goes through explicit
//! proc/sys roots or a concrete mount path so tests can stay fixture-driven.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nix::sys::statvfs::statvfs;

use crate::config::{Config, Mounts, SensorOverrides};
use crate::domain::boundary::ClockSnapshot;

use super::hwmon::{
    hwmon_dirs_matching, read_path_int, read_path_millidegrees_celsius, resolve_sensor_spec,
};

const HD_TEMP_CACHE_TTL: Duration = Duration::from_secs(30);
const FAN_SPEED_CACHE_TTL: Duration = Duration::from_secs(30);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CachedReading<T> {
    value: Option<T>,
    sampled_at: Duration,
}

/// Mutable disk cache/diff state that persists between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiskState {
    hd_temp_cache: BTreeMap<String, CachedReading<i32>>,
    fan_speed_cache: BTreeMap<String, CachedReading<i32>>,
    rate_device: Option<String>,
    prev_read_bytes: u64,
    prev_write_bytes: u64,
    rate_sample_at: Option<Duration>,
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

/// Reads a cached disk temperature, refreshing every 30 seconds.
#[must_use]
pub fn read_hd_temp_cached(
    state: &mut DiskState,
    clock: ClockSnapshot,
    label: &str,
    path: &Path,
) -> Option<i32> {
    cached_by_label(
        &mut state.hd_temp_cache,
        label,
        clock.monotonic,
        HD_TEMP_CACHE_TTL,
        || read_path_millidegrees_celsius(Some(path)),
    )
}

/// Reads a cached fan speed, refreshing every 30 seconds.
#[must_use]
pub fn read_fan_speed_cached(
    state: &mut DiskState,
    clock: ClockSnapshot,
    label: &str,
    path: &Path,
) -> Option<i32> {
    cached_by_label(
        &mut state.fan_speed_cache,
        label,
        clock.monotonic,
        FAN_SPEED_CACHE_TTL,
        || read_path_int(Some(path)),
    )
}

/// Resolves the configured mountpoint list.
///
/// Explicit lists are used as-is. `Mounts::Auto` returns `/` first plus every
/// real mount currently present under `disks.auto_roots`, sorted
/// alphabetically.
#[must_use]
pub fn resolve_mounts(proc_root: &Path, cfg: &Config) -> Vec<String> {
    match &cfg.disks.mounts {
        Mounts::Explicit(mounts) => mounts.clone(),
        Mounts::Auto => {
            let roots: Vec<String> = cfg
                .disks
                .auto_roots
                .iter()
                .map(|root| format!("{}/", root.trim_end_matches('/')))
                .collect();
            let mut found = BTreeSet::new();
            if let Some(mounts) = load_mounts(proc_root) {
                for mount in mounts {
                    if mount.mountpoint != "/"
                        && roots.iter().any(|root| mount.mountpoint.starts_with(root))
                    {
                        found.insert(mount.mountpoint);
                    }
                }
            }
            let mut ordered = vec![String::from("/")];
            ordered.extend(found);
            ordered
        }
    }
}

/// Resolves one mountpoint to the whole-disk device used for byte-rate reads.
#[must_use]
pub fn detect_disk_io_device(proc_root: &Path, sys_root: &Path, mount: &str) -> Option<String> {
    let device = resolve_mount_device(proc_root, mount)?;
    Some(whole_disk_of(sys_root, &device))
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
    let Some((read_bytes, write_bytes)) = read_disk_bytes(proc_root, device) else {
        return (None, None);
    };

    let same_device = state.rate_device.as_deref() == Some(device);
    let previous_read = state.prev_read_bytes;
    let previous_write = state.prev_write_bytes;
    let previous_sample_at = state.rate_sample_at;

    state.rate_device = Some(device.to_owned());
    state.prev_read_bytes = read_bytes;
    state.prev_write_bytes = write_bytes;
    state.rate_sample_at = Some(clock.monotonic);

    if !same_device {
        return (None, None);
    }
    let Some(previous_sample_at) = previous_sample_at else {
        return (None, None);
    };
    if read_bytes < previous_read || write_bytes < previous_write {
        return (None, None);
    }

    let elapsed = clock.monotonic.saturating_sub(previous_sample_at);
    let elapsed_nanos = elapsed.as_nanos();
    if elapsed_nanos == 0 {
        return (None, None);
    }

    let read_bps = rate_per_second(read_bytes - previous_read, elapsed_nanos);
    let write_bps = rate_per_second(write_bytes - previous_write, elapsed_nanos);
    (Some(read_bps), Some(write_bps))
}

fn cached_by_label<T: Copy>(
    cache: &mut BTreeMap<String, CachedReading<T>>,
    label: &str,
    now: Duration,
    ttl: Duration,
    read_fn: impl FnOnce() -> Option<T>,
) -> Option<T> {
    if let Some(cached) = cache.get(label)
        && now.saturating_sub(cached.sampled_at) < ttl
    {
        return cached.value;
    }
    let value = read_fn();
    cache.insert(
        label.to_owned(),
        CachedReading {
            value,
            sampled_at: now,
        },
    );
    value
}

fn load_mounts(proc_root: &Path) -> Option<Vec<MountEntry>> {
    let text = fs::read_to_string(mounts_path(proc_root)?).ok()?;
    Some(parse_mounts(&text))
}

fn mounts_path(proc_root: &Path) -> Option<PathBuf> {
    let mounts = proc_root.join("mounts");
    if mounts.exists() {
        return Some(mounts);
    }
    let self_mounts = proc_root.join("self/mounts");
    self_mounts.exists().then_some(self_mounts)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MountEntry {
    source: String,
    mountpoint: String,
}

fn parse_mounts(text: &str) -> Vec<MountEntry> {
    let mut mounts = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(source) = fields.next() else {
            continue;
        };
        let Some(mountpoint) = fields.next() else {
            continue;
        };
        mounts.push(MountEntry {
            source: source.to_owned(),
            mountpoint: decode_mount_field(mountpoint),
        });
    }
    mounts
}

fn decode_mount_field(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = String::with_capacity(value.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' && index + 3 < bytes.len() {
            let octal = &value[index + 1..index + 4];
            if octal.as_bytes().iter().all(u8::is_ascii_digit)
                && let Ok(code) = u8::from_str_radix(octal, 8)
            {
                decoded.push(char::from(code));
                index += 4;
                continue;
            }
        }
        decoded.push(char::from(bytes[index]));
        index += 1;
    }
    decoded
}

fn resolve_mount_device(proc_root: &Path, mount: &str) -> Option<String> {
    load_mounts(proc_root)?
        .into_iter()
        .find(|entry| entry.mountpoint == mount)
        .map(|entry| device_basename(&entry.source))
}

fn device_basename(source: &str) -> String {
    Path::new(source)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| source.to_owned())
}

fn whole_disk_of(sys_root: &Path, device: &str) -> String {
    let node = sys_root.join("class/block").join(device);
    if !node.join("partition").exists() {
        return device.to_owned();
    }
    match fs::canonicalize(node) {
        Ok(real) => real
            .parent()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| device.to_owned()),
        Err(_) => device.to_owned(),
    }
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
    fs::read_to_string(sys_root.join("block").join(label).join("queue/rotational"))
        .ok()
        .is_some_and(|value| value.trim() == "1")
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
                    if components.iter().any(|part| part == scsi_address)
                        && let Some(name) = block.file_name()
                    {
                        return name.to_string_lossy().into_owned();
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
    } else if doubled < denominator || quotient % 2 == 0 {
        quotient
    } else {
        quotient.saturating_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::os::unix::fs::symlink;
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
            let root = std::env::temp_dir()
                .join(format!("plasma-top-disk-{}-{unique}", std::process::id()));
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

        fn mkdir(&self, relative: &str) {
            let path = self.root.join(relative);
            if let Err(error) = fs::create_dir_all(&path) {
                panic!("failed to create {}: {error}", path.display());
            }
        }

        fn symlink_dir(&self, target_relative: &str, link_relative: &str) {
            let target = self.root.join(target_relative);
            let link = self.root.join(link_relative);
            if let Some(parent) = link.parent()
                && let Err(error) = fs::create_dir_all(parent)
            {
                panic!("failed to create {}: {error}", parent.display());
            }
            if let Err(error) = symlink(&target, &link) {
                panic!(
                    "failed to symlink {} -> {}: {error}",
                    link.display(),
                    target.display()
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
    fn resolve_mounts_explicit_list_used_as_is() {
        let mut cfg = Config::default();
        cfg.disks.mounts = Mounts::Explicit(vec![String::from("/"), String::from("/data")]);

        assert_eq!(resolve_mounts(Path::new("/ignored"), &cfg), ["/", "/data"]);
    }

    #[test]
    fn resolve_mounts_auto_filters_to_roots_and_orders() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/mounts",
            "/dev/root / ext4 rw 0 0\n\
             tmpfs /boot tmpfs rw 0 0\n\
             /dev/sdb1 /run/media/user/Backup ext4 rw 0 0\n\
             /dev/sdc1 /mnt/data ext4 rw 0 0\n\
             /dev/sdd1 /media/x ext4 rw 0 0\n\
             proc /proc proc rw 0 0\n",
        );

        let cfg = Config::default();
        assert_eq!(
            resolve_mounts(&tmp.path().join("proc"), &cfg),
            ["/", "/media/x", "/mnt/data", "/run/media/user/Backup"]
        );
    }

    #[test]
    fn resolve_mounts_auto_root_only_when_nothing_under_auto_roots() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/mounts",
            "/dev/root / ext4 rw 0 0\n/dev/sda1 /boot/efi vfat rw 0 0\n",
        );

        assert_eq!(
            resolve_mounts(&tmp.path().join("proc"), &Config::default()),
            ["/"]
        );
    }

    #[test]
    fn resolve_mounts_auto_ignores_bare_root_dirs() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/mounts",
            "/dev/root / ext4 rw 0 0\n/dev/sda1 /mnt ext4 rw 0 0\n",
        );

        assert_eq!(
            resolve_mounts(&tmp.path().join("proc"), &Config::default()),
            ["/"]
        );
    }

    #[test]
    fn resolve_mounts_decodes_escaped_mount_paths() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/mounts",
            "/dev/sdb1 /run/media/user/My\\040Drive ext4 rw 0 0\n/dev/root / ext4 rw 0 0\n",
        );

        assert_eq!(
            resolve_mounts(&tmp.path().join("proc"), &Config::default()),
            ["/", "/run/media/user/My Drive"]
        );
    }

    #[test]
    fn find_hd_temp_paths_prefers_manual_overrides_before_autodetect() {
        let tmp = TempTree::new();
        tmp.mkdir("sys/class/nvme/nvme1/nvme1n1");
        tmp.mkdir("sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/2:0:0:0/block/sda");
        tmp.mkdir("sys/devices/pci0000:00/0000:00:02.0/nvme/nvme1/hwmon1");
        tmp.write(
            "sys/devices/pci0000:00/0000:00:02.0/nvme/nvme1/hwmon1/name",
            "nvme\n",
        );
        tmp.write(
            "sys/devices/pci0000:00/0000:00:02.0/nvme/nvme1/hwmon1/temp3_input",
            "44000\n",
        );
        tmp.symlink_dir(
            "sys/devices/pci0000:00/0000:00:02.0/nvme/nvme1/hwmon1",
            "sys/class/hwmon/hwmon1",
        );

        tmp.mkdir("sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0");
        tmp.write(
            "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0/name",
            "nvme\n",
        );
        tmp.write(
            "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0/temp1_input",
            "39000\n",
        );
        tmp.symlink_dir(
            "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0",
            "sys/class/hwmon/hwmon0",
        );

        let overrides = SensorOverrides {
            hd1_temp: Some(String::from("nvme|temp3_input")),
            ..SensorOverrides::default()
        };

        let paths = find_hd_temp_paths(&tmp.path().join("sys"), &overrides);

        assert_eq!(paths.len(), 1);
        assert_eq!(
            paths.get("nvme1n1"),
            Some(&tmp.path().join("sys/class/hwmon/hwmon1/temp3_input"))
        );
    }

    #[test]
    fn find_hd_temp_paths_autodetects_nvme_and_scsi_drivetemp_labels() {
        let tmp = TempTree::new();
        tmp.mkdir("sys/class/nvme/nvme0/nvme0n1");
        tmp.mkdir("sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0");
        tmp.write(
            "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0/name",
            "nvme\n",
        );
        tmp.write(
            "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0/temp1_input",
            "39000\n",
        );
        tmp.symlink_dir(
            "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0",
            "sys/class/hwmon/hwmon0",
        );

        tmp.mkdir("sys/devices/pci0000:00/0000:00:02.0/ata1/host2/target2:0:0/2:0:0:0/block/sda");
        tmp.mkdir(
            "sys/devices/pci0000:00/0000:00:02.0/ata1/host2/target2:0:0/2:0:0:0/hwmon/hwmon1",
        );
        tmp.write(
            "sys/devices/pci0000:00/0000:00:02.0/ata1/host2/target2:0:0/2:0:0:0/hwmon/hwmon1/name",
            "drivetemp\n",
        );
        tmp.write(
            "sys/devices/pci0000:00/0000:00:02.0/ata1/host2/target2:0:0/2:0:0:0/hwmon/hwmon1/temp1_input",
            "31000\n",
        );
        tmp.symlink_dir(
            "sys/devices/pci0000:00/0000:00:02.0/ata1/host2/target2:0:0/2:0:0:0/hwmon/hwmon1",
            "sys/class/hwmon/hwmon1",
        );
        tmp.symlink_dir(
            "sys/devices/pci0000:00/0000:00:02.0/ata1/host2/target2:0:0/2:0:0:0/block/sda",
            "sys/class/block/sda",
        );

        let paths = find_hd_temp_paths(&tmp.path().join("sys"), &SensorOverrides::default());

        assert_eq!(
            paths.get("nvme0n1"),
            Some(&tmp.path().join("sys/class/hwmon/hwmon0/temp1_input"))
        );
        assert_eq!(
            paths.get("sda"),
            Some(&tmp.path().join("sys/class/hwmon/hwmon1/temp1_input"))
        );
    }

    #[test]
    fn find_fan_speed_paths_stops_after_first_missing_slot() {
        let tmp = TempTree::new();
        tmp.mkdir("sys/class/hwmon/hwmon0");
        tmp.write("sys/class/hwmon/hwmon0/name", "nct6775\n");
        tmp.write("sys/class/hwmon/hwmon0/fan2_input", "1550\n");

        let overrides = SensorOverrides {
            fan2_speed: Some(String::from("nct6775|fan2_input")),
            ..SensorOverrides::default()
        };

        assert!(find_fan_speed_paths(&tmp.path().join("sys"), &overrides).is_empty());
    }

    #[test]
    fn read_hd_temp_cached_honors_ttl() {
        let tmp = TempTree::new();
        tmp.write("sys/temp", "35000\n");
        let path = tmp.path().join("sys/temp");
        let mut state = DiskState::default();

        let first = read_hd_temp_cached(&mut state, clock_at(0), "nvme0n1", &path);
        tmp.write("sys/temp", "39000\n");
        let cached = read_hd_temp_cached(&mut state, clock_at(5), "nvme0n1", &path);
        let refreshed = read_hd_temp_cached(&mut state, clock_at(31), "nvme0n1", &path);

        assert_eq!(first, Some(35));
        assert_eq!(cached, Some(35));
        assert_eq!(refreshed, Some(39));
    }

    #[test]
    fn read_fan_speed_cached_honors_ttl() {
        let tmp = TempTree::new();
        tmp.write("sys/fan", "1500\n");
        let path = tmp.path().join("sys/fan");
        let mut state = DiskState::default();

        let first = read_fan_speed_cached(&mut state, clock_at(0), "1", &path);
        tmp.write("sys/fan", "1700\n");
        let cached = read_fan_speed_cached(&mut state, clock_at(10), "1", &path);
        let refreshed = read_fan_speed_cached(&mut state, clock_at(31), "1", &path);

        assert_eq!(first, Some(1500));
        assert_eq!(cached, Some(1500));
        assert_eq!(refreshed, Some(1700));
    }

    #[test]
    fn detect_disk_io_device_walks_partition_to_whole_disk() {
        let tmp = TempTree::new();
        tmp.write("proc/mounts", "/dev/nvme0n1p2 / ext4 rw 0 0\n");
        tmp.mkdir("sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/nvme0n1/nvme0n1p2");
        tmp.write(
            "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/nvme0n1/nvme0n1p2/partition",
            "2\n",
        );
        tmp.symlink_dir(
            "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/nvme0n1/nvme0n1p2",
            "sys/class/block/nvme0n1p2",
        );

        assert_eq!(
            detect_disk_io_device(&tmp.path().join("proc"), &tmp.path().join("sys"), "/"),
            Some(String::from("nvme0n1"))
        );
    }

    #[test]
    fn detect_disk_io_device_keeps_mapper_name_when_no_single_parent_exists() {
        let tmp = TempTree::new();
        tmp.write("proc/mounts", "/dev/mapper/vg-root / ext4 rw 0 0\n");

        assert_eq!(
            detect_disk_io_device(&tmp.path().join("proc"), &tmp.path().join("sys"), "/"),
            Some(String::from("vg-root"))
        );
    }

    #[test]
    fn detect_disks_finds_supported_whole_disks_and_rotational_flags() {
        let tmp = TempTree::new();
        tmp.write("sys/block/nvme0n1/queue/rotational", "0\n");
        tmp.write("sys/block/sda/queue/rotational", "1\n");
        tmp.write("sys/block/sr0/queue/rotational", "1\n");
        tmp.write("sys/block/loop0/queue/rotational", "0\n");

        assert_eq!(
            detect_disks(&tmp.path().join("sys")),
            vec![
                DiskIdentity {
                    label: String::from("nvme0n1"),
                    kind: DiskKind::Nvme,
                    rotational: false,
                },
                DiskIdentity {
                    label: String::from("sda"),
                    kind: DiskKind::Ata,
                    rotational: true,
                },
            ]
        );
    }

    #[test]
    fn read_disk_io_needs_two_samples_and_resets_on_device_change() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/diskstats",
            "259 0 nvme0n1 0 0 8 0 0 0 4 0 0 0 0 0 0 0 0 0\n\
             8 0 sda 0 0 2 0 0 0 2 0 0 0 0 0 0 0 0 0\n",
        );

        let mut state = DiskState::default();
        assert_eq!(
            read_disk_io(&tmp.path().join("proc"), &mut state, "nvme0n1", clock_at(0)),
            (None, None)
        );

        tmp.write(
            "proc/diskstats",
            "259 0 nvme0n1 0 0 24 0 0 0 20 0 0 0 0 0 0 0 0 0\n\
             8 0 sda 0 0 2 0 0 0 2 0 0 0 0 0 0 0 0 0\n",
        );
        assert_eq!(
            read_disk_io(&tmp.path().join("proc"), &mut state, "nvme0n1", clock_at(2)),
            (Some(4096), Some(4096))
        );

        assert_eq!(
            read_disk_io(&tmp.path().join("proc"), &mut state, "sda", clock_at(3)),
            (None, None)
        );
    }

    #[test]
    fn read_disk_io_resets_on_counter_rollback_and_zero_dt() {
        let tmp = TempTree::new();
        tmp.write(
            "proc/diskstats",
            "259 0 nvme0n1 0 0 10 0 0 0 12 0 0 0 0 0 0 0 0 0\n",
        );

        let mut state = DiskState::default();
        let _ = read_disk_io(&tmp.path().join("proc"), &mut state, "nvme0n1", clock_at(0));

        tmp.write(
            "proc/diskstats",
            "259 0 nvme0n1 0 0 12 0 0 0 14 0 0 0 0 0 0 0 0 0\n",
        );
        assert_eq!(
            read_disk_io(&tmp.path().join("proc"), &mut state, "nvme0n1", clock_at(0)),
            (None, None)
        );

        tmp.write(
            "proc/diskstats",
            "259 0 nvme0n1 0 0 2 0 0 0 3 0 0 0 0 0 0 0 0 0\n",
        );
        assert_eq!(
            read_disk_io(&tmp.path().join("proc"), &mut state, "nvme0n1", clock_at(1)),
            (None, None)
        );
    }

    #[test]
    fn read_disk_usage_returns_none_for_missing_mount() {
        assert_eq!(read_disk_usage(Path::new("/definitely/not/here")), None);
    }

    #[test]
    fn disk_usage_formula_matches_df_style_percent_and_half_even_rounding() {
        let gib = BYTES_PER_GIB;
        let usage = disk_usage_from_bytes(5 * gib, gib + gib / 2, gib / 2);

        assert_eq!(usage.percent, 87);
        assert_eq!(usage.used_gb, 4);
        assert_eq!(usage.total_gb, 5);

        let half_even = disk_usage_from_bytes(5 * gib, 2 * gib + gib / 2, gib / 2);
        assert_eq!(half_even.used_gb, 2);
    }
}
