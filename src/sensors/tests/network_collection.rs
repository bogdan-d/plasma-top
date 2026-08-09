use super::*;

#[test]
fn collect_net_speed_needs_device_and_two_samples() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/net/eth0/statistics/tx_bytes", "1000\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "2000\n");
    let cfg = cfg_panel(&["net_speed"]);
    let mut hw = HardwareInventory {
        net_device: Some("eth0".to_owned()),
        ..HardwareInventory::default()
    };

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    // First sample → None (no prev).
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
    assert!(r1.net_up_bps.is_none());

    tree.write("sys/class/net/eth0/statistics/tx_bytes", "3000\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "6000\n");
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
    // 2000 tx bytes / 2 s = 1000 B/s; 4000 rx / 2s = 2000 B/s.
    assert_eq!(r2.net_up_bps, Some(1000));
    assert_eq!(r2.net_down_bps, Some(2000));
}

#[test]
fn network_noncomparable_attempt_retains_rate_without_history_or_failure() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/net/eth0/statistics/tx_bytes", "1000\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "2000\n");
    let mut cfg = cfg_panel(&["net_speed"]);
    cfg.pages.order = vec![String::from("graphs")];
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    let mut hw = HardwareInventory {
        net_device: Some(String::from("eth0")),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();

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
        clock(0),
        false,
    );
    assert!(baseline.net_up_bps.is_none());

    tree.write("sys/class/net/eth0/statistics/tx_bytes", "3000\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "6000\n");
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
    assert_eq!(
        (valid.net_up_bps, valid.net_down_bps),
        (Some(1000), Some(2000))
    );
    let history_len = valid.net_up_history.len();

    tree.write("sys/class/net/eth0/statistics/tx_bytes", "20\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "30\n");
    let rollback = run_collect(
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

    assert_eq!(rollback.net_up_bps, valid.net_up_bps);
    assert_eq!(rollback.net_down_bps, valid.net_down_bps);
    assert_eq!(rollback.net_up_history.len(), history_len);
    assert_eq!(rollback.net_down_history.len(), history_len);
    assert_eq!(
        lanes.network.rate.attempted_at,
        Some(Duration::from_secs(3))
    );
    assert_eq!(lanes.network.rate.failed_at, None);

    tree.write("sys/class/net/eth0/statistics/tx_bytes", "40\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "50\n");
    let zero_elapsed =
        attempt_network_speed(&mut lanes.network, &tree.sys(), "eth0", clock(3), &mut None);
    assert_eq!(zero_elapsed.reading.status, AttemptStatus::Baseline);
    assert_eq!(
        zero_elapsed.reading.sample.map(|sample| sample.value),
        Some((1000, 2000))
    );

    tree.write("sys/class/net/eth0/statistics/tx_bytes", "80\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "110\n");
    let recovered =
        attempt_network_speed(&mut lanes.network, &tree.sys(), "eth0", clock(5), &mut None);
    assert_eq!(recovered.reading.status, AttemptStatus::Captured);
    assert_eq!(
        recovered.reading.sample.map(|sample| sample.value),
        Some((30, 40)),
        "the zero-elapsed read must not consume the later byte delta"
    );
}

#[test]
fn collect_net_info_reads_route_and_wifi_via_commands() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.mkdir("sys/class/net/wlan0/wireless");
    let cfg = cfg_panel(&["wifi_ssid_signal"]);
    let mut hw = HardwareInventory::default();

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev wlan0 src 10.0.0.5\n",
        ),
    );
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd(
            "iw",
            &["dev", "wlan0", "link"],
            "SSID: Home\n\tsignal: -60 dBm\n",
        ),
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
    assert_eq!(readings.net_device.as_deref(), Some("wlan0"));
    assert_eq!(readings.ip_address.as_deref(), Some("10.0.0.5"));
    assert_eq!(readings.wifi_ssid.as_deref(), Some("Home"));
    assert_eq!(readings.wifi_signal_percent, Some(80));
    assert_eq!(commands.call_trace().len(), 2);
    assert_eq!(commands.call_trace()[0].timeout, NETWORK_COMMAND_TIMEOUT);
}

#[test]
fn route_and_wifi_capture_times_follow_their_own_source_calls() {
    let tree = TempTree::new();
    tree.mkdir("sys/class/net/wlan0/wireless");
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev wlan0 src 10.0.0.5\n",
        ),
    );
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd("iw", &["dev", "wlan0", "link"], "SSID: Home\n"),
    );
    let mut captures = VecDeque::from([clock(10), clock(25)]);
    let mut capture_clock = || captures.pop_front().expect("source capture time");
    let mut state = crate::sensors::network::NetworkState::default();

    let result = attempt_network_info(
        &mut state,
        &tree.sys(),
        &mut commands,
        &mut capture_clock,
        &mut None,
    );

    assert_eq!(result.reading.status, AttemptStatus::Captured);
    assert_eq!(
        result.reading.sample.map(|sample| sample.captured_at),
        Some(Duration::from_secs(10))
    );
    assert_eq!(
        result.wifi.sample.map(|sample| sample.captured_at),
        Some(Duration::from_secs(25)),
        "a slow route source must not backdate the later Wi-Fi sample"
    );
    assert!(captures.is_empty());
}

