use super::*;

#[test]
fn collect_battery_sys_reads_sysfs_path() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/power_supply/BAT0/capacity", "85\n");
    tree.write("sys/class/power_supply/BAT0/status", "Discharging\n");
    tree.write("sys/class/power_supply/BAT0/power_now", "12500000\n");
    let cfg = cfg_panel(&["battery_sys"]);
    let mut hw = HardwareInventory::default();
    hw.battery_sys_ids = vec!["/org/freedesktop/UPower/devices/battery_BAT0".to_owned()];

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
    let bat = &readings.battery_sys[0];
    assert_eq!(bat.charge_percent, 85);
    assert_eq!(bat.rate_watts, 12); // 12.5 W banker's → 12
    assert_eq!(
        bat.state,
        crate::domain::readings::BatteryState::Discharging
    );
    // sysfs succeeded → no D-Bus GetAll.
    assert!(dbus.call_trace().is_empty());
}

#[test]
fn collect_screen_brightness_reads_backlight() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/backlight/intel_backlight/brightness", "300\n");
    tree.write(
        "sys/class/backlight/intel_backlight/max_brightness",
        "1200\n",
    );
    let cfg = cfg_panel(&["screen_brightness"]);
    let mut hw = HardwareInventory::default();

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
    assert_eq!(readings.screen_brightness, Some(25)); // 300*100//1200 = 25
}

// ── collect: external status files ───────────────────────────────────────────

#[test]
fn collect_system_updates_reads_count_file() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("run/updates", "42\n");
    let mut cfg = cfg_panel(&["system_updates"]);
    cfg.system_updates.file = tree.root.join("run/updates").to_string_lossy().into_owned();
    let mut hw = HardwareInventory::default();

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
    assert_eq!(readings.system_updates, Some(42));
}

#[test]
fn external_failure_retains_until_configured_source_replacement() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("run/updates-a", "42\n");
    tree.write("run/updates-b", "invalid\n");
    let mut cfg = cfg_panel(&["system_updates"]);
    cfg.system_updates.file = tree
        .root
        .join("run/updates-a")
        .to_string_lossy()
        .into_owned();
    let mut hw = HardwareInventory::default();
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
    assert_eq!(first.system_updates, Some(42));
    tree.write("run/updates-a", "invalid\n");
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
        clock(1),
        false,
    );
    assert_eq!(failed.system_updates, Some(42));
    assert_eq!(
        lanes
            .external
            .updates
            .latest
            .as_ref()
            .expect("retained external sample")
            .captured_at,
        Duration::ZERO
    );

    cfg.system_updates.file = tree
        .root
        .join("run/updates-b")
        .to_string_lossy()
        .into_owned();
    let replaced = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(2),
        false,
    );
    assert_eq!(replaced.system_updates, None);
    assert!(lanes.external.updates.latest.is_none());
}

#[test]
fn collect_status_files_handle_missing_empty_malformed_unreadable_valid() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("run/updates_empty", "");
    tree.write("run/updates_bad", "not-a-number\n");
    tree.write("run/server_on", "1\n");
    tree.write("run/server_off", "0\n");
    tree.write("run/server_bad", "maybe\n");
    let mut cfg = cfg_panel(&["system_updates", "server_check"]);

    // Missing updates file → None.
    cfg.system_updates.file = tree
        .root
        .join("run/updates_missing")
        .to_string_lossy()
        .into_owned();
    cfg.server_check.file = String::new();
    let readings = run_collect(
        &mut TestOwners::default(),
        &mut HardwareInventory::default(),
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut FakeCommandRunner::new(),
        &mut FakeDbus::new(),
        None,
        None,
        clock(0),
        false,
    );
    assert_eq!(readings.system_updates, None);

    // Empty + malformed updates → None.
    cfg.system_updates.file = tree
        .root
        .join("run/updates_empty")
        .to_string_lossy()
        .into_owned();
    let r = run_collect(
        &mut TestOwners::default(),
        &mut HardwareInventory::default(),
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut FakeCommandRunner::new(),
        &mut FakeDbus::new(),
        None,
        None,
        clock(0),
        false,
    );
    assert_eq!(r.system_updates, None);
    cfg.system_updates.file = tree
        .root
        .join("run/updates_bad")
        .to_string_lossy()
        .into_owned();

    // Server on/off/bad/missing.
    for (file, expected) in [
        ("run/server_on", Some(true)),
        ("run/server_off", Some(false)),
        ("run/server_bad", None),
        ("run/server_missing", None),
    ] {
        cfg.server_check.file = tree.root.join(file).to_string_lossy().into_owned();
        let r = run_collect(
            &mut TestOwners::default(),
            &mut HardwareInventory::default(),
            &cfg,
            &tree.proc(),
            &tree.sys(),
            &mut FakeCommandRunner::new(),
            &mut FakeDbus::new(),
            None,
            None,
            clock(0),
            false,
        );
        assert_eq!(r.server_ok, expected, "{file}");
    }
}

// ── collect: capability-driven zero-call proofs ──────────────────────────────

#[test]
fn collect_unrequested_capability_makes_zero_command_and_dbus_calls() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    // Only cpu_temp (no net, no battery, no nvidia-smi).
    let cfg = cfg_panel(&["cpu_temp"]);
    tree.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
    tree.write("sys/class/hwmon/hwmon0/temp1_input", "50000\n");
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
    assert!(commands.call_trace().is_empty(), "no ip/iw/nvidia-smi");
    assert!(dbus.call_trace().is_empty(), "no UPower/UDisks2");
}

// ── collect: failure isolation ───────────────────────────────────────────────

#[test]
fn collect_failure_in_one_capability_does_not_block_others() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    // cpu_temp present; uptime/loadavg present; net_info command fails.
    tree.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
    tree.write("sys/class/hwmon/hwmon0/temp1_input", "64000\n");
    let cfg = cfg_panel(&["cpu_temp", "uptime", "net_device"]);
    let mut hw = HardwareInventory {
        cpu_temp_path: Some(tree.sys().join("class/hwmon/hwmon0/temp1_input")),
        ..HardwareInventory::default()
    };

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    // ip route get exits non-zero → net_info degrades to absent, no crash.
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        CommandOutput {
            program: Path::new(IP).to_path_buf(),
            args: [
                std::ffi::OsString::from("route"),
                std::ffi::OsString::from("get"),
                std::ffi::OsString::from("8.8.8.8"),
            ]
            .to_vec(),
            status: CommandStatus::Exit(1),
            stdout: Vec::new(),
            stderr: Vec::new(),
            truncation: Default::default(),
        },
    );
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
    // cpu_temp + uptime still populated despite net_info failure.
    assert_eq!(readings.cpu_temp, Some(64));
    assert_eq!(readings.uptime_seconds, Some(12345));
    assert!(readings.net_device.is_none());
}

// ── collect: skip_slow ───────────────────────────────────────────────────────
