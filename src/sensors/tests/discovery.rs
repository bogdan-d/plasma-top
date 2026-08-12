use super::*;

use crate::sensors::{DiscoveryOutcome, ReconciliationOutcome};

#[derive(Clone, Copy)]
enum RouteReply {
    Device(&'static str),
    Empty,
    Malformed,
    Failed,
}

fn enqueue_route_reply(commands: &mut FakeCommandRunner, args: [&str; 3], reply: RouteReply) {
    let mut output = ok_cmd(IP, &args, "");
    match reply {
        RouteReply::Device(device) => {
            output.stdout = format!("default dev {device}\n").into_bytes();
        }
        RouteReply::Empty => {}
        RouteReply::Malformed => output.stdout = b"default via 10.0.0.1\n".to_vec(),
        RouteReply::Failed => output.status = CommandStatus::Exit(1),
    }
    commands.enqueue(IP, args, output);
}

fn route_outcome(first: RouteReply, fallback: RouteReply) -> DiscoveryOutcome<Option<String>> {
    let mut commands = FakeCommandRunner::new();
    enqueue_route_reply(&mut commands, ["route", "get", "8.8.8.8"], first);
    if !matches!(first, RouteReply::Device(_)) {
        enqueue_route_reply(&mut commands, ["route", "show", "default"], fallback);
    }
    detect_net_device(&mut commands)
}

#[test]
fn route_discovery_covers_primary_and_fallback_combinations() {
    assert_eq!(
        route_outcome(RouteReply::Device("eth0"), RouteReply::Failed),
        DiscoveryOutcome::Confirmed(Some(String::from("eth0")))
    );

    for first in [RouteReply::Empty, RouteReply::Malformed, RouteReply::Failed] {
        assert_eq!(
            route_outcome(first, RouteReply::Device("wlan0")),
            DiscoveryOutcome::Confirmed(Some(String::from("wlan0")))
        );
        assert_eq!(
            route_outcome(first, RouteReply::Empty),
            DiscoveryOutcome::Confirmed(None)
        );
        assert_eq!(
            route_outcome(first, RouteReply::Failed),
            DiscoveryOutcome::Failed
        );
        assert_eq!(
            route_outcome(first, RouteReply::Malformed),
            DiscoveryOutcome::Failed
        );
    }
}

#[test]
fn discover_hardware_populates_paths_and_flags_from_fixtures() {
    let tree = TempTree::new();
    // CPU temp/freq/turbo
    tree.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
    tree.write("sys/class/hwmon/hwmon0/temp1_input", "45000\n");
    tree.write(
        "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
        "3000000\n",
    );
    tree.write("sys/devices/system/cpu/intel_pstate/no_turbo", "0\n");
    // hd_temp + fan
    tree.mkdir("sys/class/nvme/nvme0/nvme0n1");
    tree.mkdir("sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0");
    tree.write(
        "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0/name",
        "nvme\n",
    );
    tree.write(
        "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0/temp1_input",
        "35000\n",
    );
    // Link the nvme hwmon into /sys/class/hwmon so the autodetect scan sees it.
    tree.symlink(
        tree.root
            .join("sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/hwmon0")
            .to_str()
            .expect("path"),
        "sys/class/hwmon/hwmon2",
    );
    tree.mkdir("sys/class/hwmon/hwmon1");
    tree.write("sys/class/hwmon/hwmon1/name", "nct6775\n");
    tree.write("sys/class/hwmon/hwmon1/fan1_input", "1200\n");
    // backlight + wifi
    tree.write("sys/class/backlight/intel_backlight/brightness", "500\n");
    tree.write(
        "sys/class/backlight/intel_backlight/max_brightness",
        "1000\n",
    );
    tree.mkdir("sys/class/net/wlan0/wireless");
    tree.write("sys/class/net/wlan0/statistics/tx_bytes", "0\n");
    // disk_io_device for "/"
    tree.write("proc/mounts", "/dev/nvme0n1p2 / ext4 rw 0 0\n");
    tree.mkdir("sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/nvme0n1/nvme0n1p2");
    tree.write(
        "sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/nvme0n1/nvme0n1p2/partition",
        "2\n",
    );
    tree.mkdir("sys/class/block");
    std::os::unix::fs::symlink(
        tree.root
            .join("sys/devices/pci0000:00/0000:00:01.0/nvme/nvme0/nvme0n1/nvme0n1p2"),
        tree.sys().join("class/block/nvme0n1p2"),
    )
    .expect("block symlink");

    // UPower: one system battery + one hidpp mouse. discover_hardware issues
    // EnumerateDevices twice (find_battery_sys, then find_peripherals); both consume
    // the same full path list.
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&[
            "/org/freedesktop/UPower/devices/battery_BAT0",
            "/org/freedesktop/UPower/devices/battery_hidpp_mouse",
            "/org/freedesktop/UPower/devices/battery_BAT1",
        ]),
    );
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&[
            "/org/freedesktop/UPower/devices/battery_BAT0",
            "/org/freedesktop/UPower/devices/battery_hidpp_mouse",
            "/org/freedesktop/UPower/devices/battery_BAT1",
        ]),
    );
    // find_peripherals: one GetAll for the hidpp mouse (Type=5).
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        "/org/freedesktop/UPower/devices/battery_hidpp_mouse",
        "org.freedesktop.DBus.Properties",
        "GetAll",
        getall_reply(
            "/org/freedesktop/UPower/devices/battery_hidpp_mouse",
            &[("Model", "MX Master"), ("Type", "5")],
        ),
    );

    // net device via `ip route get`.
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 via 1.2.3.4 dev wlan0\n",
        ),
    );

    let mut cfg = cfg_panel(&["cpu_temp"]);
    cfg.sensors.fan1_speed = Some(String::from("nct6775|fan1_input"));
    let hw = discover_hardware(&tree.sys(), &tree.proc(), &cfg, &mut dbus, &mut commands, 8);

    assert_eq!(
        hw.cpu_temp_path.as_deref(),
        Some(tree.sys().join("class/hwmon/hwmon0/temp1_input").as_path())
    );
    assert!(hw.cpu_freq_path.is_some());
    assert!(hw.cpu_turbo_supported);
    assert!(hw.hd_temp_paths.contains_key("nvme0n1"));
    assert!(hw.fan_paths.contains_key("1"));
    assert_eq!(
        hw.battery_sys_ids,
        [
            "/org/freedesktop/UPower/devices/battery_BAT0".to_owned(),
            "/org/freedesktop/UPower/devices/battery_BAT1".to_owned()
        ]
    );
    assert_eq!(
        hw.battery_mouse_id.as_deref(),
        Some("/org/freedesktop/UPower/devices/battery_hidpp_mouse")
    );
    assert!(hw.battery_kbd_id.is_none());
    assert!(!hw.has_nvidia);
    assert!(hw.has_backlight);
    assert!(hw.has_wifi);
    assert_eq!(hw.net_device.as_deref(), Some("wlan0"));
    assert_eq!(hw.cpu_count, 8);
    // SMART is enabled by default, but the fake has no managed-object reply.
    assert!(hw.disk_smart_drives.is_empty());
    let trace = dbus.call_trace();
    assert_eq!(trace[0].metadata().4, "EnumerateDevices"); // system batteries
    assert_eq!(trace[1].metadata().4, "GetManagedObjects"); // SMART disks
    assert_eq!(trace[2].metadata().4, "EnumerateDevices"); // peripherals
    assert_eq!(trace[3].metadata().4, "GetAll");
}

