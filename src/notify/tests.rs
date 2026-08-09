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
        title: "PlasmaTop".to_owned(),
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
    let readings = DisplaySnapshot {
        cpu_temp: Some(temp),
        ..DisplaySnapshot::default()
    };
    let _ = check_and_notify(
        &readings,
        cfg,
        state,
        &HardwareInventory::default(),
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
    let readings = DisplaySnapshot {
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
        ..DisplaySnapshot::default()
    };
    let hardware = HardwareInventory {
        cpu_count: 8,
        ..HardwareInventory::default()
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
    let readings = DisplaySnapshot {
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
        ..DisplaySnapshot::default()
    };
    let mut facade = FakeNotificationFacade::new();

    let _ = check_and_notify(
        &readings,
        &cfg,
        &mut NotificationState::default(),
        &HardwareInventory::default(),
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
    let readings = DisplaySnapshot {
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
        ..DisplaySnapshot::default()
    };
    let mut state = NotificationState::default();
    let mut facade = FakeNotificationFacade::new();

    let _ = check_and_notify(
        &readings,
        &cfg,
        &mut state,
        &HardwareInventory::default(),
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
    let mut readings = DisplaySnapshot {
        hd_temps: [("a".to_owned(), Some(61)), ("b".to_owned(), Some(62))].into(),
        ..DisplaySnapshot::default()
    };
    let mut state = NotificationState::default();
    let mut facade = FakeNotificationFacade::new();

    let _ = check_and_notify(
        &readings,
        &cfg,
        &mut state,
        &HardwareInventory::default(),
        Duration::ZERO,
        &mut facade,
    );
    readings.hd_temps.clear();
    let _ = check_and_notify(
        &readings,
        &cfg,
        &mut state,
        &HardwareInventory::default(),
        Duration::from_secs(1),
        &mut facade,
    );
    readings.hd_temps.insert("a".to_owned(), Some(61));
    let _ = check_and_notify(
        &readings,
        &cfg,
        &mut state,
        &HardwareInventory::default(),
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
    let mut readings = DisplaySnapshot::default();
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
            &HardwareInventory::default(),
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
        &DisplaySnapshot::default(),
        &cfg,
        &mut state,
        &HardwareInventory::default(),
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
    let readings = DisplaySnapshot {
        disk_usage: [(
            "/".to_owned(),
            Some(DiskUsageReading {
                percent: 90,
                ..DiskUsageReading::default()
            }),
        )]
        .into(),
        server_ok: Some(false),
        ..DisplaySnapshot::default()
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
        &HardwareInventory::default(),
        Duration::ZERO,
        &mut facade,
    );
    let second = check_and_notify(
        &readings,
        &cfg,
        &mut state,
        &HardwareInventory::default(),
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
    let readings = DisplaySnapshot {
        server_ok: Some(false),
        ..DisplaySnapshot::default()
    };
    let mut facade = FakeNotificationFacade::new();

    let _ = check_and_notify(
        &readings,
        &cfg,
        &mut NotificationState::default(),
        &HardwareInventory::default(),
        Duration::ZERO,
        &mut facade,
    );

    assert_eq!(facade.calls(), &[expected("Offline", "dialog-error")]);
}
