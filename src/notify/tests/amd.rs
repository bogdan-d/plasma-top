use super::*;

#[test]
fn amd_and_nvidia_alerts_have_independent_timers_hysteresis_and_delivery() {
    let mut cfg = disabled_config();
    cfg.notifications.gpu_nvidia_temp = true;
    cfg.notifications.gpu_amd_temp = true;
    cfg.notify_thresholds.gpu_nvidia_temp = 90;
    cfg.notify_thresholds.gpu_amd_temp = 80;
    cfg.notify_thresholds.temp_sustain_seconds = 10;
    cfg.notify_thresholds.temp_hysteresis = 5;
    cfg.labels
        .insert("gpu_amd_temp".into(), Value::String("AMD edge".into()));
    let mut state = NotificationState::default();
    let mut facade = FakeNotificationFacade::new();
    for (at, nvidia, amd, count) in [
        (0, Some(90), Some(79), 0),
        (5, Some(90), Some(80), 0),
        (10, Some(90), Some(80), 1),
        (15, None, None, 1),
        (20, Some(90), Some(80), 2),
        (21, Some(85), Some(75), 2),
        (22, Some(90), Some(74), 2),
        (23, Some(90), Some(81), 2),
        (33, Some(90), Some(81), 3),
    ] {
        if at == 10 {
            facade.push_result(Err(NotificationError {
                detail: "offline".into(),
            }));
        }
        let report = check_and_notify(
            &DisplaySnapshot {
                gpu_temp: nvidia,
                gpu_amd_temp: amd,
                ..DisplaySnapshot::default()
            },
            &cfg,
            &mut state,
            &HardwareInventory::default(),
            Duration::from_secs(at),
            &mut facade,
        );
        assert_eq!(facade.calls().len(), count, "at {at}");
        assert_eq!(report.failures.len(), usize::from(at == 10));
    }
    assert_eq!(
        facade.calls(),
        &[
            expected("Gpu temp 90C", "dialog-error"),
            expected("AMD edge 80C", "dialog-error"),
            expected("AMD edge 81C", "dialog-error"),
        ]
    );
    assert!(state.gpu_nvidia_temp.active);
    assert!(state.gpu_amd_temp.active);
}

#[test]
fn amd_enable_and_reading_are_independent_of_legacy_gpu_temperature() {
    let mut cfg = disabled_config();
    cfg.notify_thresholds.temp_sustain_seconds = 0;
    let mut state = NotificationState::default();
    let mut facade = FakeNotificationFacade::new();
    let readings = DisplaySnapshot {
        gpu_temp: Some(100),
        gpu_amd_temp: Some(80),
        ..DisplaySnapshot::default()
    };
    let _ = check_and_notify(
        &readings,
        &cfg,
        &mut state,
        &HardwareInventory::default(),
        Duration::ZERO,
        &mut facade,
    );
    assert!(facade.calls().is_empty());
    cfg.notifications.gpu_amd_temp = true;
    let _ = check_and_notify(
        &DisplaySnapshot {
            gpu_temp: Some(100),
            ..DisplaySnapshot::default()
        },
        &cfg,
        &mut state,
        &HardwareInventory::default(),
        Duration::ZERO,
        &mut facade,
    );
    assert!(facade.calls().is_empty());
    let _ = check_and_notify(
        &readings,
        &cfg,
        &mut state,
        &HardwareInventory::default(),
        Duration::ZERO,
        &mut facade,
    );
    assert_eq!(
        facade.calls(),
        &[expected("AMD GPU temperature 80C", "dialog-error")]
    );
    assert!(!state.gpu_nvidia_temp.active);
}