#[test]
fn discover_hardware_degrades_to_safe_defaults_on_absence() {
    let tree = TempTree::new();
    let mut dbus = FakeDbus::new();
    let mut commands = FakeCommandRunner::new();
    let cfg = Config::default();

    let hw = discover_hardware(&tree.sys(), &tree.proc(), &cfg, &mut dbus, &mut commands, 0);

    assert!(hw.cpu_temp_path.is_none());
    assert!(hw.hd_temp_paths.is_empty());
    assert!(hw.battery_sys_ids.is_empty());
    assert!(hw.battery_mouse_id.is_none());
    assert!(!hw.has_nvidia);
    assert!(!hw.has_backlight);
    assert!(!hw.has_wifi);
    assert!(hw.net_device.is_none());
    assert_eq!(hw.cpu_count, 1);
}

#[test]
fn local_startup_discovery_seeds_sysfs_inventory_without_slow_boundaries() {
    let tree = TempTree::new();
    tree.write("sys/class/hwmon/hwmon0/name", "nct6775\n");
    tree.write("sys/class/hwmon/hwmon0/fan1_input", "1200\n");
    tree.write("sys/class/backlight/panel/brightness", "50\n");
    tree.write("sys/class/backlight/panel/max_brightness", "100\n");
    tree.mkdir("sys/class/net/wlan0/wireless");
    tree.mkdir("sys/bus/pci/devices");
    tree.mkdir("sys/class/drm");
    tree.mkdir("sys/devices/system/cpu");
    tree.write("proc/mounts", "");
    let mut cfg = Config::default();
    cfg.sensors.fan1_speed = Some(String::from("nct6775|fan1_input"));

    let hw = crate::sensors::discover_local_hardware(&tree.sys(), &tree.proc(), &cfg, 8);

    assert_eq!(hw.cpu_count, 8);
    assert!(hw.fan_paths.contains_key("1"));
    assert!(hw.has_backlight);
    assert!(hw.has_wifi);
    assert!(hw.battery_sys_ids.is_empty());
    assert!(hw.disk_smart_drives.is_empty());
    assert!(hw.net_device.is_none());
}

