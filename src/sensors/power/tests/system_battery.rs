use super::*;

// ── read_battery_sys ─────────────────────────────────────────────────────

#[test]
fn read_battery_sys_reads_sysfs_first() {
    let tmp = TempTree::new();
    tmp.write("sys/class/power_supply/BAT0/capacity", "85\n");
    tmp.write("sys/class/power_supply/BAT0/status", "Discharging\n");
    tmp.write("sys/class/power_supply/BAT0/power_now", "12500000\n");
    tmp.write(
        "sys/class/power_supply/BAT0/charge_control_end_threshold",
        "80\n",
    );

    let mut state = PowerState::default();
    let mut dbus = FakeDbus::new();

    let readings = read_battery_sys_once(
        &mut state,
        &mut dbus,
        &["/org/freedesktop/UPower/devices/battery_BAT0".to_owned()],
        &tmp.sys(),
        clock(0),
    );

    let battery = readings.first().expect("battery read");
    assert_eq!(battery.id, "/org/freedesktop/UPower/devices/battery_BAT0");
    assert_eq!(battery.charge_percent, 85);
    // 12_500_000 µW → 12.5 W → banker's rounding → 12.
    assert_eq!(battery.rate_watts, 12);
    assert_eq!(battery.state, BatteryState::Discharging);
    assert_eq!(battery.charge_limit_percent, Some(80));
    // No D-Bus calls: sysfs path succeeded.
    assert!(dbus.call_trace().is_empty());
}

#[test]
fn read_battery_sys_falls_back_to_upower_when_sysfs_absent() {
    let tmp = TempTree::new();
    let mut state = PowerState::default();
    let mut dbus = FakeDbus::new();
    let path = "/org/freedesktop/UPower/devices/battery_BAT0";
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        battery_props_reply(
            path,
            &[("Percentage", "90"), ("State", "1"), ("EnergyRate", "15.5")],
        ),
    );

    let readings = read_battery_sys_once(
        &mut state,
        &mut dbus,
        &[path.to_owned()],
        &tmp.sys(),
        clock(0),
    );

    let battery = readings.first().expect("fallback battery");
    assert_eq!(battery.charge_percent, 90);
    assert_eq!(battery.state, BatteryState::Charging);
    // 15.5 → banker's rounding → 16.
    assert_eq!(battery.rate_watts, 16);
    let request = dbus.call_trace().first().expect("GetAll request");
    assert_eq!(request.interface, "org.freedesktop.DBus.Properties");
    assert_eq!(request.member, "GetAll");
    assert_eq!(
        request.arguments,
        [DbusArgument::String(UPOWER_DEV_IFACE.to_owned())]
    );
}

#[test]
fn read_battery_sys_upower_zero_rate_falls_back_to_sysfs_power_now() {
    let tmp = TempTree::new();
    // Sysfs has no capacity (so the sysfs primary path fails and we drop to
    // UPower), but power_now IS readable for the rate fallback.
    tmp.write("sys/class/power_supply/BAT0/power_now", "5000000\n");
    let mut state = PowerState::default();
    let mut dbus = FakeDbus::new();
    let path = "/org/freedesktop/UPower/devices/battery_BAT0";
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        battery_props_reply(
            path,
            &[("Percentage", "70"), ("State", "2"), ("EnergyRate", "0")],
        ),
    );

    let readings = read_battery_sys_once(
        &mut state,
        &mut dbus,
        &[path.to_owned()],
        &tmp.sys(),
        clock(0),
    );

    let battery = readings.first().expect("battery");
    // EnergyRate 0 + discharging → fallback to sysfs 5_000_000 µW = 5 W.
    assert_eq!(battery.rate_watts, 5);
}

#[test]
fn read_battery_sys_skips_batteries_without_percentage() {
    let tmp = TempTree::new();
    let mut state = PowerState::default();
    let mut dbus = FakeDbus::new();
    let path = "/org/freedesktop/UPower/devices/battery_BAT0";
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        battery_props_reply(path, &[("State", "2")]),
    );

    let readings = read_battery_sys_once(
        &mut state,
        &mut dbus,
        &[path.to_owned()],
        &tmp.sys(),
        clock(0),
    );

    assert!(readings.is_empty(), "no percentage → no row");
}

#[test]
fn failed_battery_attempt_retains_latest_reading() {
    let tmp = TempTree::new();
    tmp.write("sys/class/power_supply/BAT0/capacity", "50\n");
    tmp.write("sys/class/power_supply/BAT0/status", "Charging\n");

    let mut state = PowerState::default();
    let mut dbus = FakeDbus::new();

    let _ = read_battery_sys_once(
        &mut state,
        &mut dbus,
        &["/org/freedesktop/UPower/devices/battery_BAT0".to_owned()],
        &tmp.sys(),
        clock(0),
    );

    // Remove sysfs; failed fallback must not discard latest valid reading.
    let _ = fs::remove_file(tmp.sys().join("class/power_supply/BAT0/capacity"));

    let readings = read_battery_sys_once(
        &mut state,
        &mut dbus,
        &["/org/freedesktop/UPower/devices/battery_BAT0".to_owned()],
        &tmp.sys(),
        clock(10),
    );

    assert_eq!(readings.first().expect("cached").charge_percent, 50);
    assert_eq!(
        state.battery_sys_cache["/org/freedesktop/UPower/devices/battery_BAT0"].failed_at,
        Some(Duration::from_secs(10))
    );
}

#[test]
fn read_battery_sys_charge_limit_100_treated_as_unset() {
    let tmp = TempTree::new();
    tmp.write("sys/class/power_supply/BAT0/capacity", "99\n");
    tmp.write("sys/class/power_supply/BAT0/status", "Full\n");
    tmp.write(
        "sys/class/power_supply/BAT0/charge_control_end_threshold",
        "100\n",
    );

    let mut state = PowerState::default();
    let mut dbus = FakeDbus::new();

    let readings = read_battery_sys_once(
        &mut state,
        &mut dbus,
        &["/org/freedesktop/UPower/devices/battery_BAT0".to_owned()],
        &tmp.sys(),
        clock(0),
    );

    assert_eq!(
        readings.first().expect("battery").charge_limit_percent,
        None
    );
}

#[test]
fn cached_system_battery_result_keeps_latest_failure_timestamp() {
    let cache = BatterySystemCache {
        charge_percent: Some(50),
        sampled_at: Some(Duration::from_secs(1)),
        attempted_at: Some(Duration::from_secs(2)),
        failed_at: Some(Duration::from_secs(2)),
        ..BatterySystemCache::default()
    };

    let result = crate::sensors::cached_system_battery("battery_BAT0", Some(&cache));

    assert_eq!(result.reading.status, crate::sensors::AttemptStatus::Cached);
    assert_eq!(result.reading.failed_at, Some(Duration::from_secs(2)));
    assert_eq!(
        result.reading.sample.map(|sample| sample.captured_at),
        Some(Duration::from_secs(1))
    );
}
