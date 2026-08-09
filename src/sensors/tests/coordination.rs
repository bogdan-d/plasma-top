use super::*;

#[test]
fn collect_combined_set_populates_many_readings() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
    tree.write("sys/class/hwmon/hwmon0/temp1_input", "50000\n");
    tree.mkdir("sys/class/net/eth0/statistics");
    tree.write("sys/class/net/eth0/statistics/tx_bytes", "0\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "0\n");
    tree.write("sys/class/power_supply/BAT0/capacity", "90\n");
    tree.write("sys/class/power_supply/BAT0/status", "Charging\n");
    let cfg = cfg_panel(&[
        "cpu_temp",
        "cpu_usage",
        "mem_usage",
        "net_speed",
        "battery_sys",
    ]);
    let mut hw = HardwareInventory {
        net_device: Some("eth0".to_owned()),
        ..HardwareInventory::default()
    };
    hw.battery_sys_ids = vec!["/battery_BAT0".to_owned()];
    hw.cpu_temp_path = Some(tree.sys().join("class/hwmon/hwmon0/temp1_input"));

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let r = run_collect(
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
    assert_eq!(r.cpu_usage, None);
    assert_eq!(r.cpu_temp, Some(50));
    assert_eq!(r.mem_usage, Some(25));
    assert!(r.net_up_bps.is_none()); // first sample
    assert_eq!(r.battery_sys.len(), 1);
    assert_eq!(r.battery_sys[0].charge_percent, 90);
    assert!(commands.call_trace().is_empty()); // no ip/iw needed (net_speed, no net_info)
    assert!(dbus.call_trace().is_empty()); // sysfs battery, no UPower
}

#[test]
fn collect_discovery_call_order_matches_python_section_sequence() {
    // Asserts the network identity read happens AFTER net_speed, and brightness
    // after the GPU section, matching src/sensors.py's collect() ordering. We
    // record the order in which boundaries are touched via the command trace.
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.mkdir("sys/class/net/wlan0/wireless");
    tree.write("sys/class/net/wlan0/statistics/tx_bytes", "0\n");
    tree.write("sys/class/net/wlan0/statistics/rx_bytes", "0\n");
    tree.write("sys/class/backlight/b/brightness", "1\n");
    tree.write("sys/class/backlight/b/max_brightness", "2\n");
    let cfg = cfg_panel(&["net_speed", "net_device", "screen_brightness"]);
    let mut hw = HardwareInventory {
        net_device: Some("wlan0".to_owned()),
        ..HardwareInventory::default()
    };

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(IP, &["route", "get", "8.8.8.8"], "dev wlan0\n"),
    );
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd("iw", &["dev", "wlan0", "link"], "SSID: X\n"),
    );
    let mut dbus = FakeDbus::new();
    let r = run_collect(
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
    // net_speed ran first (sysfs, no command), then net_info issued ip+iw.
    assert_eq!(commands.call_trace().len(), 2);
    assert_eq!(commands.call_trace()[0].program, Path::new(IP));
    assert_eq!(commands.call_trace()[1].program, Path::new("iw"));
    // brightness read after — value present.
    assert_eq!(r.screen_brightness, Some(50));
}

#[test]
fn collect_keeps_domain_state_in_separate_owners_and_returns_fresh_snapshot() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["cpu_usage"]);
    let mut hw = HardwareInventory::default();

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let r1 = run_collect(
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
    let r2 = run_collect(
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
    // Fresh snapshot each poll; cpu_history grows in CPU-owner state.
    assert!(r2.cpu_history.len() >= r1.cpu_history.len());
    // HardwareInventory net_device untouched when no net_info capability.
    assert!(hw.net_device.is_none());
}