#[test]
fn family_reconciliation_confirms_removal_without_polling_other_families() {
    let tree = TempTree::new();
    tree.mkdir("sys/class/hwmon");
    let mut cfg = Config::default();
    cfg.sensors.fan1_speed = Some(String::from("nct6775|fan1_input"));
    let mut hw = HardwareInventory {
        fan_paths: [(String::from("1"), PathBuf::from("/old/fan"))].into(),
        battery_sys_ids: vec![String::from("BAT0")],
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut dbus = FakeDbus::new();
    let mut commands = FakeCommandRunner::new();

    let outcome = crate::sensors::reconcile_inventory_family(
        crate::domain::readings::InventoryFamily::Thermal,
        &mut hw,
        &tree.sys(),
        &tree.proc(),
        &cfg,
        &mut dbus,
        &mut commands,
    );

    assert_eq!(outcome, ReconciliationOutcome::Captured);
    assert!(hw.fan_paths.is_empty());
    assert_eq!(hw.battery_sys_ids, ["BAT0"]);
    assert!(hw.has_nvidia);
    assert!(dbus.call_trace().is_empty());
    assert!(commands.call_trace().is_empty());
}

#[test]
fn route_reconciliation_retains_failure_and_captures_confirmed_absence() {
    let tree = TempTree::new();
    tree.mkdir("sys/class/net");
    let mut hw = HardwareInventory {
        net_device: Some(String::from("eth0")),
        ..HardwareInventory::default()
    };
    let mut dbus = FakeDbus::new();
    let mut failed_commands = FakeCommandRunner::new();

    let failed = crate::sensors::reconcile_inventory_family(
        crate::domain::readings::InventoryFamily::Network,
        &mut hw,
        &tree.sys(),
        &tree.proc(),
        &Config::default(),
        &mut dbus,
        &mut failed_commands,
    );

    assert_eq!(failed, ReconciliationOutcome::Failed);
    assert_eq!(hw.net_device.as_deref(), Some("eth0"));

    let mut absent_commands = FakeCommandRunner::new();
    for args in [["route", "get", "8.8.8.8"], ["route", "show", "default"]] {
        absent_commands.enqueue(IP, args, ok_cmd(IP, &args, ""));
    }
    let absent = crate::sensors::reconcile_inventory_family(
        crate::domain::readings::InventoryFamily::Network,
        &mut hw,
        &tree.sys(),
        &tree.proc(),
        &Config::default(),
        &mut dbus,
        &mut absent_commands,
    );

    assert_eq!(absent, ReconciliationOutcome::Captured);
    assert!(hw.net_device.is_none());
}

#[test]
fn disk_io_reconciliation_captures_unsupported_source_and_adopts_later_device() {
    let tree = TempTree::new();
    tree.write("proc/mounts", "composefs / overlay rw 0 0\n");
    let mut hw = HardwareInventory {
        disk_io_device: Some(String::from("old-disk")),
        ..HardwareInventory::default()
    };
    let mut dbus = FakeDbus::new();
    let mut commands = FakeCommandRunner::new();
    let reconcile =
        |hw: &mut HardwareInventory, dbus: &mut FakeDbus, commands: &mut FakeCommandRunner| {
            crate::sensors::reconcile_inventory_family(
                crate::domain::readings::InventoryFamily::DiskIo,
                hw,
                &tree.sys(),
                &tree.proc(),
                &Config::default(),
                dbus,
                commands,
            )
        };

    assert_eq!(
        reconcile(&mut hw, &mut dbus, &mut commands),
        ReconciliationOutcome::Captured
    );
    assert!(hw.disk_io_device.is_none());

    tree.mkdir("sys/class/block/sda");
    tree.write("proc/mounts", "/dev/sda / ext4 rw 0 0\n");
    assert_eq!(
        reconcile(&mut hw, &mut dbus, &mut commands),
        ReconciliationOutcome::Captured
    );
    assert_eq!(hw.disk_io_device.as_deref(), Some("sda"));

    tree.write("proc/mounts", "malformed\n");
    assert_eq!(
        reconcile(&mut hw, &mut dbus, &mut commands),
        ReconciliationOutcome::Failed
    );
    assert_eq!(hw.disk_io_device.as_deref(), Some("sda"));
}

#[test]
fn battery_reconciliation_retains_dbus_failure_and_captures_absence() {
    let tree = TempTree::new();
    let mut hw = HardwareInventory {
        battery_sys_ids: vec![String::from("BAT0")],
        ..HardwareInventory::default()
    };
    let mut commands = FakeCommandRunner::new();
    let mut failed_dbus = FakeDbus::new();

    let failed = crate::sensors::reconcile_inventory_family(
        crate::domain::readings::InventoryFamily::SystemBattery,
        &mut hw,
        &tree.sys(),
        &tree.proc(),
        &Config::default(),
        &mut failed_dbus,
        &mut commands,
    );

    assert_eq!(failed, ReconciliationOutcome::Failed);
    assert_eq!(hw.battery_sys_ids, ["BAT0"]);

    let mut absent_dbus = FakeDbus::new();
    absent_dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&[]),
    );
    let absent = crate::sensors::reconcile_inventory_family(
        crate::domain::readings::InventoryFamily::SystemBattery,
        &mut hw,
        &tree.sys(),
        &tree.proc(),
        &Config::default(),
        &mut absent_dbus,
        &mut commands,
    );

    assert_eq!(absent, ReconciliationOutcome::Captured);
    assert!(hw.battery_sys_ids.is_empty());
}

