use super::*;

use crate::domain::boundary::BoundaryError;
use crate::domain::readings::{DiskSmartInterface, SmartDisk};

fn enqueue_smart(dbus: &mut FakeDbus, drive: &str, warning: &str) {
    dbus.enqueue(
        SYSTEM,
        "org.freedesktop.UDisks2",
        drive,
        "org.freedesktop.UDisks2.NVMe.Controller",
        "SmartUpdate",
        DbusOutput {
            bus: SYSTEM,
            service: "org.freedesktop.UDisks2".to_owned(),
            object_path: drive.to_owned(),
            interface: "org.freedesktop.UDisks2.NVMe.Controller".to_owned(),
            member: "SmartUpdate".to_owned(),
            body: Vec::new(),
        },
    );
    dbus.enqueue(
        SYSTEM,
        "org.freedesktop.UDisks2",
        drive,
        "org.freedesktop.DBus.Properties",
        "Get",
        DbusOutput {
            bus: SYSTEM,
            service: "org.freedesktop.UDisks2".to_owned(),
            object_path: drive.to_owned(),
            interface: "org.freedesktop.DBus.Properties".to_owned(),
            member: "Get".to_owned(),
            body: vec![warning.to_owned()],
        },
    );
}

fn isolated_cfg(items: &[&str]) -> Config {
    let mut cfg = cfg_panel(items);
    cfg.tooltip.sections.clear();
    cfg.pages.order.clear();
    cfg.notifications.disk_usage = false;
    cfg.notifications.disk_smart = false;
    cfg.notifications.cpu_temp = false;
    cfg.notifications.gpu_nvidia_temp = false;
    cfg.notifications.hd_temp = false;
    cfg.notifications.battery_sys = false;
    cfg.notifications.battery_mouse = false;
    cfg.notifications.battery_kbd = false;
    cfg.notifications.server_check = false;
    cfg.notifications.load_avg = false;
    cfg
}

#[test]
fn initial_system_battery_failure_obeys_thirty_second_attempt_cadence() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["battery_sys"]);
    let mut hw = HardwareInventory {
        battery_sys_ids: vec!["/battery_BAT0".to_owned()],
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();

    let first = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(0),
        false,
    );
    let cached = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(1),
        false,
    );

    assert!(first.battery_sys.is_empty());
    assert!(cached.battery_sys.is_empty());
    assert_eq!(dbus.call_trace().len(), 1);
    assert_eq!(
        lanes.power.battery_sys_cache["/battery_BAT0"].attempted_at,
        Some(Duration::ZERO)
    );
}

#[test]
fn retained_system_battery_failure_obeys_thirty_second_attempt_cadence() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/power_supply/BAT0/capacity", "80\n");
    tree.write("sys/class/power_supply/BAT0/status", "Discharging\n");
    let cfg = cfg_panel(&["battery_sys"]);
    let mut hw = HardwareInventory {
        battery_sys_ids: vec!["/battery_BAT0".to_owned()],
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let _ = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(0),
        false,
    );
    fs::remove_dir_all(tree.sys().join("class/power_supply/BAT0")).expect("remove battery");

    let failed = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(30),
        false,
    );
    let cached = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(31),
        false,
    );

    assert_eq!(failed.battery_sys[0].charge_percent, 80);
    assert_eq!(cached.battery_sys[0].charge_percent, 80);
    assert_eq!(dbus.call_trace().len(), 1);
    assert_eq!(
        lanes.power.battery_sys_cache["/battery_BAT0"].attempted_at,
        Some(Duration::from_secs(30))
    );
}

