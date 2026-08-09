use super::*;

#[test]
fn collect_disk_io_reads_rate_after_two_samples() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 8 0 0 0 4 0 0 0 0 0 0 0 0 0\n",
    );
    let cfg = cfg_panel(&["disk_io"]);
    let mut hw = HardwareInventory {
        disk_io_device: Some("nvme0n1".to_owned()),
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
    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 24 0 0 0 20 0 0 0 0 0 0 0 0 0\n",
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
        clock(2),
        false,
    );
    // 16 read sectors * 512 = 8192 B / 2s = 4096 B/s.
    assert_eq!(r2.disk_read_bps, Some(4096));
    assert_eq!(r2.disk_write_bps, Some(4096));
}

#[test]
fn disk_io_noncomparable_attempts_retain_rate_without_losing_counters() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 8 0 0 0 4 0 0 0 0 0 0 0 0 0\n",
    );
    let mut state = disk::DiskState::default();
    let mut timings = None;

    let baseline = attempt_disk_io(&mut state, &tree.proc(), "nvme0n1", clock(0), &mut timings);
    assert_eq!(baseline.reading.status, AttemptStatus::Baseline);
    assert!(baseline.reading.sample.is_none());

    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 24 0 0 0 20 0 0 0 0 0 0 0 0 0\n",
    );
    let valid = attempt_disk_io(&mut state, &tree.proc(), "nvme0n1", clock(2), &mut timings);
    let valid_sample = valid.reading.sample.expect("valid rate");

    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 2 0 0 0 3 0 0 0 0 0 0 0 0 0\n",
    );
    let rollback = attempt_disk_io(&mut state, &tree.proc(), "nvme0n1", clock(3), &mut timings);
    assert_eq!(rollback.reading.status, AttemptStatus::Baseline);
    assert_eq!(rollback.reading.sample, Some(valid_sample.clone()));
    assert_eq!(rollback.reading.attempted_at, Some(Duration::from_secs(3)));
    assert_eq!(rollback.reading.failed_at, None);

    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 4 0 0 0 5 0 0 0 0 0 0 0 0 0\n",
    );
    let zero_elapsed = attempt_disk_io(&mut state, &tree.proc(), "nvme0n1", clock(3), &mut timings);
    assert_eq!(zero_elapsed.reading.status, AttemptStatus::Baseline);
    assert_eq!(zero_elapsed.reading.sample, Some(valid_sample.clone()));
    assert_eq!(state.io.failed_at, None);

    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 8 0 0 0 9 0 0 0 0 0 0 0 0 0\n",
    );
    let recovered = attempt_disk_io(&mut state, &tree.proc(), "nvme0n1", clock(5), &mut timings);
    assert_eq!(recovered.reading.status, AttemptStatus::Captured);
    assert_eq!(
        recovered.reading.sample.map(|sample| sample.value),
        Some((1536, 1536)),
        "the zero-elapsed read must not consume the later byte delta"
    );
}

