//! Network route, wifi, and byte-rate readings.
//!
//! This module ports the Wave 3 network lane from `src/sensors.py`: active
//! route/device detection via `ip`, wifi SSID/signal via `iw`, interface
//! presence via sysfs, per-interface tx/rx byte rates from
//! `/sys/class/net/<if>/statistics`, and the graphs page's bounded network
//! history. The API is deterministic by construction: callers provide explicit
//! sysfs roots, command execution closures, and monotonic [`ClockSnapshot`]s so
//! tests never touch the host network stack or sleep.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::config::Config;
use crate::domain::boundary::{ClockSnapshot, CommandOutput, CommandStatus};

const IP_PROGRAM: &str = "ip";
const IW_PROGRAM: &str = "iw";
#[cfg(all(test, feature = "test-support"))]
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
const NET_INFO_TTL: Duration = Duration::from_secs(10);
const NANOS_PER_SECOND: u128 = 1_000_000_000;

/// Point-in-time network identity details for the active route.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetInfo {
    /// Current route device, e.g. `wlan0`.
    pub device: Option<String>,
    /// Current source IP for the active route.
    pub ip_address: Option<String>,
    /// Current wifi SSID when the route device is wireless.
    pub ssid: Option<String>,
    /// Wifi signal quality in percent, derived from dBm.
    pub signal_pct: Option<i32>,
}

/// Mutable network cache/diff/history state that persists between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkState {
    cached_info: NetInfo,
    net_info_sample_at: Option<Duration>,
    rate_device: Option<String>,
    prev_tx_bytes: u64,
    prev_rx_bytes: u64,
    rate_sample_at: Option<Duration>,
    net_up_history: Vec<u64>,
    net_down_history: Vec<u64>,
    net_history_sample_at: Option<Duration>,
}

impl NetworkState {
    /// Returns the cached upload-rate history for the graphs page.
    #[must_use]
    pub fn net_up_history(&self) -> &[u64] {
        &self.net_up_history
    }

    /// Returns the cached download-rate history for the graphs page.
    #[must_use]
    pub fn net_down_history(&self) -> &[u64] {
        &self.net_down_history
    }
}

/// Detects the current route device using the same two-command fallback as Python.
///
/// First tries `ip route get 8.8.8.8`; if that fails or yields no `dev` token,
/// falls back to `ip route show default`.
#[must_use]
pub fn detect_net_device<E>(
    run_command: &mut impl FnMut(&Path, &[OsString]) -> Result<CommandOutput, E>,
) -> Option<String> {
    for args in [["route", "get", "8.8.8.8"], ["route", "show", "default"]] {
        let Some(stdout) = run_command_stdout(run_command, IP_PROGRAM, &args) else {
            continue;
        };
        let tokens: Vec<&str> = stdout.split_whitespace().collect();
        if let Some(device) = token_after(&tokens, "dev") {
            return Some(device.to_owned());
        }
    }
    None
}

/// Returns `true` when at least one wireless network interface exists in sysfs.
#[must_use]
pub fn detect_has_wifi(sys_root: &Path) -> bool {
    let Ok(entries) = fs::read_dir(sys_root.join("class/net")) else {
        return false;
    };
    entries
        .flatten()
        .map(|entry| entry.file_name())
        .filter_map(|name| name.into_string().ok())
        .any(|device| is_wireless(sys_root, &device))
}

/// Converts wifi signal strength from dBm into a percentage.
///
/// Matches Python's linear clamp: `-50 dBm` or better becomes `100%`,
/// `-100 dBm` or worse becomes `0%`, and values in between scale linearly.
#[must_use]
pub fn dbm_to_pct(dbm: i32) -> i32 {
    (2 * (dbm + 100)).clamp(0, 100)
}

/// Reads route device/IP plus wifi SSID/signal for the active route.
///
/// Mirrors `src/sensors.py::_read_net_info`: route device and source IP come
/// from a single `ip route get 8.8.8.8` invocation, and `iw dev <if> link`
/// runs only when that device is wireless.
#[must_use]
pub fn read_net_info<E>(
    sys_root: &Path,
    run_command: &mut impl FnMut(&Path, &[OsString]) -> Result<CommandOutput, E>,
) -> NetInfo {
    let mut info = NetInfo::default();

    if let Some(stdout) = run_command_stdout(run_command, IP_PROGRAM, &["route", "get", "8.8.8.8"])
    {
        let tokens: Vec<&str> = stdout.split_whitespace().collect();
        info.device = token_after(&tokens, "dev").map(str::to_owned);
        info.ip_address = token_after(&tokens, "src").map(str::to_owned);
    }

    let Some(device) = info.device.as_deref() else {
        return info;
    };
    if !is_wireless(sys_root, device) {
        return info;
    }

    let Some(stdout) = run_command_stdout(run_command, IW_PROGRAM, &["dev", device, "link"]) else {
        return info;
    };
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("SSID:") {
            let ssid = rest.trim();
            if !ssid.is_empty() {
                info.ssid = Some(ssid.to_owned());
            }
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("signal:") {
            let Some(dbm_text) = rest.split_whitespace().next() else {
                continue;
            };
            let Ok(dbm) = dbm_text.parse::<i32>() else {
                continue;
            };
            info.signal_pct = Some(dbm_to_pct(dbm));
        }
    }

    info
}