#[test]
fn grouped_smart_battery_hid_and_nvidia_attempts_use_distinct_snapshots() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    for battery in ["BAT0", "BAT1"] {
        tree.write(
            &format!("sys/class/power_supply/{battery}/capacity"),
            "80\n",
        );
        tree.write(
            &format!("sys/class/power_supply/{battery}/status"),
            "Full\n",
        );
    }
    let mut cfg = cfg_panel(&[
        "disk_smart:pair",
        "battery_sys",
        "battery_mouse",
        "battery_kbd",
        "gpu_nvidia_temp",
    ]);
    cfg.battery.mouse_bolt = Some(1);
    cfg.battery.kbd_bolt = Some(2);
    let mut hw = HardwareInventory {
        battery_sys_ids: vec!["/battery_BAT0".to_owned(), "/battery_BAT1".to_owned()],
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    for (label, drive) in [("a", "/drive/a"), ("b", "/drive/b")] {
        hw.disk_smart_drives.insert(
            label.to_owned(),
            SmartDisk {
                object_path: drive.to_owned(),
                interface: DiskSmartInterface::Nvme,
                rotational: false,
            },
        );
    }
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        "nvidia-smi",
        [
            "--query-gpu=temperature.gpu,utilization.gpu,utilization.memory,fan.speed,utilization.decoder",
            "--format=csv,noheader,nounits",
        ],
        ok_cmd(
            "nvidia-smi",
            &[
                "--query-gpu=temperature.gpu,utilization.gpu,utilization.memory,fan.speed,utilization.decoder",
                "--format=csv,noheader,nounits",
            ],
            "60, 50, 30, 40, 5\n",
        ),
    );
    let mut dbus = FakeDbus::new();
    enqueue_smart(&mut dbus, "/drive/a", "");
    enqueue_smart(&mut dbus, "/drive/b", "");
    let mut bolt = FakeBolt::new(vec![
        Ok(Some(BoltBattery {
            name: "Mouse".to_owned(),
            level: 80,
        })),
        Ok(Some(BoltBattery {
            name: "Keyboard".to_owned(),
            level: 70,
        })),
    ]);
    let mut nvml = FakeNvml::new(vec![Err(NvmlError::Read)]);
    let mut fake_clock = FakeClock::default();
    let mut capture_clock = || fake_clock.tick();

    let readings = run_collect_with_clock(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        Some(&mut bolt),
        &mut capture_clock,
        false,
    );

    let captures = [
        lanes.disk.smart_cache["a"]
            .latest
            .as_ref()
            .expect("SMART a")
            .captured_at,
        lanes.disk.smart_cache["b"]
            .latest
            .as_ref()
            .expect("SMART b")
            .captured_at,
        lanes.power.battery_sys_cache["/battery_BAT0"]
            .sampled_at
            .expect("BAT0"),
        lanes.power.battery_sys_cache["/battery_BAT1"]
            .sampled_at
            .expect("BAT1"),
        lanes.power.battery_mouse_cache.sampled_at.expect("mouse"),
        lanes.power.battery_kbd_cache.sampled_at.expect("keyboard"),
        lanes.nvidia.cache.nvml_attempted_at.expect("NVML"),
        lanes.nvidia.cache.fallback_attempted_at.expect("fallback"),
    ];
    assert!(captures.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(readings.assembled_at.monotonic > captures[captures.len() - 1]);
    assert_eq!(commands.call_trace().len(), 1);
    assert_eq!(bolt.calls, 2);
}

#[test]
fn later_disk_jobs_become_due_at_their_own_decision_boundaries() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/temp", "41000\n");
    tree.write("sys/fan", "1500\n");
    let cfg = isolated_cfg(&["hd_temp", "fan_speed"]);
    let mut hw = HardwareInventory {
        hd_temp_paths: [(String::from("disk"), tree.sys().join("temp"))].into(),
        fan_paths: [(String::from("1"), tree.sys().join("fan"))].into(),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let _ = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(0),
        false,
    );
    tree.write("sys/temp", "42000\n");
    tree.write("sys/fan", "1600\n");
    let mut clock = FakeClock::at(clock(25));

    let readings = run_collect_with_clock(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        &mut || clock.tick(),
        false,
    );

    assert_eq!(readings.hd_temps["disk"], Some(41));
    assert_eq!(readings.fan_speeds["1"], Some(1600));
    assert_eq!(
        lanes.disk.hd_temp_cache["disk"].attempted_at,
        Some(Duration::ZERO)
    );
    assert_eq!(
        lanes.disk.fan_speed_cache["1"].attempted_at,
        Some(Duration::from_secs(31))
    );
}

#[test]
fn later_smart_and_system_battery_jobs_become_due_during_a_pass() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    for battery in ["BAT0", "BAT1"] {
        tree.write(
            &format!("sys/class/power_supply/{battery}/capacity"),
            "80\n",
        );
        tree.write(
            &format!("sys/class/power_supply/{battery}/status"),
            "Full\n",
        );
    }
    let mut smart_cfg = isolated_cfg(&["disk_smart:pair"]);
    smart_cfg.disks.smart_interval = crate::domain::Cadence::from_millis(30_000);
    let mut smart_hw = HardwareInventory::default();
    for (label, drive) in [("a", "/drive/a"), ("b", "/drive/b")] {
        smart_hw.disk_smart_drives.insert(
            label.to_owned(),
            SmartDisk {
                object_path: drive.to_owned(),
                interface: DiskSmartInterface::Nvme,
                rotational: false,
            },
        );
    }
    let mut smart_lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    enqueue_smart(&mut dbus, "/drive/a", "");
    enqueue_smart(&mut dbus, "/drive/b", "");
    let _ = run_collect(
        &mut smart_lanes,
        &mut smart_hw,
        &smart_cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(0),
        false,
    );
    enqueue_smart(&mut dbus, "/drive/b", "");
    let calls_before = dbus.call_trace().len();
    let mut advancing = FakeClock::at(clock(25));
    let _ = run_collect_with_clock(
        &mut smart_lanes,
        &mut smart_hw,
        &smart_cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        &mut || advancing.tick(),
        false,
    );
    assert_eq!(dbus.call_trace().len() - calls_before, 2);
    assert_eq!(
        smart_lanes.disk.smart_cache["a"].attempted_at,
        Some(Duration::ZERO)
    );
    assert_eq!(
        smart_lanes.disk.smart_cache["b"].attempted_at,
        Some(Duration::from_secs(31))
    );

    let battery_cfg = isolated_cfg(&["battery_sys"]);
    let mut battery_hw = HardwareInventory {
        battery_sys_ids: vec![String::from("/battery_BAT0"), String::from("/battery_BAT1")],
        ..HardwareInventory::default()
    };
    let mut battery_lanes = TestOwners::default();
    let _ = run_collect(
        &mut battery_lanes,
        &mut battery_hw,
        &battery_cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(0),
        false,
    );
    tree.write("sys/class/power_supply/BAT0/capacity", "70\n");
    tree.write("sys/class/power_supply/BAT1/capacity", "60\n");
    let mut advancing = FakeClock::at(clock(25));
    let readings = run_collect_with_clock(
        &mut battery_lanes,
        &mut battery_hw,
        &battery_cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        &mut || advancing.tick(),
        false,
    );
    assert_eq!(readings.battery_sys[0].charge_percent, 80);
    assert_eq!(readings.battery_sys[1].charge_percent, 60);
    assert_eq!(
        battery_lanes.power.battery_sys_cache["/battery_BAT0"].attempted_at,
        Some(Duration::ZERO)
    );
    assert_eq!(
        battery_lanes.power.battery_sys_cache["/battery_BAT1"].attempted_at,
        Some(Duration::from_secs(31))
    );
}