#[test]
fn disabling_and_reenabling_rate_jobs_resets_counter_baselines() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/net/eth0/statistics/tx_bytes", "1000\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "2000\n");
    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 8 0 0 0 4 0 0 0 0 0 0 0 0 0\n",
    );
    let cfg = cfg_panel(&["net_speed", "disk_io"]);
    let mut disabled = Config::default();
    disabled.panel.sections.clear();
    disabled.tooltip.sections.clear();
    disabled.pages.order.clear();
    let mut hw = HardwareInventory {
        net_device: Some(String::from("eth0")),
        disk_io_device: Some(String::from("nvme0n1")),
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
    tree.write("sys/class/net/eth0/statistics/tx_bytes", "3000\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "6000\n");
    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 24 0 0 0 20 0 0 0 0 0 0 0 0 0\n",
    );
    let valid = run_collect(
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
    assert!(valid.net_up_bps.is_some());
    assert!(valid.disk_read_bps.is_some());

    let _ = run_collect(
        &mut lanes,
        &mut hw,
        &disabled,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(3),
        false,
    );
    assert!(lanes.network.rate.latest.is_none());
    assert!(lanes.disk.io.latest.is_none());

    tree.write("sys/class/net/eth0/statistics/tx_bytes", "5000\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "9000\n");
    tree.write(
        "proc/diskstats",
        "259 0 nvme0n1 0 0 40 0 0 0 36 0 0 0 0 0 0 0 0 0\n",
    );
    let baseline = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(4),
        false,
    );
    assert!(baseline.net_up_bps.is_none());
    assert!(baseline.disk_read_bps.is_none());
}

#[test]
fn collect_disk_usage_inserts_per_mount_results() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["disk_usage"]);
    // Explicit list: one real temp mount, one missing.
    let real = tree.root.join("mnt/data");
    fs::create_dir_all(&real).expect("mount");
    let mut cfg = cfg;
    cfg.disks.mounts = Mounts::Explicit(vec![
        "/missing/path".to_owned(),
        real.to_string_lossy().into_owned(),
    ]);

    let mut lanes = TestOwners::default();
    let mut hw = HardwareInventory::default();
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
    assert_eq!(
        readings.disk_usage.get("/missing/path").copied().flatten(),
        None
    );
    assert!(
        readings
            .disk_usage
            .contains_key(real.to_string_lossy().as_ref())
    );
    let real_key = real.to_string_lossy().into_owned();
    let first_real = readings.disk_usage[&real_key];
    fs::remove_dir_all(&real).expect("remove mount");
    let retained = run_collect(
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
    assert_eq!(retained.disk_usage[&real_key], first_real);
    assert_eq!(
        lanes.disk.usage[&real_key]
            .latest
            .as_ref()
            .expect("retained mount sample")
            .captured_at,
        Duration::ZERO
    );
}

#[test]
fn collect_hd_temp_and_fan_read_with_cache() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.mkdir("sys/class/hwmon/hwmon1");
    tree.write("sys/class/hwmon/hwmon1/name", "nvme\n");
    tree.write("sys/class/hwmon/hwmon1/temp1_input", "41000\n");
    tree.mkdir("sys/class/hwmon/hwmon2");
    tree.write("sys/class/hwmon/hwmon2/name", "nct\n");
    tree.write("sys/class/hwmon/hwmon2/fan1_input", "1500\n");
    let cfg = cfg_panel(&["hd_temp", "fan_speed"]);
    // Use overrides to bind the paths to specific labels deterministically.
    let mut cfg = cfg;
    cfg.sensors.hd1_temp = Some("nvme|temp1_input".to_owned());
    cfg.sensors.fan1_speed = Some("nct|fan1_input".to_owned());
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
    let label = r1.hd_temps.keys().next().expect("hd_temp label");
    assert_eq!(r1.hd_temps[label], Some(41));
    assert_eq!(r1.fan_speeds["1"], Some(1500));

    // Change files; within 30s TTL the cached value persists.
    tree.write("sys/class/hwmon/hwmon1/temp1_input", "45000\n");
    tree.write("sys/class/hwmon/hwmon2/fan1_input", "1700\n");
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
        clock(10),
        false,
    );
    assert_eq!(r2.hd_temps[label], Some(41));
    assert_eq!(r2.fan_speeds["1"], Some(1500));
    assert_eq!(
        lanes.disk.hd_temp_cache[label]
            .latest
            .as_ref()
            .expect("cached temperature")
            .captured_at,
        Duration::ZERO,
    );
    assert_eq!(
        lanes.disk.hd_temp_cache[label].attempted_at,
        Some(Duration::ZERO),
    );
    // After TTL: refresh.
    let r3 = run_collect(
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
    assert_eq!(r3.hd_temps[label], Some(45));
    assert_eq!(r3.fan_speeds["1"], Some(1700));

    // A failed due attempt advances retry cadence but retains the latest valid sample.
    tree.write("sys/class/hwmon/hwmon1/temp1_input", "invalid\n");
    tree.write("sys/class/hwmon/hwmon2/fan1_input", "invalid\n");
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
        clock(62),
        false,
    );
    assert_eq!(failed.hd_temps[label], Some(45));
    assert_eq!(failed.fan_speeds["1"], Some(1700));
    let temperature = lanes.disk.hd_temp_cache.get(label).expect("temp state");
    assert_eq!(
        temperature.latest.as_ref().expect("valid temp").captured_at,
        Duration::from_secs(31)
    );
    assert_eq!(temperature.attempted_at, Some(Duration::from_secs(62)));
    let fan = lanes.disk.fan_speed_cache.get("1").expect("fan state");
    assert_eq!(
        fan.latest.as_ref().expect("valid fan").captured_at,
        Duration::from_secs(31)
    );
    assert_eq!(fan.attempted_at, Some(Duration::from_secs(62)));
}

