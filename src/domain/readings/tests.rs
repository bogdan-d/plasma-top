use super::*;

#[test]
fn battery_state_tokens_match_python_contract() {
    assert_eq!(BatteryState::Unknown.as_str(), "");
    assert_eq!(BatteryState::Charging.as_str(), "charging");
    assert_eq!(BatteryState::Discharging.as_str(), "discharging");
    assert_eq!(BatteryState::FullyCharged.as_str(), "fully-charged");
}

#[test]
fn smart_disk_interface_tokens_match_python_contract() {
    assert_eq!(DiskSmartInterface::Ata.as_str(), "ata");
    assert_eq!(DiskSmartInterface::Nvme.as_str(), "nvme");
}

#[test]
fn hardware_inventory_default_is_a_safe_empty_machine() {
    let hardware = HardwareInventory::default();

    assert!(hardware.capabilities.is_empty());
    assert!(hardware.metrics.is_empty());
    assert_eq!(hardware.cpu_count, 1);
    assert!(hardware.hd_temp_paths.is_empty());
    assert!(hardware.disk_smart_drives.is_empty());
}

#[test]
fn display_snapshot_default_starts_empty_at_zero_time() {
    let readings = DisplaySnapshot::default();

    assert_eq!(readings.assembled_at, ClockSnapshot::default());
    assert!(readings.metrics.is_empty());
    assert!(readings.cpu_history.is_empty());
    assert!(readings.disk_usage.is_empty());
    assert!(readings.battery_sys.is_empty());
    assert_eq!(readings.server_ok, None);
}

#[test]
fn metric_sample_keeps_value_and_capture_time() {
    let sample = MetricSample::new(42, Duration::from_millis(125));

    assert_eq!(sample.value, 42);
    assert_eq!(sample.captured_at, Duration::from_millis(125));
}

#[test]
fn retained_metric_sample_keeps_valid_value_after_failure() {
    let mut sample = RetainedMetricSample::default();
    sample.record(Some(42), Duration::from_secs(1));
    sample.record(None, Duration::from_secs(2));

    assert_eq!(
        sample.latest,
        Some(MetricSample::new(42, Duration::from_secs(1)))
    );
    assert_eq!(sample.attempted_at, Some(Duration::from_secs(2)));
    assert_eq!(sample.failed_at, Some(Duration::from_secs(2)));
}
