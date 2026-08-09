//! Network route, wifi, and byte-rate readings.
//!
//! Active
//! route/device detection via `ip`, wifi SSID/signal via `iw`, interface
//! presence via sysfs, per-interface tx/rx byte rates from
//! `/sys/class/net/<if>/statistics`, and the graphs page's bounded network
//! history. The API is deterministic by construction: callers provide explicit
//! sysfs roots, command execution closures, and monotonic [`ClockSnapshot`]s so
//! tests never touch the host network stack or sleep.

use std::ffi::OsString;
use std::fs;
use std::io;
use std::path::Path;
use std::time::Duration;

use crate::config::Config;
use crate::domain::boundary::{ClockSnapshot, CommandOutput, CommandStatus};
use crate::domain::readings::RetainedMetricSample;

const IP_PROGRAM: &str = "ip";
const IW_PROGRAM: &str = "iw";
#[cfg(all(test, feature = "test-support"))]
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
pub(super) const NET_INFO_TTL: Duration = Duration::from_secs(10);
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

/// Wi-Fi identity captured independently from the active route.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WifiInfo {
    pub(crate) device: String,
    pub(crate) ssid: Option<String>,
    pub(crate) signal_pct: Option<i32>,
}

/// Mutable network sample, diff, and history state that persists between sampling passes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkState {
    /// Latest route/IP sample. Wi-Fi fields remain empty and are composed only for display.
    pub(super) info: RetainedMetricSample<NetInfo>,
    pub(super) wifi: RetainedMetricSample<WifiInfo>,
    pub(super) info_source: Option<String>,
    pub(super) rate: RetainedMetricSample<(u64, u64)>,
    pub(super) rate_device: Option<String>,
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

    pub(crate) fn reset_rate(&mut self) {
        self.rate.invalidate();
        self.rate_device = None;
        self.prev_tx_bytes = 0;
        self.prev_rx_bytes = 0;
        self.rate_sample_at = None;
    }

    pub(crate) fn reset_info(&mut self) {
        self.info.invalidate();
        self.wifi.invalidate();
    }

    pub(crate) fn reset_history(&mut self) {
        self.net_up_history.clear();
        self.net_down_history.clear();
        self.net_history_sample_at = None;
    }

    pub(crate) fn reconcile_sources(
        &mut self,
        device: Option<&str>,
        wants_rate: bool,
        wants_info: bool,
        wants_history: bool,
    ) {
        let rate_source_changed =
            self.rate_device.is_some() && self.rate_device.as_deref() != device;
        if !wants_rate || device.is_none() || rate_source_changed {
            self.reset_rate();
            if rate_source_changed {
                self.reset_history();
            }
        }

        if !wants_info {
            self.reset_info();
            self.info_source = None;
        } else if self.info_source.as_deref() != device {
            self.reset_info();
            self.info_source = device.map(str::to_owned);
        }

        if !wants_history {
            self.reset_history();
        }
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
        if let Some(device) = route_device(&stdout) {
            return Some(device.to_owned());
        }
    }
    None
}

/// Returns `true` when at least one wireless network interface exists in sysfs.
#[must_use]
pub fn detect_has_wifi(sys_root: &Path) -> bool {
    detect_has_wifi_outcome(sys_root).unwrap_or(false)
}

/// Detects wireless interfaces without flattening enumeration failures.
pub(crate) fn detect_has_wifi_outcome(sys_root: &Path) -> io::Result<bool> {
    let entries = fs::read_dir(sys_root.join("class/net"))?;
    for entry in entries {
        let entry = entry?;
        let device = entry
            .file_name()
            .into_string()
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid interface name"))?;
        if is_wireless_outcome(sys_root, &device)? {
            return Ok(true);
        }
    }
    Ok(false)
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
    match read_net_info_once(sys_root, run_command, &mut || Duration::ZERO) {
        NetInfoReadOutcome::Route { mut info, wifi, .. } => {
            if let WifiReadOutcome::Captured {
                ssid, signal_pct, ..
            } = wifi
            {
                info.ssid = ssid;
                info.signal_pct = signal_pct;
            }
            info
        }
        NetInfoReadOutcome::RouteFailed { .. } => NetInfo::default(),
    }
}

/// Outcome of the Wi-Fi portion of one successful route attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WifiReadOutcome {
    Captured {
        ssid: Option<String>,
        signal_pct: Option<i32>,
        captured_at: Duration,
    },
    NotWireless {
        checked_at: Duration,
    },
    Failed {
        attempted_at: Duration,
    },
}

/// Separate route and Wi-Fi outcomes from one network-identity attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NetInfoReadOutcome {
    Route {
        info: NetInfo,
        captured_at: Duration,
        wifi: WifiReadOutcome,
    },
    RouteFailed {
        attempted_at: Duration,
    },
}