#[test]
fn failed_then_cached_temperature_is_retained_for_display_not_notifications() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let temp_path = tree.sys().join("temp");
    tree.write("sys/temp", "85000\n");
    let mut cfg = cfg_panel(&["hd_temp"]);
    cfg.notifications.hd_temp = true;
    let mut hw = HardwareInventory {
        hd_temp_paths: [(String::from("disk"), temp_path.clone())].into(),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();

    let captured = run_collect_output(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        clock(0),
    );
    assert_eq!(captured.notifications.hd_temps["disk"], Some(85));

    fs::remove_file(temp_path).expect("remove temperature source");
    let failed = run_collect_output(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        clock(30),
    );
    assert_eq!(failed.display.hd_temps["disk"], Some(85));
    assert_eq!(failed.notifications.hd_temps["disk"], None);

    let cached = run_collect_output(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        clock(31),
    );
    assert_eq!(cached.display.hd_temps["disk"], Some(85));
    assert_eq!(cached.notifications.hd_temps["disk"], None);
}

#[test]
fn disk_owner_attempt_is_usable_and_inventory_changes_invalidate_samples() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/temp-a", "41000\n");
    tree.write("sys/temp-b", "52000\n");
    let path_a = tree.sys().join("temp-a");
    let path_b = tree.sys().join("temp-b");

    let mut direct_state = disk::DiskState::default();
    let direct =
        attempt_disk_temperature(&mut direct_state, "disk", &path_a, Duration::from_secs(7));
    assert_eq!(direct.label, "disk");
    assert_eq!(direct.reading.attempted_at, Some(Duration::from_secs(7)));
    assert_eq!(direct.reading.sample.expect("typed owner result").value, 41);

    let cfg = cfg_panel(&["hd_temp"]);
    let mut hw = HardwareInventory {
        hd_temp_paths: [(String::from("disk"), path_a)].into(),
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
    assert_eq!(first.hd_temps["disk"], Some(41));

    hw.hd_temp_paths.insert(String::from("disk"), path_b);
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
        clock(1),
        false,
    );
    assert_eq!(replaced.hd_temps["disk"], Some(52));
    assert_eq!(
        lanes.disk.hd_temp_cache["disk"]
            .latest
            .as_ref()
            .expect("replacement sample")
            .captured_at,
        Duration::from_secs(1),
    );

    let mut disabled_cfg = cfg_panel(&["cpu_usage"]);
    disabled_cfg.tooltip.sections.clear();
    disabled_cfg.notifications.hd_temp = false;
    let disabled = run_collect(
        &mut lanes,
        &mut hw,
        &disabled_cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(2),
        false,
    );
    assert!(disabled.hd_temps.is_empty());
    assert!(!lanes.disk.hd_temp_cache.contains_key("disk"));

    let restored = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(3),
        false,
    );
    assert_eq!(restored.hd_temps["disk"], Some(52));

    hw.hd_temp_paths.clear();
    let removed = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(4),
        false,
    );
    assert!(removed.hd_temps.is_empty());
    assert!(!lanes.disk.hd_temp_cache.contains_key("disk"));
}
