use std::path::Path;

use crate::domain::boundary::{ClockSnapshot, CommandRunner};

use super::{
    AttemptStatus, NetworkInfoResult, ReadOutcome, Timings, cached_attempt, commit_attempt, timed,
};
use crate::sensors::{NETWORK_COMMAND_TIMEOUT, network};

/// Performs one network-identity attempt without cadence policy.
pub(crate) fn attempt_network_info(
    state: &mut network::NetworkState,
    sys_root: &Path,
    commands: &mut dyn CommandRunner,
    clock: &mut dyn FnMut() -> ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> NetworkInfoResult {
    let outcome = timed(timings, "net_info", || {
        network::read_net_info_once(
            sys_root,
            &mut |program, args| commands.run(program, args, NETWORK_COMMAND_TIMEOUT),
            &mut || clock().monotonic,
        )
    });
    let (reading, wifi, route_status) = match outcome {
        network::NetInfoReadOutcome::RouteFailed { attempted_at } => (
            commit_attempt(&mut state.info, ReadOutcome::Failed, attempted_at),
            cached_attempt(&state.wifi),
            AttemptStatus::Failed,
        ),
        network::NetInfoReadOutcome::Route {
            info,
            captured_at,
            wifi,
        } => {
            let device = info.device.clone();
            let wifi = match wifi {
                network::WifiReadOutcome::Captured {
                    ssid,
                    signal_pct,
                    captured_at,
                } => {
                    let value = network::WifiInfo {
                        device: device.clone().unwrap_or_default(),
                        ssid,
                        signal_pct,
                    };
                    commit_attempt(&mut state.wifi, ReadOutcome::Value(value), captured_at)
                }
                network::WifiReadOutcome::NotWireless { checked_at } => {
                    commit_attempt(&mut state.wifi, ReadOutcome::Absent, checked_at)
                }
                network::WifiReadOutcome::Failed { attempted_at } => {
                    let same_device = state
                        .wifi
                        .latest
                        .as_ref()
                        .is_some_and(|sample| Some(&sample.value.device) == device.as_ref());
                    if !same_device {
                        state.wifi.invalidate();
                    }
                    commit_attempt(&mut state.wifi, ReadOutcome::Failed, attempted_at)
                }
            };
            let reading = commit_attempt(&mut state.info, ReadOutcome::Value(info), captured_at);
            state.info_source.clone_from(&device);
            (reading, wifi, AttemptStatus::Captured)
        }
    };
    NetworkInfoResult {
        reading,
        wifi,
        route_status,
    }
}