#[test]
fn sysfs_reconciliation_retains_incomplete_enumeration_and_captures_absence() {
    let tree = TempTree::new();
    let mut cfg = Config::default();
    cfg.sensors.fan1_speed = Some(String::from("nct6775|fan1_input"));
    let mut hw = HardwareInventory {
        fan_paths: [(String::from("1"), PathBuf::from("/old/fan"))].into(),
        ..HardwareInventory::default()
    };
    let mut dbus = FakeDbus::new();
    let mut commands = FakeCommandRunner::new();

    let failed = crate::sensors::reconcile_inventory_family(
        crate::domain::readings::InventoryFamily::Thermal,
        &mut hw,
        &tree.sys(),
        &tree.proc(),
        &cfg,
        &mut dbus,
        &mut commands,
    );

    assert_eq!(failed, ReconciliationOutcome::Failed);
    assert!(hw.fan_paths.contains_key("1"));

    tree.mkdir("sys/class/hwmon");
    let absent = crate::sensors::reconcile_inventory_family(
        crate::domain::readings::InventoryFamily::Thermal,
        &mut hw,
        &tree.sys(),
        &tree.proc(),
        &cfg,
        &mut dbus,
        &mut commands,
    );

    assert_eq!(absent, ReconciliationOutcome::Captured);
    assert!(hw.fan_paths.is_empty());
}

