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
        let root =
            std::env::temp_dir().join(format!("plasma-top-disk-{}-{unique}", std::process::id()));
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
    tmp.mkdir("sys/devices/pci0000:00/0000:00:02.0/ata1/host2/target2:0:0/2:0:0:0/hwmon/hwmon1");
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
fn detect_disk_io_device_reports_malformed_mounts_and_incomplete_sysfs() {
    let malformed = TempTree::new();
    malformed.write("proc/mounts", "malformed\n");
    assert!(
        detect_disk_io_device_outcome(
            &malformed.path().join("proc"),
            &malformed.path().join("sys"),
            "/"
        )
        .is_err()
    );

    let incomplete = TempTree::new();
    incomplete.write("proc/mounts", "/dev/nvme0n1p2 / ext4 rw 0 0\n");
    incomplete.write("sys/class/block/nvme0n1p2", "not a block entry\n");
    assert!(
        detect_disk_io_device_outcome(
            &incomplete.path().join("proc"),
            &incomplete.path().join("sys"),
            "/"
        )
        .is_err()
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
