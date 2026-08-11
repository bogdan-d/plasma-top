use super::*;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::boundary::CommandRunner;
use crate::test_support::{FakeClock, FakeCommandRunner};

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "plasma-top-network-{}-{unique}",
            std::process::id()
        ));
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
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn ok_output(program: &str, args: &[&str], stdout: &str) -> CommandOutput {
    CommandOutput {
        program: Path::new(program).to_path_buf(),
        args: args.iter().map(|arg| OsString::from(*arg)).collect(),
        status: CommandStatus::Exit(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        truncation: Default::default(),
    }
}

fn exit_output(program: &str, args: &[&str], code: i32, stdout: &str) -> CommandOutput {
    CommandOutput {
        program: Path::new(program).to_path_buf(),
        args: args.iter().map(|arg| OsString::from(*arg)).collect(),
        status: CommandStatus::Exit(code),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        truncation: Default::default(),
    }
}

#[test]
fn detect_net_device_prefers_route_get_output() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        IP_PROGRAM,
        ["route", "get", "8.8.8.8"],
        ok_output(
            IP_PROGRAM,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 via 192.168.1.1 dev wlan0 src 192.168.1.5 uid 1000\n",
        ),
    );

    let device = detect_net_device(&mut |program, args| runner.run(program, args, COMMAND_TIMEOUT));

    assert_eq!(device.as_deref(), Some("wlan0"));
    assert_eq!(runner.call_trace().len(), 1);
    assert_eq!(runner.call_trace()[0].program, PathBuf::from(IP_PROGRAM));
    assert_eq!(runner.call_trace()[0].timeout, COMMAND_TIMEOUT);
}

#[test]
fn detect_net_device_falls_back_to_default_route() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        IP_PROGRAM,
        ["route", "get", "8.8.8.8"],
        exit_output(IP_PROGRAM, &["route", "get", "8.8.8.8"], 1, ""),
    );
    runner.enqueue(
        IP_PROGRAM,
        ["route", "show", "default"],
        ok_output(
            IP_PROGRAM,
            &["route", "show", "default"],
            "default via 10.0.0.1 dev eth0 proto dhcp metric 100\n",
        ),
    );

    let device = detect_net_device(&mut |program, args| runner.run(program, args, COMMAND_TIMEOUT));

    assert_eq!(device.as_deref(), Some("eth0"));
    assert_eq!(runner.call_trace().len(), 2);
}

#[test]
fn detect_has_wifi_checks_wireless_subdirectories() {
    let tmp = TempTree::new();
    tmp.write("sys/class/net/eth0/statistics/tx_bytes", "1\n");
    tmp.write("sys/class/net/wlan0/wireless/.keep", "");

    assert!(detect_has_wifi(&tmp.path().join("sys")));
    assert!(!detect_has_wifi(&tmp.path().join("missing")));
}

#[test]
fn dbm_to_pct_clamps_to_visible_range() {
    assert_eq!(dbm_to_pct(-50), 100);
    assert_eq!(dbm_to_pct(-100), 0);
    assert_eq!(dbm_to_pct(-67), 66);
    assert_eq!(dbm_to_pct(-120), 0);
}

#[test]
fn read_net_info_reads_wireless_route_ip_ssid_and_signal() {
    let tmp = TempTree::new();
    tmp.write("sys/class/net/wlan0/wireless/.keep", "");

    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        IP_PROGRAM,
        ["route", "get", "8.8.8.8"],
        ok_output(
            IP_PROGRAM,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 via 192.168.1.1 dev wlan0 src 192.168.1.5 uid 1000\n",
        ),
    );
    runner.enqueue(
        IW_PROGRAM,
        ["dev", "wlan0", "link"],
        ok_output(
            IW_PROGRAM,
            &["dev", "wlan0", "link"],
            "Connected to 00:11:22:33:44:55 (on wlan0)\n\
             \tSSID: MyWifi\n\
             \tsignal: -60 dBm\n",
        ),
    );

    let info = read_net_info(&tmp.path().join("sys"), &mut |program, args| {
        runner.run(program, args, COMMAND_TIMEOUT)
    });

    assert_eq!(
        info,
        NetInfo {
            device: Some(String::from("wlan0")),
            ip_address: Some(String::from("192.168.1.5")),
            ssid: Some(String::from("MyWifi")),
            signal_pct: Some(80),
        }
    );
    assert_eq!(runner.call_trace().len(), 2);
}