#[test]
fn discover_hardware_enumerates_smart_drives_when_enabled() {
    let tree = TempTree::new();
    tree.write("sys/block/nvme0n1/queue/rotational", "0\n");
    let mut dbus = FakeDbus::new();
    // find_battery_sys (Enumerate) + detect_smart_disks (GetManagedObjects) +
    // find_peripherals (Enumerate), matching Python discovery order.
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&[]),
    );
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&[]),
    );
    dbus.enqueue(
        SYSTEM,
        "org.freedesktop.UDisks2",
        "/org/freedesktop/UDisks2",
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        DbusOutput::UdisksManagedObjects(vec![
            UdisksManagedObject {
                path: "/org/freedesktop/UDisks2/block_devices/nvme0n1".to_owned(),
                interfaces: BTreeSet::from(["org.freedesktop.UDisks2.Block".to_owned()]),
                drive: Some("/org/freedesktop/UDisks2/drives/NVMe_1".to_owned()),
            },
            UdisksManagedObject {
                path: "/org/freedesktop/UDisks2/drives/NVMe_1".to_owned(),
                interfaces: BTreeSet::from(["org.freedesktop.UDisks2.NVMe.Controller".to_owned()]),
                drive: None,
            },
        ]),
    );
    let mut commands = FakeCommandRunner::new();
    let cfg = cfg_panel(&["disk_smart:pair"]);

    let hw = discover_hardware(&tree.sys(), &tree.proc(), &cfg, &mut dbus, &mut commands, 4);

    let drive = hw.disk_smart_drives.get("nvme0n1").expect("smart drive");
    assert_eq!(drive.object_path, "/org/freedesktop/UDisks2/drives/NVMe_1");
    assert!(!drive.rotational);
    let trace = dbus.call_trace();
    assert_eq!(trace[0].metadata().4, "EnumerateDevices");
    assert_eq!(trace[1].metadata().4, "GetManagedObjects");
    assert_eq!(trace[2].metadata().4, "EnumerateDevices");
}

// ── needs_periph_rescan ──────────────────────────────────────────────────────

#[test]
fn needs_periph_rescan_when_mouse_wanted_and_absent() {
    let cfg = cfg_panel(&["battery_mouse"]);
    let hw = HardwareInventory::default();
    assert!(needs_periph_rescan(&hw, &cfg));
}

#[test]
fn needs_periph_rescan_skipped_when_mouse_id_known() {
    let cfg = cfg_panel(&["battery_mouse"]);
    let mut hw = HardwareInventory::default();
    hw.battery_mouse_id = Some("/battery_hidpp_mouse".to_owned());
    assert!(!needs_periph_rescan(&hw, &cfg));
}

