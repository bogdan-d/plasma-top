use super::*;

#[test]
fn collect_battery_sys_falls_back_to_upower_when_sysfs_absent() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    // No sysfs power_supply; UPower GetAll provides the reading.
    let cfg = cfg_panel(&["battery_sys"]);
    let mut hw = HardwareInventory::default();
    hw.battery_sys_ids = vec!["/battery_BAT0".to_owned()];

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        "/battery_BAT0",
        "org.freedesktop.DBus.Properties",
        "GetAll",
        getall_reply(
            "/battery_BAT0",
            &[("Percentage", "64"), ("State", "2"), ("EnergyRate", "10.5")],
        ),
    );
    let readings = run_collect(
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
    let bat = &readings.battery_sys[0];
    assert_eq!(bat.charge_percent, 64);
    assert_eq!(bat.rate_watts, 10); // 10.5 banker's → 10
    assert_eq!(
        bat.state,
        crate::domain::readings::BatteryState::Discharging
    );
}

#[test]
fn collect_notification_flags_pull_capabilities_without_items() {
    // No panel item, but cpu_temp notification enabled → CpuTemperature still read.
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
    tree.write("sys/class/hwmon/hwmon0/temp1_input", "72000\n");
    let mut cfg = Config::default(); // empty surfaces
    cfg.notifications.cpu_temp = true;
    let mut hw = discover_hardware(
        &tree.sys(),
        &tree.proc(),
        &cfg,
        &mut FakeDbus::new(),
        &mut FakeCommandRunner::new(),
        2,
    );

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let readings = run_collect(
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
    assert_eq!(readings.cpu_temp, Some(72));
}

#[test]
fn collect_emits_no_duplicate_shared_calls_per_pass() {
    // net_info is shared by net_device/net_ip/wifi_* — must issue ip+iw once.
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.mkdir("sys/class/net/wlan0/wireless");
    let cfg = cfg_panel(&[
        "net_device",
        "net_ip",
        "wifi_ssid",
        "wifi_signal",
        "wifi_ssid_signal",
    ]);
    let mut hw = HardwareInventory::default();

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(IP, &["route", "get", "8.8.8.8"], "dev wlan0 src 1.2.3.4\n"),
    );
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd(
            "iw",
            &["dev", "wlan0", "link"],
            "SSID: H\nsignal: -50 dBm\n",
        ),
    );
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
    // Exactly one ip + one iw, despite five items sharing the net_info capability.
    assert_eq!(commands.call_trace().len(), 2);
}
