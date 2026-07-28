//! Edge-triggered desktop-notification state machine.

use std::time::Duration;

use toml::{Table, Value};

use crate::config::Config;
use crate::domain::boundary::{
    NotificationError, NotificationFacade, NotificationPayload, NotificationTimeout,
    NotificationUrgency,
};
use crate::domain::readings::{BatteryState, HardwareSnapshot, ReadingsSnapshot};
use crate::domain::state::{NotificationLatch, NotificationState};

const TITLE: &str = "PiroStats";
const ERROR_ICON: &str = "dialog-error";
const WARNING_ICON: &str = "dialog-warning";
const BATTERY_ICON: &str = "battery-caution";
const TEMP_SCALE: &str = "C";

/// One non-fatal desktop-service failure from a notification pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationFailure {
    /// Payload whose state transition completed even though delivery failed.
    pub payload: NotificationPayload,
    /// Adapter failure returned for the payload.
    pub error: NotificationError,
}

/// Outcome of one notification pass.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NotificationReport {
    /// Number of edge-triggered sends attempted.
    pub attempted: usize,
    /// Ordered adapter failures. Processing continues after every failure.
    pub failures: Vec<NotificationFailure>,
}

fn label<'a>(table: &'a Table, key: &str, fallback: &'a str) -> &'a str {
    table.get(key).and_then(Value::as_str).unwrap_or(fallback)
}

fn notify_label<'a>(table: &'a Table, key: &str, fallback: &'a str) -> &'a str {
    table
        .get("notify")
        .and_then(Value::as_table)
        .and_then(|notify| notify.get(key))
        .and_then(Value::as_str)
        .unwrap_or(fallback)
}

fn payload(body: String, icon: &str) -> NotificationPayload {
    NotificationPayload {
        title: TITLE.to_owned(),
        body,
        icon: icon.to_owned(),
        urgency: NotificationUrgency::Critical,
        timeout: NotificationTimeout::Never,
    }
}

fn emit(
    facade: &mut impl NotificationFacade,
    report: &mut NotificationReport,
    notification: NotificationPayload,
) {
    report.attempted += 1;
    if let Err(error) = facade.send(&notification) {
        report.failures.push(NotificationFailure {
            payload: notification,
            error,
        });
    }
}

/// Advances one sustained-alert latch and returns `true` only on its trip edge.
#[must_use]
pub fn sustained(
    latch: &mut NotificationLatch,
    value: f64,
    trip: f64,
    clear: f64,
    hold_seconds: f64,
    now: Duration,
) -> bool {
    if latch.active {
        if value < clear {
            latch.active = false;
            latch.since = None;
        }
        return false;
    }
    if value < trip {
        latch.since = None;
        return false;
    }
    let since = *latch.since.get_or_insert(now);
    if now.saturating_sub(since).as_secs_f64() < hold_seconds {
        return false;
    }
    latch.active = true;
    true
}