/// Returns cached network identity details, refreshing every 10 seconds.
#[must_use]
pub fn read_net_info_cached<E>(
    sys_root: &Path,
    state: &mut NetworkState,
    clock: ClockSnapshot,
    run_command: &mut impl FnMut(&Path, &[OsString]) -> Result<CommandOutput, E>,
) -> NetInfo {
    let refresh = match state.net_info_sample_at {
        None => true,
        Some(previous) => clock.monotonic.saturating_sub(previous) >= NET_INFO_TTL,
    };
    if refresh {
        state.cached_info = read_net_info(sys_root, run_command);
        state.net_info_sample_at = Some(clock.monotonic);
    }
    state.cached_info.clone()
}

/// Reads per-interface upload/download byte rates from sysfs statistics.
///
/// The first sample for a device returns `(None, None)`. A device change or a
/// counter rollback resets the diff state and also returns `(None, None)` to
/// avoid negative or spurious spikes.
#[must_use]
pub fn read_net_speed(
    sys_root: &Path,
    state: &mut NetworkState,
    device: &str,
    clock: ClockSnapshot,
) -> (Option<u64>, Option<u64>) {
    let Some(tx_bytes) = read_interface_counter(sys_root, device, "tx_bytes") else {
        return (None, None);
    };
    let Some(rx_bytes) = read_interface_counter(sys_root, device, "rx_bytes") else {
        return (None, None);
    };

    let same_device = state.rate_device.as_deref() == Some(device);
    let previous_tx = state.prev_tx_bytes;
    let previous_rx = state.prev_rx_bytes;
    let previous_sample_at = state.rate_sample_at;

    state.rate_device = Some(device.to_owned());
    state.prev_tx_bytes = tx_bytes;
    state.prev_rx_bytes = rx_bytes;
    state.rate_sample_at = Some(clock.monotonic);

    if !same_device {
        return (None, None);
    }
    let Some(previous_sample_at) = previous_sample_at else {
        return (None, None);
    };
    if tx_bytes < previous_tx || rx_bytes < previous_rx {
        return (None, None);
    }

    let elapsed = clock.monotonic.saturating_sub(previous_sample_at);
    let elapsed_nanos = elapsed.as_nanos();
    if elapsed_nanos == 0 {
        return (None, None);
    }

    let up_bps = rate_per_second(tx_bytes - previous_tx, elapsed_nanos);
    let down_bps = rate_per_second(rx_bytes - previous_rx, elapsed_nanos);
    (Some(up_bps), Some(down_bps))
}

/// Samples upload/download history for the graphs page.
///
/// History is recorded only when the `graphs` page is enabled. Like Python, if
/// either direction is present at a sampling instant then the missing side is
/// recorded as zero for that sample.
pub fn sample_net_history(
    state: &mut NetworkState,
    cfg: &Config,
    clock: ClockSnapshot,
    up_bps: Option<u64>,
    down_bps: Option<u64>,
) {
    if !graphs_enabled(cfg) || (up_bps.is_none() && down_bps.is_none()) {
        return;
    }
    if history_due(
        &mut state.net_history_sample_at,
        clock.monotonic,
        history_interval(cfg),
    ) {
        state.net_up_history.push(up_bps.unwrap_or(0));
        state.net_down_history.push(down_bps.unwrap_or(0));
        let max_len = cfg.pages.graph_history_length.max(0) as usize;
        trim_to_len(&mut state.net_up_history, max_len);
        trim_to_len(&mut state.net_down_history, max_len);
    }
}

fn run_command_stdout<E>(
    run_command: &mut impl FnMut(&Path, &[OsString]) -> Result<CommandOutput, E>,
    program: &str,
    args: &[&str],
) -> Option<String> {
    let argv: Vec<OsString> = args.iter().map(|arg| OsString::from(*arg)).collect();
    let output = run_command(Path::new(program), &argv).ok()?;
    match output.status {
        CommandStatus::Exit(0) => Some(String::from_utf8_lossy(&output.stdout).into_owned()),
        CommandStatus::Exit(_) | CommandStatus::Signal(_) => None,
    }
}