#[test]
fn later_hid_and_nvidia_jobs_use_fresh_due_decisions() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut hid_cfg = isolated_cfg(&["battery_mouse", "battery_kbd"]);
    hid_cfg.battery.mouse_bolt = Some(1);
    hid_cfg.battery.kbd_bolt = Some(2);
    let mut hid_hw = HardwareInventory::default();
    let mut hid_lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut bolt = FakeBolt::new(vec![
        Ok(Some(BoltBattery {
            name: String::from("Mouse"),
            level: 80,
        })),
        Ok(Some(BoltBattery {
            name: String::from("Keyboard"),
            level: 70,
        })),
    ]);
    let _ = run_collect(
        &mut hid_lanes,
        &mut hid_hw,
        &hid_cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        Some(&mut bolt),
        clock(0),
        false,
    );
    let mut bolt = FakeBolt::new(vec![Ok(Some(BoltBattery {
        name: String::from("Keyboard"),
        level: 60,
    }))]);
    let mut advancing = FakeClock::at(clock(3595));
    let readings = run_collect_with_clock(
        &mut hid_lanes,
        &mut hid_hw,
        &hid_cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        Some(&mut bolt),
        &mut || advancing.tick(),
        false,
    );
    assert_eq!(
        readings
            .battery_mouse
            .as_ref()
            .map(|value| value.charge_percent),
        Some(80)
    );
    assert_eq!(
        readings
            .battery_kbd
            .as_ref()
            .map(|value| value.charge_percent),
        Some(60)
    );
    assert_eq!(bolt.calls, 1);

    let nvidia_cfg = isolated_cfg(&["gpu_nvidia_temp"]);
    let mut nvidia_hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut nvidia_lanes = TestOwners::default();
    for output in ["60, 50, 30, 40, 5\n", "70, 60, 40, 50, 4\n"] {
        commands.enqueue(
            "nvidia-smi",
            [
                "--query-gpu=temperature.gpu,utilization.gpu,utilization.memory,fan.speed,utilization.decoder",
                "--format=csv,noheader,nounits",
            ],
            ok_cmd("nvidia-smi", &[], output),
        );
    }
    let _ = run_collect(
        &mut nvidia_lanes,
        &mut nvidia_hw,
        &nvidia_cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(0),
        false,
    );
    let calls_before = commands.call_trace().len();
    let mut advancing = FakeClock::default();
    let readings = run_collect_with_clock(
        &mut nvidia_lanes,
        &mut nvidia_hw,
        &nvidia_cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        &mut || advancing.tick(),
        false,
    );
    assert_eq!(commands.call_trace().len() - calls_before, 1);
    assert_eq!(readings.gpu_temp, Some(70));
    assert_eq!(
        nvidia_lanes.nvidia.cache.fallback_attempted_at,
        Some(Duration::from_secs(5))
    );
}

#[test]
fn hid_transport_failure_retains_and_retries() {
    let mut cache = crate::sensors::power::BatteryPeripheralCache {
        charge_percent: Some(80),
        sampled_at: Some(Duration::ZERO),
        ..crate::sensors::power::BatteryPeripheralCache::default()
    };
    let mut bolt = FakeBolt::new(vec![Err(BoundaryError::CommandFailed {
        program: PathBuf::from("bolt"),
        args: Vec::new(),
        detail: "transport".to_owned(),
    })]);

    let reading =
        crate::sensors::power::read_battery_bolt_once(&mut cache, &mut bolt, 1, None, clock(1));

    assert_eq!(reading.expect("retained").charge_percent, 80);
    assert_eq!(cache.sampled_at, Some(Duration::ZERO));
    assert_eq!(cache.bolt_completed_at, None);
}