/// Checks every enabled notification and advances cross-poll latch state.
///
/// `now` must come from the daemon's monotonic clock and is sampled once for the
/// whole pass. Desktop-service errors are returned in [`NotificationReport`];
/// they never stop later checks or roll back edge state.
pub fn check_and_notify(
    readings: &ReadingsSnapshot,
    cfg: &Config,
    state: &mut NotificationState,
    hardware: &HardwareSnapshot,
    now: Duration,
    facade: &mut impl NotificationFacade,
) -> NotificationReport {
    let enabled = &cfg.notifications;
    let thresholds = &cfg.notify_thresholds;
    let labels = &cfg.labels;
    let hold = f64::from(thresholds.temp_sustain_seconds);
    let cool = f64::from(thresholds.temp_hysteresis);
    let mut report = NotificationReport::default();

    if enabled.cpu_temp
        && let Some(temp) = readings.cpu_temp
        && sustained(
            &mut state.cpu_temp,
            f64::from(temp),
            f64::from(thresholds.cpu_temp),
            f64::from(thresholds.cpu_temp) - cool,
            hold,
            now,
        )
    {
        emit(
            facade,
            &mut report,
            payload(
                format!(
                    "{} {temp}{TEMP_SCALE}",
                    label(labels, "cpu_temp", "Cpu temp")
                ),
                ERROR_ICON,
            ),
        );
    }

    if enabled.gpu_nvidia_temp
        && let Some(temp) = readings.gpu_temp
        && sustained(
            &mut state.gpu_nvidia_temp,
            f64::from(temp),
            f64::from(thresholds.gpu_nvidia_temp),
            f64::from(thresholds.gpu_nvidia_temp) - cool,
            hold,
            now,
        )
    {
        emit(
            facade,
            &mut report,
            payload(
                format!(
                    "{} {temp}{TEMP_SCALE}",
                    label(labels, "gpu_nvidia_temp", "Gpu temp")
                ),
                ERROR_ICON,
            ),
        );
    }

    if enabled.disk_usage {
        for (mount, usage) in &readings.disk_usage {
            let Some(usage) = usage else { continue };
            let over = usage.percent >= thresholds.disk_usage;
            let was = state.disk.get(mount).copied().unwrap_or(false);
            if over && !was {
                emit(
                    facade,
                    &mut report,
                    payload(
                        format!(
                            "{} {mount} {}%",
                            notify_label(labels, "disk_usage", "Disk"),
                            usage.percent
                        ),
                        ERROR_ICON,
                    ),
                );
            }
            state.disk.insert(mount.clone(), over);
        }
    }

    if enabled.disk_smart {
        for (disk_label, healthy) in &readings.disk_smart {
            let Some(healthy) = healthy else { continue };
            let bad = !healthy;
            let was = state.disk_smart.get(disk_label).copied().unwrap_or(false);
            if bad && !was {
                emit(
                    facade,
                    &mut report,
                    payload(
                        format!(
                            "{} {disk_label} {}",
                            notify_label(labels, "disk_smart", "Disk"),
                            notify_label(labels, "smart_fail", "SMART check FAILED")
                        ),
                        ERROR_ICON,
                    ),
                );
            }
            state.disk_smart.insert(disk_label.clone(), bad);
        }
    }

    if enabled.hd_temp {
        for (disk_label, temp) in &readings.hd_temps {
            let Some(temp) = temp else { continue };
            let latch = state.hd_temp.entry(disk_label.clone()).or_default();
            if sustained(
                latch,
                f64::from(*temp),
                f64::from(thresholds.hd_temp),
                f64::from(thresholds.hd_temp) - cool,
                hold,
                now,
            ) {
                emit(
                    facade,
                    &mut report,
                    payload(
                        format!(
                            "{} {disk_label} temp {temp}{TEMP_SCALE}",
                            label(labels, "hd_temp", "Disk")
                        ),
                        WARNING_ICON,
                    ),
                );
            }
        }
    }

    if enabled.battery_sys {
        for battery in &readings.battery_sys {
            if battery.charge_percent == 0 {
                continue;
            }
            let over = battery.state != BatteryState::Charging
                && battery.charge_percent > 0
                && battery.charge_percent <= thresholds.battery_sys;
            let was = state.battery_sys.get(&battery.id).copied().unwrap_or(false);
            if over && !was {
                emit(
                    facade,
                    &mut report,
                    payload(
                        format!(
                            "{} {}%",
                            label(labels, "battery_sys", "Battery"),
                            battery.charge_percent
                        ),
                        BATTERY_ICON,
                    ),
                );
            }
            state.battery_sys.insert(battery.id.clone(), over);
        }
    }

    if enabled.battery_mouse
        && let Some(battery) = &readings.battery_mouse
        && battery.charge_percent != 0
    {
        let over = battery.charge_percent > 0 && battery.charge_percent < thresholds.battery_mouse;
        if over && !state.battery_mouse {
            let name = if battery.name.is_empty() {
                label(labels, "battery_mouse", "Mouse")
            } else {
                &battery.name
            };
            emit(
                facade,
                &mut report,
                payload(format!("{name}: {}%", battery.charge_percent), BATTERY_ICON),
            );
        }
        state.battery_mouse = over;
    }

    if enabled.battery_kbd
        && let Some(battery) = &readings.battery_kbd
        && battery.charge_percent != 0
    {
        let over = battery.charge_percent > 0 && battery.charge_percent < thresholds.battery_kbd;
        if over && !state.battery_kbd {
            let name = if battery.name.is_empty() {
                label(labels, "battery_kbd", "Keyboard")
            } else {
                &battery.name
            };
            emit(
                facade,
                &mut report,
                payload(format!("{name}: {}%", battery.charge_percent), BATTERY_ICON),
            );
        }
        state.battery_kbd = over;
    }

    if enabled.load_avg
        && let Some(load) = readings.load_average
        && sustained(
            &mut state.load_avg,
            load.fifteen / hardware.cpu_count as f64,
            thresholds.load_avg_15,
            thresholds.load_avg_15,
            f64::from(thresholds.load_avg_minutes) * 60.0,
            now,
        )
    {
        emit(
            facade,
            &mut report,
            payload(
                format!(
                    "{} 15m {} {} min ({:.2})",
                    label(labels, "load_avg", "Load avg"),
                    notify_label(labels, "load_high_for", "high for"),
                    thresholds.load_avg_minutes,
                    load.fifteen
                ),
                WARNING_ICON,
            ),
        );
    }

    if enabled.server_check
        && let Some(server_ok) = readings.server_ok
    {
        let down = !server_ok;
        if down && !state.server {
            emit(
                facade,
                &mut report,
                payload(
                    notify_label(labels, "server_down", "Server is not reachable!").to_owned(),
                    ERROR_ICON,
                ),
            );
        }
        state.server = down;
    }

    report
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use crate::config::NotificationConfig;
    use crate::domain::readings::{
        BatteryPeripheralReading, BatterySystemReading, DiskUsageReading, LoadAverage,
    };
    use crate::test_support::{FakeClock, FakeNotificationFacade};

    fn disabled_config() -> Config {
        Config {
            notifications: NotificationConfig {
                disk_usage: false,
                disk_smart: false,
                cpu_temp: false,
                gpu_nvidia_temp: false,
                hd_temp: false,
                battery_sys: false,
                battery_mouse: false,
                battery_kbd: false,
                server_check: false,
                load_avg: false,
            },
            ..Config::default()
        }
    }

    fn expected(body: &str, icon: &str) -> NotificationPayload {
        NotificationPayload {
            title: "PiroStats".to_owned(),
            body: body.to_owned(),
            icon: icon.to_owned(),
            urgency: NotificationUrgency::Critical,
            timeout: NotificationTimeout::Never,
        }
    }

    fn poll_cpu(
        cfg: &Config,
        state: &mut NotificationState,
        clock: &mut FakeClock,
        facade: &mut FakeNotificationFacade,
        temp: i32,
        advance: Duration,
    ) {
        let now = clock.advance(advance).monotonic;
        let readings = ReadingsSnapshot {
            cpu_temp: Some(temp),
            ..ReadingsSnapshot::default()
        };
        let _ = check_and_notify(
            &readings,
            cfg,
            state,
            &HardwareSnapshot::default(),
            now,
            facade,
        );
    }

    #[test]
    fn cpu_temp_spike_never_notifies() {
        let mut cfg = disabled_config();
        cfg.notifications.cpu_temp = true;
        let mut state = NotificationState::default();
        let mut clock = FakeClock::default();
        let mut facade = FakeNotificationFacade::new();

        for temp in [50, 82, 50, 84, 50, 91, 50] {
            poll_cpu(
                &cfg,
                &mut state,
                &mut clock,
                &mut facade,
                temp,
                Duration::from_millis(1500),
            );
        }

        assert!(facade.calls().is_empty());
    }

    #[test]
    fn cpu_temp_notifies_once_when_sustained() {
        let mut cfg = disabled_config();
        cfg.notifications.cpu_temp = true;
        let mut state = NotificationState::default();
        let mut clock = FakeClock::default();
        let mut facade = FakeNotificationFacade::new();

        for _ in 0..60 {
            poll_cpu(
                &cfg,
                &mut state,
                &mut clock,
                &mut facade,
                85,
                Duration::from_millis(1500),
            );
        }

        assert_eq!(facade.calls(), &[expected("Cpu temp 85C", "dialog-error")]);
    }

    #[test]
    fn cpu_temp_hysteresis_recovery_and_retrigger_match_python() {
        let mut cfg = disabled_config();
        cfg.notifications.cpu_temp = true;
        cfg.notify_thresholds.temp_sustain_seconds = 0;
        let mut state = NotificationState::default();
        let mut clock = FakeClock::default();
        let mut facade = FakeNotificationFacade::new();

        for temp in [85, 78, 81, 76, 82, 79, 74, 85] {
            poll_cpu(
                &cfg,
                &mut state,
                &mut clock,
                &mut facade,
                temp,
                Duration::from_secs(1),
            );
        }

        assert_eq!(facade.calls().len(), 2);
    }

    #[test]
    fn cpu_temp_hold_restarts_after_one_dip() {
        let mut cfg = disabled_config();
        cfg.notifications.cpu_temp = true;
        let mut state = NotificationState::default();
        let mut clock = FakeClock::default();
        let mut facade = FakeNotificationFacade::new();

        poll_cpu(
            &cfg,
            &mut state,
            &mut clock,
            &mut facade,
            85,
            Duration::ZERO,
        );
        poll_cpu(
            &cfg,
            &mut state,
            &mut clock,
            &mut facade,
            70,
            Duration::from_secs(59),
        );
        poll_cpu(
            &cfg,
            &mut state,
            &mut clock,
            &mut facade,
            85,
            Duration::from_secs(1),
        );
        poll_cpu(
            &cfg,
            &mut state,
            &mut clock,
            &mut facade,
            85,
            Duration::from_secs(59),
        );

        assert!(facade.calls().is_empty());
        poll_cpu(
            &cfg,
            &mut state,
            &mut clock,
            &mut facade,
            85,
            Duration::from_secs(1),
        );
        assert_eq!(facade.calls().len(), 1);
    }

    #[test]
    fn sustained_uses_elapsed_monotonic_time_and_fires_once() {
        let mut latch = NotificationLatch::default();
        assert!(!sustained(
            &mut latch,
            85.0,
            80.0,
            75.0,
            60.0,
            Duration::from_secs(1_000)
        ));
        assert!(sustained(
            &mut latch,
            85.0,
            80.0,
            75.0,
            60.0,
            Duration::from_secs(1_061)
        ));
        assert!(!sustained(
            &mut latch,
            85.0,
            80.0,
            75.0,
            60.0,
            Duration::from_secs(1_062)
        ));
    }

    #[test]
    fn sustained_without_hysteresis_clears_below_trip() {
        let mut latch = NotificationLatch::default();
        assert!(sustained(&mut latch, 1.0, 0.9, 0.9, 0.0, Duration::ZERO));
        assert!(!sustained(&mut latch, 0.89, 0.9, 0.9, 0.0, Duration::ZERO));
        assert!(!latch.active);
    }

    #[test]
    fn every_notification_type_emits_exact_ordered_payloads() {
        let mut cfg = Config::default();
        cfg.notifications.cpu_temp = true;
        cfg.notifications.gpu_nvidia_temp = true;
        cfg.notifications.load_avg = true;
        cfg.notifications.server_check = true;
        cfg.notify_thresholds.temp_sustain_seconds = 0;
        cfg.notify_thresholds.load_avg_minutes = 0;
        let readings = ReadingsSnapshot {
            cpu_temp: Some(80),
            gpu_temp: Some(80),
            disk_usage: [(
                "/".to_owned(),
                Some(DiskUsageReading {
                    percent: 80,
                    ..DiskUsageReading::default()
                }),
            )]
            .into(),
            disk_smart: [("nvme0".to_owned(), Some(false))].into(),
            hd_temps: [("nvme0".to_owned(), Some(60))].into(),
            battery_sys: vec![BatterySystemReading {
                id: "BAT0".to_owned(),
                charge_percent: 10,
                state: BatteryState::Discharging,
                ..BatterySystemReading::default()
            }],
            battery_mouse: Some(BatteryPeripheralReading {
                name: "MX Master".to_owned(),
                charge_percent: 19,
            }),
            battery_kbd: Some(BatteryPeripheralReading {
                name: String::new(),
                charge_percent: 19,
            }),
            load_average: Some(LoadAverage {
                fifteen: 7.2,
                ..LoadAverage::default()
            }),
            server_ok: Some(false),
            ..ReadingsSnapshot::default()
        };
        let hardware = HardwareSnapshot {
            cpu_count: 8,
            ..HardwareSnapshot::default()
        };
        let mut state = NotificationState::default();
        let mut facade = FakeNotificationFacade::new();

        let report = check_and_notify(
            &readings,
            &cfg,
            &mut state,
            &hardware,
            Duration::from_secs(100),
            &mut facade,
        );

        assert_eq!(report.attempted, 10);
        assert!(report.failures.is_empty());
        assert_eq!(
            facade.calls(),
            &[
                expected("Cpu temp 80C", "dialog-error"),
                expected("Gpu temp 80C", "dialog-error"),
                expected("Disk / 80%", "dialog-error"),
                expected("Disk nvme0 SMART check FAILED", "dialog-error"),
                expected("Disk nvme0 temp 60C", "dialog-warning"),
                expected("Battery 10%", "battery-caution"),
                expected("MX Master: 19%", "battery-caution"),
                expected("Keyboard: 19%", "battery-caution"),
                expected("Load avg 15m high for 0 min (7.20)", "dialog-warning"),
                expected("Server is not reachable!", "dialog-error"),
            ]
        );
    }

    #[test]
    fn threshold_boundaries_match_inclusive_and_exclusive_python_rules() {
        let mut cfg = disabled_config();
        cfg.notifications.disk_usage = true;
        cfg.notifications.battery_sys = true;
        cfg.notifications.battery_mouse = true;
        let readings = ReadingsSnapshot {
            disk_usage: [
                (
                    "/at".to_owned(),
                    Some(DiskUsageReading {
                        percent: 80,
                        ..DiskUsageReading::default()
                    }),
                ),
                (
                    "/below".to_owned(),
                    Some(DiskUsageReading {
                        percent: 79,
                        ..DiskUsageReading::default()
                    }),
                ),
            ]
            .into(),
            battery_sys: vec![BatterySystemReading {
                id: "BAT0".to_owned(),
                charge_percent: 10,
                state: BatteryState::Discharging,
                ..BatterySystemReading::default()
            }],
            battery_mouse: Some(BatteryPeripheralReading {
                name: String::new(),
                charge_percent: 20,
            }),
            ..ReadingsSnapshot::default()
        };
        let mut facade = FakeNotificationFacade::new();

        let _ = check_and_notify(
            &readings,
            &cfg,
            &mut NotificationState::default(),
            &HardwareSnapshot::default(),
            Duration::ZERO,
            &mut facade,
        );

        assert_eq!(
            facade.calls(),
            &[
                expected("Disk /at 80%", "dialog-error"),
                expected("Battery 10%", "battery-caution"),
            ]
        );
    }

    #[test]
    fn battery_charging_zero_and_disconnected_values_are_excluded() {
        let mut cfg = disabled_config();
        cfg.notifications.battery_sys = true;
        cfg.notifications.battery_mouse = true;
        cfg.notifications.battery_kbd = true;
        let readings = ReadingsSnapshot {
            battery_sys: vec![
                BatterySystemReading {
                    id: "charging".to_owned(),
                    charge_percent: 5,
                    state: BatteryState::Charging,
                    ..BatterySystemReading::default()
                },
                BatterySystemReading {
                    id: "zero".to_owned(),
                    charge_percent: 0,
                    state: BatteryState::Discharging,
                    ..BatterySystemReading::default()
                },
            ],
            battery_mouse: Some(BatteryPeripheralReading {
                name: String::new(),
                charge_percent: 0,
            }),
            battery_kbd: Some(BatteryPeripheralReading {
                name: String::new(),
                charge_percent: 0,
            }),
            ..ReadingsSnapshot::default()
        };
        let mut state = NotificationState::default();
        let mut facade = FakeNotificationFacade::new();

        let _ = check_and_notify(
            &readings,
            &cfg,
            &mut state,
            &HardwareSnapshot::default(),
            Duration::ZERO,
            &mut facade,
        );

        assert!(facade.calls().is_empty());
        assert_eq!(state.battery_sys.get("charging"), Some(&false));
        assert!(!state.battery_sys.contains_key("zero"));
    }

    #[test]
    fn device_latches_are_independent_and_removed_device_state_is_retained() {
        let mut cfg = disabled_config();
        cfg.notifications.hd_temp = true;
        cfg.notify_thresholds.temp_sustain_seconds = 0;
        let mut readings = ReadingsSnapshot {
            hd_temps: [("a".to_owned(), Some(61)), ("b".to_owned(), Some(62))].into(),
            ..ReadingsSnapshot::default()
        };
        let mut state = NotificationState::default();
        let mut facade = FakeNotificationFacade::new();

        let _ = check_and_notify(
            &readings,
            &cfg,
            &mut state,
            &HardwareSnapshot::default(),
            Duration::ZERO,
            &mut facade,
        );
        readings.hd_temps.clear();
        let _ = check_and_notify(
            &readings,
            &cfg,
            &mut state,
            &HardwareSnapshot::default(),
            Duration::from_secs(1),
            &mut facade,
        );
        readings.hd_temps.insert("a".to_owned(), Some(61));
        let _ = check_and_notify(
            &readings,
            &cfg,
            &mut state,
            &HardwareSnapshot::default(),
            Duration::from_secs(2),
            &mut facade,
        );

        assert_eq!(facade.calls().len(), 2);
        assert!(state.hd_temp.contains_key("a"));
        assert!(state.hd_temp.contains_key("b"));
    }

    #[test]
    fn simple_edges_recover_and_retrigger_per_device() {
        let mut cfg = disabled_config();
        cfg.notifications.disk_usage = true;
        let mut readings = ReadingsSnapshot::default();
        let mut state = NotificationState::default();
        let mut facade = FakeNotificationFacade::new();

        for percent in [80, 90, 79, 80] {
            readings.disk_usage.insert(
                "/".to_owned(),
                Some(DiskUsageReading {
                    percent,
                    ..DiskUsageReading::default()
                }),
            );
            let _ = check_and_notify(
                &readings,
                &cfg,
                &mut state,
                &HardwareSnapshot::default(),
                Duration::ZERO,
                &mut facade,
            );
        }

        assert_eq!(facade.calls().len(), 2);
    }

    #[test]
    fn disabled_and_absent_inputs_leave_state_silent_and_unchanged() {
        let cfg = disabled_config();
        let mut state = NotificationState {
            server: true,
            ..NotificationState::default()
        };
        state.disk.insert("removed".to_owned(), true);
        let before = state.clone();
        let mut facade = FakeNotificationFacade::new();

        let report = check_and_notify(
            &ReadingsSnapshot::default(),
            &cfg,
            &mut state,
            &HardwareSnapshot::default(),
            Duration::ZERO,
            &mut facade,
        );

        assert_eq!(state, before);
        assert_eq!(report, NotificationReport::default());
        assert!(facade.calls().is_empty());
    }

    #[test]
    fn facade_failure_is_reported_but_does_not_stop_or_rearm_processing() {
        let mut cfg = disabled_config();
        cfg.notifications.disk_usage = true;
        cfg.notifications.server_check = true;
        let readings = ReadingsSnapshot {
            disk_usage: [(
                "/".to_owned(),
                Some(DiskUsageReading {
                    percent: 90,
                    ..DiskUsageReading::default()
                }),
            )]
            .into(),
            server_ok: Some(false),
            ..ReadingsSnapshot::default()
        };
        let mut state = NotificationState::default();
        let mut facade = FakeNotificationFacade::new();
        facade.push_result(Err(NotificationError {
            detail: "service unavailable".to_owned(),
        }));

        let report = check_and_notify(
            &readings,
            &cfg,
            &mut state,
            &HardwareSnapshot::default(),
            Duration::ZERO,
            &mut facade,
        );
        let second = check_and_notify(
            &readings,
            &cfg,
            &mut state,
            &HardwareSnapshot::default(),
            Duration::from_secs(1),
            &mut facade,
        );

        assert_eq!(report.attempted, 2);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(
            report.failures[0].payload,
            expected("Disk / 90%", "dialog-error")
        );
        assert_eq!(second.attempted, 0);
        assert_eq!(facade.calls().len(), 2);
    }

    #[test]
    fn configured_labels_are_used_in_exact_payload_text() {
        let mut cfg = disabled_config();
        cfg.notifications.server_check = true;
        let mut notify_labels = Table::new();
        notify_labels.insert(
            "server_down".to_owned(),
            Value::String("Offline".to_owned()),
        );
        cfg.labels
            .insert("notify".to_owned(), Value::Table(notify_labels));
        let readings = ReadingsSnapshot {
            server_ok: Some(false),
            ..ReadingsSnapshot::default()
        };
        let mut facade = FakeNotificationFacade::new();

        let _ = check_and_notify(
            &readings,
            &cfg,
            &mut NotificationState::default(),
            &HardwareSnapshot::default(),
            Duration::ZERO,
            &mut facade,
        );

        assert_eq!(facade.calls(), &[expected("Offline", "dialog-error")]);
    }
}