fn token_after<'a>(tokens: &'a [&'a str], key: &str) -> Option<&'a str> {
    tokens
        .windows(2)
        .find(|window| window[0] == key)
        .map(|window| window[1])
}

fn is_wireless(sys_root: &Path, device: &str) -> bool {
    sys_root
        .join("class/net")
        .join(device)
        .join("wireless")
        .exists()
}

fn read_interface_counter(sys_root: &Path, device: &str, counter: &str) -> Option<u64> {
    fs::read_to_string(
        sys_root
            .join("class/net")
            .join(device)
            .join("statistics")
            .join(counter),
    )
    .ok()?
    .trim()
    .parse::<u64>()
    .ok()
}

fn rate_per_second(delta: u64, elapsed_nanos: u128) -> u64 {
    let scaled = u128::from(delta).saturating_mul(NANOS_PER_SECOND) / elapsed_nanos;
    u64::try_from(scaled).unwrap_or(u64::MAX)
}

fn graphs_enabled(cfg: &Config) -> bool {
    cfg.pages.order.iter().any(|page| page == "graphs")
}

fn history_interval(cfg: &Config) -> Duration {
    if cfg.display.history_interval <= 0.0 {
        Duration::ZERO
    } else {
        Duration::from_secs_f64(cfg.display.history_interval)
    }
}

fn history_due(last_sample_at: &mut Option<Duration>, now: Duration, interval: Duration) -> bool {
    match last_sample_at {
        None => {
            *last_sample_at = Some(now);
            true
        }
        Some(previous) if now.saturating_sub(*previous) >= interval => {
            *previous = now;
            true
        }
        Some(_) => false,
    }
}

fn trim_to_len<T>(values: &mut Vec<T>, max_len: usize) {
    if max_len == 0 {
        values.clear();
        return;
    }
    if values.len() > max_len {
        let excess = values.len() - max_len;
        values.drain(..excess);
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
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
            let root = std::env::temp_dir()
                .join(format!("pirostats-network-{}-{unique}", std::process::id()));
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
        }
    }

    fn exit_output(program: &str, args: &[&str], code: i32, stdout: &str) -> CommandOutput {
        CommandOutput {
            program: Path::new(program).to_path_buf(),
            args: args.iter().map(|arg| OsString::from(*arg)).collect(),
            status: CommandStatus::Exit(code),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
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

        let device =
            detect_net_device(&mut |program, args| runner.run(program, args, COMMAND_TIMEOUT));

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

        let device =
            detect_net_device(&mut |program, args| runner.run(program, args, COMMAND_TIMEOUT));

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
    fn read_net_info_cached_honors_ttl() {
        let tmp = TempTree::new();
        tmp.write("sys/class/net/wlan0/wireless/.keep", "");

        let mut runner = FakeCommandRunner::new();
        runner.enqueue(
            IP_PROGRAM,
            ["route", "get", "8.8.8.8"],
            ok_output(
                IP_PROGRAM,
                &["route", "get", "8.8.8.8"],
                "8.8.8.8 dev wlan0 src 192.168.1.5\n",
            ),
        );
        runner.enqueue(
            IW_PROGRAM,
            ["dev", "wlan0", "link"],
            ok_output(IW_PROGRAM, &["dev", "wlan0", "link"], "SSID: Alpha\n"),
        );
        runner.enqueue(
            IP_PROGRAM,
            ["route", "get", "8.8.8.8"],
            ok_output(
                IP_PROGRAM,
                &["route", "get", "8.8.8.8"],
                "8.8.8.8 dev wlan0 src 192.168.1.6\n",
            ),
        );
        runner.enqueue(
            IW_PROGRAM,
            ["dev", "wlan0", "link"],
            ok_output(IW_PROGRAM, &["dev", "wlan0", "link"], "SSID: Beta\n"),
        );

        let mut state = NetworkState::default();
        let mut clock = FakeClock::default();
        let first = read_net_info_cached(
            &tmp.path().join("sys"),
            &mut state,
            clock.now,
            &mut |program, args| runner.run(program, args, COMMAND_TIMEOUT),
        );
        clock.advance(Duration::from_secs(5));
        let second = read_net_info_cached(
            &tmp.path().join("sys"),
            &mut state,
            clock.now,
            &mut |program, args| runner.run(program, args, COMMAND_TIMEOUT),
        );
        clock.advance(Duration::from_secs(5));
        let third = read_net_info_cached(
            &tmp.path().join("sys"),
            &mut state,
            clock.now,
            &mut |program, args| runner.run(program, args, COMMAND_TIMEOUT),
        );

        assert_eq!(first.ssid.as_deref(), Some("Alpha"));
        assert_eq!(second.ssid.as_deref(), Some("Alpha"));
        assert_eq!(third.ssid.as_deref(), Some("Beta"));
        assert_eq!(
            runner.call_trace().len(),
            4,
            "two refreshes, each with ip+iw"
        );
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
        cfg.display.history_interval = 2.0;
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
}
