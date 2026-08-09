use super::*;

use crate::sensors::gpu_nvidia::{NvmlMetrics, NvmlOptionalMetric};

#[test]
fn collect_cpu_and_mem_histories_accumulate_at_cadence() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut cfg = cfg_panel(&["cpu_usage", "mem_usage"]);
    cfg.display.history_interval = crate::domain::Cadence::from_millis(2000);
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
    assert!(r1.cpu_history.is_empty());
    assert_eq!(r1.mem_history.len(), 1);
    tree.write(
        "proc/stat",
        "cpu  30 0 20 100 0 0 0 0 0 0\n\
         cpu0 15 0 10 50 0 0 0 0 0 0\n\
         cpu1 15 0 10 50 0 0 0 0 0 0\n",
    );
    // The first comparable delta creates the first real history point.
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
    assert_eq!(r2.cpu_history.len(), 1);
    tree.write(
        "proc/stat",
        "cpu  50 0 30 120 0 0 0 0 0 0\n\
         cpu0 25 0 15 60 0 0 0 0 0 0\n\
         cpu1 25 0 15 60 0 0 0 0 0 0\n",
    );
    // t=3: cadence elapsed → new sample appended.
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
        clock(3),
        false,
    );
    assert_eq!(r3.cpu_history.len(), 2);
}

#[test]
fn histories_wait_for_first_sample_then_append_one_carried_point_after_failure() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("proc/stat", "malformed\n");
    tree.write("proc/meminfo", "malformed\n");
    let mut cfg = cfg_panel(&["cpu_usage", "mem_usage"]);
    cfg.pages.order = vec![String::from("cpu_cores")];
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    let mut hw = HardwareInventory::default();
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();

    let missing = run_collect(
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
    let still_missing = run_collect(
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
    assert!(missing.cpu_history.is_empty());
    assert!(missing.mem_history.is_empty());
    assert!(missing.cpu_core_history.is_none());
    assert!(still_missing.cpu_history.is_empty());
    assert!(still_missing.mem_history.is_empty());
    assert!(still_missing.cpu_core_history.is_none());

    baseline_proc(&tree);
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
        clock(11),
        false,
    );
    assert!(baseline.cpu_history.is_empty());
    assert!(baseline.cpu_core_history.is_none());
    tree.write(
        "proc/stat",
        "cpu  30 0 20 100 0 0 0 0 0 0\n\
         cpu0 15 0 10 50 0 0 0 0 0 0\n\
         cpu1 15 0 10 50 0 0 0 0 0 0\n",
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
        clock(11),
        false,
    );
    tree.write("proc/stat", "malformed\n");
    tree.write("proc/meminfo", "malformed\n");
    let carried = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(100),
        false,
    );

    assert_eq!(valid.cpu_history.len(), 1);
    assert_eq!(valid.mem_history.len(), 1);
    assert!(
        valid
            .cpu_core_history
            .as_ref()
            .is_some_and(|histories| { histories.iter().all(|history| history.len() == 1) })
    );
    assert_eq!(carried.cpu_history.len(), 2);
    assert_eq!(carried.mem_history.len(), 2);
    assert_eq!(carried.cpu_history.last(), valid.cpu_history.last());
    assert_eq!(carried.mem_history.last(), valid.mem_history.last());
    assert!(
        carried
            .cpu_core_history
            .as_ref()
            .is_some_and(|histories| { histories.iter().all(|history| history.len() == 2) })
    );
}

#[test]
fn network_history_appends_retained_rate_after_failed_due_attempt() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/net/eth0/statistics/tx_bytes", "0\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "0\n");
    let mut cfg = cfg_panel(&["net_speed"]);
    cfg.pages.order = vec![String::from("graphs")];
    cfg.pages.graph_history_length = 10;
    cfg.display.history_interval = crate::domain::Cadence::from_millis(2000);
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
    assert!(baseline.net_up_history.is_empty());

    tree.write("sys/class/net/eth0/statistics/tx_bytes", "100\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "200\n");
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
        clock(1),
        false,
    );
    fs::remove_file(tree.sys().join("class/net/eth0/statistics/tx_bytes"))
        .expect("remove network counter");
    let carried = run_collect(
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

    assert_eq!(valid.net_up_history.len(), 1);
    assert_eq!(carried.net_up_bps, valid.net_up_bps);
    assert_eq!(carried.net_down_bps, valid.net_down_bps);
    assert_eq!(carried.net_up_history, vec![100, 100]);
    assert_eq!(carried.net_down_history, vec![200, 200]);
}