#[test]
fn read_net_info_skips_iw_for_wired_devices() {
    let tmp = TempTree::new();
    tmp.write("sys/class/net/eth0/statistics/tx_bytes", "1\n");

    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        IP_PROGRAM,
        ["route", "get", "8.8.8.8"],
        ok_output(
            IP_PROGRAM,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 via 10.0.0.1 dev eth0 src 10.0.0.20 uid 1000\n",
        ),
    );

    let info = read_net_info(&tmp.path().join("sys"), &mut |program, args| {
        runner.run(program, args, COMMAND_TIMEOUT)
    });

    assert_eq!(info.device.as_deref(), Some("eth0"));
    assert_eq!(info.ip_address.as_deref(), Some("10.0.0.20"));
    assert_eq!(info.ssid, None);
    assert_eq!(info.signal_pct, None);
    assert_eq!(runner.call_trace().len(), 1);
}

#[test]
fn read_net_speed_needs_two_samples_and_resets_on_interface_change() {
    let tmp = TempTree::new();
    tmp.write("sys/class/net/wlan0/statistics/tx_bytes", "100\n");
    tmp.write("sys/class/net/wlan0/statistics/rx_bytes", "400\n");
    tmp.write("sys/class/net/eth0/statistics/tx_bytes", "50\n");
    tmp.write("sys/class/net/eth0/statistics/rx_bytes", "60\n");

    let mut state = NetworkState::default();
    let mut clock = FakeClock::default();
    assert_eq!(
        read_net_speed(&tmp.path().join("sys"), &mut state, "wlan0", clock.now),
        (None, None)
    );

    tmp.write("sys/class/net/wlan0/statistics/tx_bytes", "300\n");
    tmp.write("sys/class/net/wlan0/statistics/rx_bytes", "900\n");
    clock.advance(Duration::from_secs(2));
    assert_eq!(
        read_net_speed(&tmp.path().join("sys"), &mut state, "wlan0", clock.now),
        (Some(100), Some(250))
    );

    clock.advance(Duration::from_secs(1));
    assert_eq!(
        read_net_speed(&tmp.path().join("sys"), &mut state, "eth0", clock.now),
        (None, None)
    );
}

#[test]
fn read_net_speed_resets_on_counter_rollback_and_zero_dt() {
    let tmp = TempTree::new();
    tmp.write("sys/class/net/wlan0/statistics/tx_bytes", "100\n");
    tmp.write("sys/class/net/wlan0/statistics/rx_bytes", "100\n");

    let mut state = NetworkState::default();
    let mut clock = FakeClock::default();
    let _ = read_net_speed(&tmp.path().join("sys"), &mut state, "wlan0", clock.now);

    tmp.write("sys/class/net/wlan0/statistics/tx_bytes", "120\n");
    tmp.write("sys/class/net/wlan0/statistics/rx_bytes", "140\n");
    assert_eq!(
        read_net_speed(&tmp.path().join("sys"), &mut state, "wlan0", clock.now),
        (None, None),
        "same timestamp cannot yield a rate"
    );

    clock.advance(Duration::from_secs(1));
    tmp.write("sys/class/net/wlan0/statistics/tx_bytes", "20\n");
    tmp.write("sys/class/net/wlan0/statistics/rx_bytes", "30\n");
    assert_eq!(
        read_net_speed(&tmp.path().join("sys"), &mut state, "wlan0", clock.now),
        (None, None),
        "counter rollback resets instead of emitting negatives"
    );
}

#[test]
fn sample_net_history_requires_graphs_and_trims_to_length() {
    let mut cfg = Config::default();
    cfg.display.history_interval = crate::domain::Cadence::from_millis(2000);
    cfg.pages.order = vec![String::from("graphs")];
    cfg.pages.graph_history_length = 2;

    let mut state = NetworkState::default();
    let mut clock = FakeClock::default();
    sample_net_history(&mut state, &cfg, clock.now, Some(10), Some(20));
    clock.advance(Duration::from_secs(1));
    sample_net_history(&mut state, &cfg, clock.now, Some(30), Some(40));
    clock.advance(Duration::from_secs(1));
    sample_net_history(&mut state, &cfg, clock.now, Some(50), None);
    clock.advance(Duration::from_secs(2));
    sample_net_history(&mut state, &cfg, clock.now, Some(70), Some(80));

    assert_eq!(state.net_up_history(), &[50, 70]);
    assert_eq!(state.net_down_history(), &[0, 80]);
}

#[test]
fn sample_net_history_noops_when_graphs_disabled() {
    let cfg = Config::default();
    let mut state = NetworkState::default();

    sample_net_history(
        &mut state,
        &cfg,
        ClockSnapshot::default(),
        Some(10),
        Some(20),
    );

    assert!(state.net_up_history().is_empty());
    assert!(state.net_down_history().is_empty());
}