#[test]
fn needs_periph_rescan_skipped_when_bolt_configured() {
    let mut cfg = cfg_panel(&["battery_mouse"]);
    cfg.battery.mouse_bolt = Some(1);
    let hw = HardwareInventory::default();
    // Bolt devices are addressed by index, not enumerated → no rescan.
    assert!(!needs_periph_rescan(&hw, &cfg));
}

#[test]
fn needs_periph_rescan_when_net_device_missing_and_a_net_item_configured() {
    let cfg = cfg_panel(&["net_speed"]);
    let hw = HardwareInventory::default();
    assert!(needs_periph_rescan(&hw, &cfg));

    let mut hw = HardwareInventory::default();
    hw.net_device = Some("eth0".to_owned());
    assert!(!needs_periph_rescan(&hw, &cfg));
}

#[test]
fn needs_periph_rescan_false_when_nothing_wanted() {
    let cfg = cfg_panel(&["cpu_usage"]);
    let hw = HardwareInventory::default();
    assert!(!needs_periph_rescan(&hw, &cfg));
}

// ── rescan_peripherals ───────────────────────────────────────────────────────

#[test]
fn rescan_finds_peripherals_and_clears_confirmed_removals() {
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&["/battery_hidpp_kbd"]),
    );
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        "/battery_hidpp_kbd",
        "org.freedesktop.DBus.Properties",
        "GetAll",
        getall_reply("/battery_hidpp_kbd", &[("Model", "K780"), ("Type", "6")]),
    );
    let mut commands = FakeCommandRunner::new();
    let cfg = cfg_panel(&["battery_kbd"]);

    let mut hw = HardwareInventory {
        battery_mouse_id: Some("/old_mouse".to_owned()),
        net_device: Some("eth0".to_owned()),
        ..HardwareInventory::default()
    };
    rescan_peripherals(&mut hw, &cfg, &mut dbus, &mut commands);

    assert!(hw.battery_mouse_id.is_none());
    assert_eq!(hw.battery_kbd_id.as_deref(), Some("/battery_hidpp_kbd"));
    // net_device already known → not retried (no ip command).
    assert_eq!(hw.net_device.as_deref(), Some("eth0"));
    assert!(commands.call_trace().is_empty());
}

#[test]
fn rescan_retries_net_device_only_when_still_missing() {
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&[]),
    );
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(IP, &["route", "get", "8.8.8.8"], "8.8.8.8 dev wlan0\n"),
    );
    let cfg = cfg_panel(&["net_speed"]);

    let mut hw = HardwareInventory::default(); // net_device None
    rescan_peripherals(&mut hw, &cfg, &mut dbus, &mut commands);

    assert_eq!(hw.net_device.as_deref(), Some("wlan0"));
}

#[test]
fn rescan_failure_retains_existing_peripheral_ids() {
    let mut dbus = FakeDbus::new();
    let mut commands = FakeCommandRunner::new();
    let cfg = cfg_panel(&["battery_mouse", "battery_kbd"]);
    let mut hw = HardwareInventory {
        battery_mouse_id: Some("/old_mouse".to_owned()),
        battery_kbd_id: Some("/old_keyboard".to_owned()),
        net_device: Some("eth0".to_owned()),
        ..HardwareInventory::default()
    };

    rescan_peripherals(&mut hw, &cfg, &mut dbus, &mut commands);

    assert_eq!(hw.battery_mouse_id.as_deref(), Some("/old_mouse"));
    assert_eq!(hw.battery_kbd_id.as_deref(), Some("/old_keyboard"));
}