#[test]
fn route_change_does_not_append_old_source_rate_to_network_history() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/net/eth0/statistics/tx_bytes", "0\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "0\n");
    tree.write("sys/class/net/wlan0/statistics/tx_bytes", "1000\n");
    tree.write("sys/class/net/wlan0/statistics/rx_bytes", "2000\n");
    let mut cfg = cfg_panel(&["net_speed", "net_device"]);
    cfg.pages.order = vec![String::from("graphs")];
    cfg.pages.graph_history_length = 10;
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    let mut hw = HardwareInventory {
        net_device: Some(String::from("eth0")),
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
            "8.8.8.8 dev eth0 src 10.0.0.1\n",
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
    tree.write("sys/class/net/eth0/statistics/tx_bytes", "100\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "200\n");
    let established = run_collect(
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
    assert_eq!(established.net_up_history, vec![100]);
    assert_eq!(established.net_down_history, vec![200]);

    tree.write("sys/class/net/eth0/statistics/tx_bytes", "10000\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "20000\n");
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

    assert_eq!(changed.net_up_bps, None);
    assert_eq!(changed.net_down_bps, None);
    assert!(changed.net_up_history.is_empty());
    assert!(changed.net_down_history.is_empty());
    assert!(lanes.network.net_up_history().is_empty());
    assert!(lanes.network.net_down_history().is_empty());

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
        clock(11),
        false,
    );
    assert!(baseline.net_up_history.is_empty());

    tree.write("sys/class/net/wlan0/statistics/tx_bytes", "1100\n");
    tree.write("sys/class/net/wlan0/statistics/rx_bytes", "2400\n");
    let resumed = run_collect(
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
    assert_eq!(resumed.net_up_history, vec![100]);
    assert_eq!(resumed.net_down_history, vec![400]);
}

#[test]
fn collect_graphs_page_samples_gpu_and_net_history() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/net/eth0/statistics/tx_bytes", "0\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "0\n");
    let mut cfg = cfg_panel(&["net_speed", "gpu_nvidia_temp"]);
    cfg.pages.order = vec![String::from("graphs")];
    cfg.pages.graph_history_length = 3;
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        net_device: Some("eth0".to_owned()),
        ..HardwareInventory::default()
    };

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut nvml = FakeNvml::new(vec![
        Ok(NvidiaMetrics {
            temp_celsius: Some(60),
            usage_percent: Some(42),
            memory_percent: None,
            decoder_percent: Some(7),
            fan_percent: None,
        }),
        Ok(NvidiaMetrics {
            temp_celsius: Some(61),
            usage_percent: Some(55),
            memory_percent: None,
            decoder_percent: Some(8),
            fan_percent: None,
        }),
    ]);
    let r1 = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(0),
        false,
    );
    // First net_speed sample is None (no prev); net history records 0 for it
    // because up/down are None (sample skipped) → empty buffer.
    assert!(r1.net_up_history.is_empty());
    assert_eq!(r1.gpu_usage_history, vec![42]);
    assert_eq!(r1.gpu_dec_history, vec![7]);

    tree.write("sys/class/net/eth0/statistics/tx_bytes", "100\n");
    tree.write("sys/class/net/eth0/statistics/rx_bytes", "200\n");
    let r2 = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(1),
        false,
    );
    // Second poll: net rate present → history records it; gpu usage appended.
    assert_eq!(r2.net_up_history.len(), 1);
    assert_eq!(r2.gpu_usage_history, vec![42, 55]);
    assert_eq!(r2.gpu_dec_history, vec![7, 8]);
}

#[test]
fn decoder_history_carries_transient_failure_and_stops_on_not_supported() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut cfg = cfg_panel(&["gpu_nvidia_dec_usage"]);
    cfg.pages.order = vec![String::from("graphs")];
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mandatory = |decoder| NvmlMetrics {
        temp_celsius: 50,
        usage_percent: 40,
        memory_percent: 20,
        decoder,
        fan: NvmlOptionalMetric::NotSupported,
    };
    let mut nvml = FakeNvml::typed(vec![
        Ok(mandatory(NvmlOptionalMetric::Value(7))),
        Ok(mandatory(NvmlOptionalMetric::Failed)),
        Ok(mandatory(NvmlOptionalMetric::NotSupported)),
    ]);

    let value = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(0),
        false,
    );
    let failure = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(1),
        false,
    );
    let absent = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(2),
        false,
    );

    assert_eq!(value.gpu_dec_history, vec![7]);
    assert_eq!(failure.gpu_dec_history, vec![7, 7]);
    assert_eq!(absent.gpu_dec_history, vec![7, 7]);
    assert_eq!(lanes.gpu_history.latest_decoder, None);
}

// ── collect: combined capability set + ordered trace ─────────────────────────