#[test]
fn collect_net_info_adopts_new_device_into_hardware_inventory() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.mkdir("sys/class/net/wlan0/wireless");
    let cfg = cfg_panel(&["net_device"]);
    let mut hw = HardwareInventory::default(); // net_device None

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev wlan0 src 10.0.0.5\n",
        ),
    );
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd("iw", &["dev", "wlan0", "link"], "SSID: H\n"),
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
    // hw.net_device adopts the live route device.
    assert_eq!(hw.net_device.as_deref(), Some("wlan0"));
}

#[test]
fn same_wireless_device_iw_failure_retains_wifi_and_records_failure() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.mkdir("sys/class/net/wlan0/wireless");
    let cfg = cfg_panel(&["wifi_ssid_signal"]);
    let mut hw = HardwareInventory {
        net_device: Some(String::from("wlan0")),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev wlan0 src 10.0.0.1\n",
        ),
    );
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd(
            "iw",
            &["dev", "wlan0", "link"],
            "SSID: Home\n\tsignal: -60 dBm\n",
        ),
    );
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev wlan0 src 10.0.0.2\n",
        ),
    );
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
        clock(10),
        false,
    );

    assert_eq!(failed.net_device, first.net_device);
    assert_eq!(failed.ip_address.as_deref(), Some("10.0.0.2"));
    assert_eq!(failed.wifi_ssid, first.wifi_ssid);
    assert_eq!(failed.wifi_signal_percent, first.wifi_signal_percent);
    assert_eq!(lanes.network.wifi.failed_at, Some(Duration::from_secs(10)));
    assert_eq!(
        lanes
            .network
            .wifi
            .latest
            .as_ref()
            .map(|sample| sample.captured_at),
        Some(Duration::ZERO)
    );
    assert_eq!(lanes.network.info.failed_at, None);
    assert_eq!(
        lanes
            .network
            .info
            .latest
            .as_ref()
            .map(|sample| sample.captured_at),
        Some(Duration::from_secs(10))
    );
    assert_eq!(commands.call_trace().len(), 4);
}

#[test]
fn malformed_successful_route_retains_previous_wireless_identity_and_inventory() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.mkdir("sys/class/net/wlan0/wireless");
    let cfg = cfg_panel(&["wifi_ssid_signal"]);
    let mut hw = HardwareInventory {
        net_device: Some(String::from("wlan0")),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev wlan0 src 10.0.0.1\n",
        ),
    );
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd(
            "iw",
            &["dev", "wlan0", "link"],
            "SSID: Home\n\tsignal: -60 dBm\n",
        ),
    );
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 via 10.0.0.254 src 10.0.0.2\n",
        ),
    );
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
        clock(10),
        false,
    );

    assert_eq!(retained.net_device, first.net_device);
    assert_eq!(retained.ip_address, first.ip_address);
    assert_eq!(retained.wifi_ssid, first.wifi_ssid);
    assert_eq!(retained.wifi_signal_percent, first.wifi_signal_percent);
    assert_eq!(hw.net_device.as_deref(), Some("wlan0"));
    assert_eq!(lanes.network.info.failed_at, Some(Duration::from_secs(10)));
    assert_eq!(lanes.network.wifi.failed_at, None);
    assert_eq!(commands.call_trace().len(), 3, "failed route must skip iw");
}