#[test]
fn reload_inventory_retains_failures_clears_confirmed_absence_and_readds() {
    let tree = TempTree::new();
    tree.mkdir("sys/class/hwmon");
    tree.mkdir("sys/bus/pci/devices");
    tree.mkdir("sys/class/drm");
    tree.mkdir("sys/class/backlight");
    tree.mkdir("sys/class/net");
    tree.write("proc/mounts", "");
    tree.write("sys/block/nvme0n1/queue/rotational", "0\n");
    let cfg = Config::default();
    let drive = crate::domain::readings::SmartDisk {
        object_path: "/old_drive".to_owned(),
        interface: crate::domain::readings::DiskSmartInterface::Nvme,
        rotational: false,
    };
    let mut hw = HardwareInventory {
        battery_sys_ids: vec!["/old_BAT".to_owned()],
        battery_mouse_id: Some("/old_mouse".to_owned()),
        battery_kbd_id: Some("/old_keyboard".to_owned()),
        net_device: Some("eth0".to_owned()),
        disk_smart_drives: [("old".to_owned(), drive)].into(),
        ..HardwareInventory::default()
    };

    let mut failed_dbus = FakeDbus::new();
    let mut failed_commands = FakeCommandRunner::new();
    crate::sensors::discover_hardware_attempt(
        &tree.sys(),
        &tree.proc(),
        &cfg,
        &mut failed_dbus,
        &mut failed_commands,
        4,
    )
    .merge_into(&mut hw);
    assert_eq!(hw.battery_sys_ids, ["/old_BAT"]);
    assert_eq!(hw.battery_mouse_id.as_deref(), Some("/old_mouse"));
    assert_eq!(hw.net_device.as_deref(), Some("eth0"));
    assert!(hw.disk_smart_drives.contains_key("old"));

    let mut empty_dbus = FakeDbus::new();
    empty_dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&[]),
    );
    empty_dbus.enqueue(
        SYSTEM,
        "org.freedesktop.UDisks2",
        "/org/freedesktop/UDisks2",
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        DbusOutput::UdisksManagedObjects(Vec::new()),
    );
    empty_dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&[]),
    );
    let mut empty_commands = FakeCommandRunner::new();
    for args in [["route", "get", "8.8.8.8"], ["route", "show", "default"]] {
        empty_commands.enqueue(IP, args, ok_cmd(IP, &args, ""));
    }
    crate::sensors::discover_hardware_attempt(
        &tree.sys(),
        &tree.proc(),
        &cfg,
        &mut empty_dbus,
        &mut empty_commands,
        4,
    )
    .merge_into(&mut hw);
    assert!(hw.battery_sys_ids.is_empty());
    assert!(hw.battery_mouse_id.is_none());
    assert!(hw.battery_kbd_id.is_none());
    assert!(hw.net_device.is_none());
    assert!(hw.disk_smart_drives.is_empty());

    let mut readd_dbus = FakeDbus::new();
    readd_dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&["/battery_BAT0"]),
    );
    readd_dbus.enqueue(
        SYSTEM,
        "org.freedesktop.UDisks2",
        "/org/freedesktop/UDisks2",
        "org.freedesktop.DBus.ObjectManager",
        "GetManagedObjects",
        DbusOutput::UdisksManagedObjects(vec![
            UdisksManagedObject {
                path: "/org/freedesktop/UDisks2/block_devices/nvme0n1".to_owned(),
                interfaces: BTreeSet::from(["org.freedesktop.UDisks2.Block".to_owned()]),
                drive: Some("/drives/NVMe".to_owned()),
            },
            UdisksManagedObject {
                path: "/drives/NVMe".to_owned(),
                interfaces: BTreeSet::from(["org.freedesktop.UDisks2.NVMe.Controller".to_owned()]),
                drive: None,
            },
        ]),
    );
    readd_dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        enumerate_reply(&["/battery_hidpp_mouse"]),
    );
    readd_dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        "/battery_hidpp_mouse",
        "org.freedesktop.DBus.Properties",
        "GetAll",
        getall_reply(
            "/battery_hidpp_mouse",
            &[("Model", "MX Master"), ("Type", "5")],
        ),
    );
    let mut readd_commands = FakeCommandRunner::new();
    readd_commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(IP, &["route", "get", "8.8.8.8"], "default dev wlan0\n"),
    );
    crate::sensors::discover_hardware_attempt(
        &tree.sys(),
        &tree.proc(),
        &cfg,
        &mut readd_dbus,
        &mut readd_commands,
        4,
    )
    .merge_into(&mut hw);
    assert_eq!(hw.battery_sys_ids, ["/battery_BAT0"]);
    assert_eq!(hw.battery_mouse_id.as_deref(), Some("/battery_hidpp_mouse"));
    assert_eq!(hw.net_device.as_deref(), Some("wlan0"));
    assert!(hw.disk_smart_drives.contains_key("nvme0n1"));
}

