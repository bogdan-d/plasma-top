//! Edge-triggered desktop-notification state machine.

use std::time::Duration;

use toml::{Table, Value};

use crate::config::Config;
use crate::domain::boundary::{
    NotificationError, NotificationFacade, NotificationPayload, NotificationTimeout,
    NotificationUrgency,
};
use crate::domain::readings::{BatteryState, DisplaySnapshot, HardwareInventory};
use crate::domain::state::{NotificationLatch, NotificationState};

const TITLE: &str = "PlasmaTop";
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
    readings: &DisplaySnapshot,
    cfg: &Config,
    state: &mut NotificationState,
    hardware: &HardwareInventory,
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
mod tests;