#[test]
fn same_wireless_device_sysfs_error_retains_wifi_until_confirmed_absent() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.mkdir("sys/class/net/wlan0/wireless");
    let cfg = cfg_panel(&["wifi_ssid_signal"]);
    let mut hw = HardwareInventory {
        net_device: Some(String::from("wlan0")),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    for ip in ["10.0.0.1", "10.0.0.2", "10.0.0.3"] {
        commands.enqueue(
            IP,
            ["route", "get", "8.8.8.8"],
            ok_cmd(
                IP,
                &["route", "get", "8.8.8.8"],
                &format!("8.8.8.8 dev wlan0 src {ip}\n"),
            ),
        );
    }
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd(
            "iw",
            &["dev", "wlan0", "link"],
            "SSID: Home\n\tsignal: -60 dBm\n",
        ),
    );
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
    fs::remove_dir(tree.root.join("sys/class/net/wlan0/wireless")).expect("remove wireless dir");
    tree.symlink("wireless", "sys/class/net/wlan0/wireless");

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
        clock(10),
        false,
    );

    assert_eq!(failed.ip_address.as_deref(), Some("10.0.0.2"));
    assert_eq!(failed.wifi_ssid, first.wifi_ssid);
    assert_eq!(failed.wifi_signal_percent, first.wifi_signal_percent);
    assert_eq!(lanes.network.wifi.failed_at, Some(Duration::from_secs(10)));
    assert!(lanes.network.wifi.latest_attempt_failed);
    assert_eq!(commands.call_trace().len(), 3, "sysfs failure must skip iw");

    fs::remove_file(tree.root.join("sys/class/net/wlan0/wireless"))
        .expect("remove wireless symlink");
    let absent = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(20),
        false,
    );

    assert_eq!(absent.ip_address.as_deref(), Some("10.0.0.3"));
    assert_eq!(absent.wifi_ssid, None);
    assert_eq!(absent.wifi_signal_percent, None);
    assert!(lanes.network.wifi.latest.is_none());
    assert!(!lanes.network.wifi.latest_attempt_failed);
    assert_eq!(commands.call_trace().len(), 4);
}

#[test]
fn refreshed_route_change_commits_partial_info_when_iw_fails() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.mkdir("sys/class/net/wlan0/wireless");
    tree.mkdir("sys/class/net/wlan1/wireless");
    tree.write("sys/class/net/wlan0/statistics/tx_bytes", "1000\n");
    tree.write("sys/class/net/wlan0/statistics/rx_bytes", "2000\n");
    let cfg = cfg_panel(&["net_speed", "net_device"]);
    let mut hw = HardwareInventory {
        net_device: Some(String::from("wlan0")),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev wlan0 src 10.0.0.1\n",
        ),
    );
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd("iw", &["dev", "wlan0", "link"], "SSID: OldWifi\n"),
    );
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev wlan1 src 10.0.0.2\n",
        ),
    );
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
    assert_eq!(first.net_device.as_deref(), Some("wlan0"));
    assert_eq!(first.wifi_ssid.as_deref(), Some("OldWifi"));
    tree.write("sys/class/net/wlan0/statistics/tx_bytes", "11000\n");
    tree.write("sys/class/net/wlan0/statistics/rx_bytes", "22000\n");

    let changed = run_collect(
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

    assert_eq!(changed.net_device.as_deref(), Some("wlan1"));
    assert_eq!(changed.ip_address.as_deref(), Some("10.0.0.2"));
    assert_eq!(changed.wifi_ssid, None);
    assert_eq!(changed.wifi_signal_percent, None);
    assert_eq!(changed.net_up_bps, None);
    assert_eq!(changed.net_down_bps, None);
    assert_eq!(hw.net_device.as_deref(), Some("wlan1"));
    assert_eq!(lanes.network.info_source.as_deref(), Some("wlan1"));
    assert_eq!(lanes.network.wifi.failed_at, Some(Duration::from_secs(10)));
    assert!(lanes.network.wifi.latest.is_none());
    assert_eq!(
        lanes
            .network
            .info
            .latest
            .as_ref()
            .map(|sample| sample.captured_at),
        Some(Duration::from_secs(10))
    );
    assert!(lanes.network.rate.latest.is_none());
    assert_eq!(lanes.network.rate_device, None);
    assert_eq!(commands.call_trace().len(), 4);
    assert_eq!(commands.call_trace()[3].program, Path::new("iw"));
}

#[test]
fn network_info_disable_reenable_invalidates_ttl_cache() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["net_device"]);
    let mut disabled = Config::default();
    disabled.panel.sections.clear();
    disabled.tooltip.sections.clear();
    disabled.pages.order.clear();
    let mut hw = HardwareInventory::default();
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev eth0 src 10.0.0.1\n",
        ),
    );
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
    assert_eq!(first.ip_address.as_deref(), Some("10.0.0.1"));

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
        clock(10),
        false,
    );
    assert_eq!(failed.ip_address, first.ip_address);
    assert_eq!(
        lanes
            .network
            .info
            .latest
            .as_ref()
            .expect("retained network info")
            .captured_at,
        Duration::ZERO
    );

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
        clock(11),
        false,
    );
    assert!(lanes.network.info.latest.is_none());

    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev eth0 src 10.0.0.2\n",
        ),
    );
    let refreshed = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(12),
        false,
    );
    assert_eq!(refreshed.ip_address.as_deref(), Some("10.0.0.2"));
    assert_eq!(commands.call_trace().len(), 3);
}