#[test]
fn incomplete_local_enumerations_retain_inventory_until_completed_absence() {
    let incomplete = TempTree::new();
    incomplete.mkdir("sys/class/hwmon/hwmon0");
    incomplete.write("sys/bus/pci/devices/0000:01:00.0/vendor", "malformed\n");
    incomplete.write("sys/class/drm/card0/device/vendor", "malformed\n");
    incomplete.write("sys/class/backlight", "not a directory\n");
    incomplete.write("sys/class/net", "not a directory\n");
    incomplete.write("proc/mounts", "malformed\n");

    let old_temp = PathBuf::from("/old/cpu_temp");
    let old_hd_temp = PathBuf::from("/old/disk_temp");
    let old_intel_freq = PathBuf::from("/old/intel_freq");
    let mut hw = HardwareInventory {
        cpu_temp_path: Some(old_temp.clone()),
        hd_temp_paths: [(String::from("old"), old_hd_temp.clone())].into(),
        has_nvidia: true,
        intel_gpu_freq_path: Some(old_intel_freq.clone()),
        intel_gpu_pci: Some(String::from("0000:00:02.0")),
        disk_io_device: Some(String::from("old-disk")),
        has_backlight: true,
        has_wifi: true,
        ..HardwareInventory::default()
    };
    let mut cfg = Config::default();
    cfg.disks.smart = false;
    let mut dbus = FakeDbus::new();
    let mut commands = FakeCommandRunner::new();

    crate::sensors::discover_hardware_attempt(
        &incomplete.sys(),
        &incomplete.proc(),
        &cfg,
        &mut dbus,
        &mut commands,
        4,
    )
    .merge_into(&mut hw);

    assert_eq!(hw.cpu_temp_path, Some(old_temp));
    assert_eq!(hw.hd_temp_paths.get("old"), Some(&old_hd_temp));
    assert!(hw.has_nvidia);
    assert_eq!(hw.intel_gpu_freq_path, Some(old_intel_freq));
    assert_eq!(hw.intel_gpu_pci.as_deref(), Some("0000:00:02.0"));
    assert_eq!(hw.disk_io_device.as_deref(), Some("old-disk"));
    assert!(hw.has_backlight);
    assert!(hw.has_wifi);

    let complete = TempTree::new();
    for path in [
        "sys/class/hwmon",
        "sys/devices/system/cpu",
        "sys/bus/pci/devices",
        "sys/class/drm",
        "sys/class/backlight",
        "sys/class/net",
    ] {
        complete.mkdir(path);
    }
    complete.write("proc/mounts", "");
    let mut dbus = FakeDbus::new();
    let mut commands = FakeCommandRunner::new();
    crate::sensors::discover_hardware_attempt(
        &complete.sys(),
        &complete.proc(),
        &cfg,
        &mut dbus,
        &mut commands,
        4,
    )
    .merge_into(&mut hw);

    assert!(hw.cpu_temp_path.is_none());
    assert!(hw.hd_temp_paths.is_empty());
    assert!(!hw.has_nvidia);
    assert!(hw.intel_gpu_freq_path.is_none());
    assert!(hw.intel_gpu_pci.is_none());
    assert_eq!(hw.disk_io_device.as_deref(), Some("old-disk"));
    assert!(!hw.has_backlight);
    assert!(!hw.has_wifi);
}

// ── collect: empty capability set ────────────────────────────────────────────