/// Performs one network-identity attempt without conflating route and Wi-Fi failures.
pub(crate) fn read_net_info_once<E>(
    sys_root: &Path,
    run_command: &mut impl FnMut(&Path, &[OsString]) -> Result<CommandOutput, E>,
    monotonic: &mut impl FnMut() -> Duration,
) -> NetInfoReadOutcome {
    let mut info = NetInfo::default();

    let route = run_command_stdout(run_command, IP_PROGRAM, &["route", "get", "8.8.8.8"]);
    let route_at = monotonic();
    let Some(stdout) = route else {
        return NetInfoReadOutcome::RouteFailed {
            attempted_at: route_at,
        };
    };
    let Some(device) = route_device(&stdout) else {
        return NetInfoReadOutcome::RouteFailed {
            attempted_at: route_at,
        };
    };
    let tokens: Vec<&str> = stdout.split_whitespace().collect();
    info.device = Some(device.to_owned());
    info.ip_address = token_after(&tokens, "src").map(str::to_owned);

    match is_wireless_outcome(sys_root, device) {
        Ok(true) => {}
        Ok(false) => {
            let checked_at = monotonic();
            return NetInfoReadOutcome::Route {
                info,
                captured_at: route_at,
                wifi: WifiReadOutcome::NotWireless { checked_at },
            };
        }
        Err(_) => {
            let attempted_at = monotonic();
            return NetInfoReadOutcome::Route {
                info,
                captured_at: route_at,
                wifi: WifiReadOutcome::Failed { attempted_at },
            };
        }
    }

    let wifi = run_command_stdout(run_command, IW_PROGRAM, &["dev", device, "link"]);
    let wifi_at = monotonic();
    let Some(stdout) = wifi else {
        return NetInfoReadOutcome::Route {
            info,
            captured_at: route_at,
            wifi: WifiReadOutcome::Failed {
                attempted_at: wifi_at,
            },
        };
    };
    let mut ssid = None;
    let mut signal_pct = None;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("SSID:") {
            let value = rest.trim();
            if !value.is_empty() {
                ssid = Some(value.to_owned());
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
            signal_pct = Some(dbm_to_pct(dbm));
        }
    }

    NetInfoReadOutcome::Route {
        info,
        captured_at: route_at,
        wifi: WifiReadOutcome::Captured {
            ssid,
            signal_pct,
            captured_at: wifi_at,
        },
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RateReadOutcome {
    Value(u64, u64),
    Baseline,
    NoDelta,
    Failed,
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
    match read_net_speed_once(sys_root, state, device, clock) {
        RateReadOutcome::Value(up, down) => (Some(up), Some(down)),
        RateReadOutcome::Baseline | RateReadOutcome::NoDelta | RateReadOutcome::Failed => {
            (None, None)
        }
    }
}

/// Performs one network-counter attempt and distinguishes a new baseline from an invalid same-source delta.
pub(crate) fn read_net_speed_once(
    sys_root: &Path,
    state: &mut NetworkState,
    device: &str,
    clock: ClockSnapshot,
) -> RateReadOutcome {
    let Some(tx_bytes) = read_interface_counter(sys_root, device, "tx_bytes") else {
        return RateReadOutcome::Failed;
    };
    let Some(rx_bytes) = read_interface_counter(sys_root, device, "rx_bytes") else {
        return RateReadOutcome::Failed;
    };

    let same_device = state.rate_device.as_deref() == Some(device);
    if !same_device {
        if state.rate_device.is_some() {
            state.reset_history();
        }
        state.rate_device = Some(device.to_owned());
        state.prev_tx_bytes = tx_bytes;
        state.prev_rx_bytes = rx_bytes;
        state.rate_sample_at = Some(clock.monotonic);
        return RateReadOutcome::Baseline;
    }
    let Some(previous_sample_at) = state.rate_sample_at else {
        state.prev_tx_bytes = tx_bytes;
        state.prev_rx_bytes = rx_bytes;
        state.rate_sample_at = Some(clock.monotonic);
        return RateReadOutcome::Baseline;
    };
    if tx_bytes < state.prev_tx_bytes || rx_bytes < state.prev_rx_bytes {
        state.prev_tx_bytes = tx_bytes;
        state.prev_rx_bytes = rx_bytes;
        state.rate_sample_at = Some(clock.monotonic);
        return RateReadOutcome::Baseline;
    }
    let Some(elapsed) = clock.monotonic.checked_sub(previous_sample_at) else {
        return RateReadOutcome::NoDelta;
    };
    let elapsed_nanos = elapsed.as_nanos();
    if elapsed_nanos == 0 {
        return RateReadOutcome::NoDelta;
    }

    let up_bps = rate_per_second(tx_bytes - state.prev_tx_bytes, elapsed_nanos);
    let down_bps = rate_per_second(rx_bytes - state.prev_rx_bytes, elapsed_nanos);
    state.prev_tx_bytes = tx_bytes;
    state.prev_rx_bytes = rx_bytes;
    state.rate_sample_at = Some(clock.monotonic);
    RateReadOutcome::Value(up_bps, down_bps)
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
        append_net_history(state, cfg, clock.monotonic, up_bps, down_bps);
    }
}

/// Appends one network history point after synchronous collection marks it due.
pub(crate) fn append_net_history(
    state: &mut NetworkState,
    cfg: &Config,
    captured_at: Duration,
    up_bps: Option<u64>,
    down_bps: Option<u64>,
) {
    state.net_history_sample_at = Some(captured_at);
    state.net_up_history.push(up_bps.unwrap_or(0));
    state.net_down_history.push(down_bps.unwrap_or(0));
    let max_len = cfg.pages.graph_history_length.max(0) as usize;
    trim_to_len(&mut state.net_up_history, max_len);
    trim_to_len(&mut state.net_down_history, max_len);
}

pub(crate) const fn net_history_sample_at(state: &NetworkState) -> Option<Duration> {
    state.net_history_sample_at
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

fn token_after<'a>(tokens: &[&'a str], key: &str) -> Option<&'a str> {
    tokens
        .windows(2)
        .find(|window| window[0] == key)
        .map(|window| window[1])
}

pub(crate) fn route_device(stdout: &str) -> Option<&str> {
    let mut tokens = stdout.split_whitespace();
    tokens.find(|token| *token == "dev")?;
    tokens.next()
}

fn is_wireless_outcome(sys_root: &Path, device: &str) -> io::Result<bool> {
    sys_root
        .join("class/net")
        .join(device)
        .join("wireless")
        .try_exists()
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
    cfg.display.history_interval.duration()
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
mod tests;
