use super::*;

use std::time::UNIX_EPOCH;

fn clock(seconds: u64) -> ClockSnapshot {
    ClockSnapshot {
        monotonic: Duration::from_secs(seconds),
        wall: UNIX_EPOCH,
    }
}

#[test]
fn history_prefers_nvidia_without_synthesizing_missing_decoder() {
    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs")];
    cfg.pages.graph_history_length = 2;
    cfg.display.history_interval = crate::domain::Cadence::from_millis(2000);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        intel_gpu_pci: Some(String::from("0000:00:02.0")),
        ..HardwareInventory::default()
    };
    let mut state = GpuHistoryState::default();
    let mut readings = DisplaySnapshot {
        gpu_usage: Some(70),
        gpu_dec: None,
        gpu_intel_usage: Some(20),
        gpu_intel_dec_usage: Some(10),
        ..DisplaySnapshot::default()
    };

    let first = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::Unmeasured,
        clock(10),
        true,
    );
    assert!(first.is_some());
    let Some(first) = first else { return };
    let Some(first_usage) = first.usage else {
        return;
    };
    assert_eq!(first_usage.value, vec![70]);
    assert!(first.decoder.is_none());
    assert!(state.decoder.is_empty());

    readings.gpu_usage = Some(71);
    let within = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::Unmeasured,
        clock(11),
        false,
    );
    assert!(within.is_some());
    let Some(within) = within else { return };
    let Some(within_usage) = within.usage else {
        return;
    };
    assert_eq!(within_usage.value, vec![70]);
    assert_eq!(within_usage.captured_at, first_usage.captured_at);
    readings.gpu_usage = Some(72);
    readings.gpu_dec = Some(4);
    let _ = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::Value(4),
        clock(12),
        true,
    );
    readings.gpu_usage = Some(73);
    let _ = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::TransientFailure,
        clock(14),
        true,
    );
    assert_eq!(state.usage, vec![72, 73]);
    assert_eq!(state.decoder, vec![4, 4]);

    hw.has_nvidia = false;
    readings.gpu_intel_usage = Some(22);
    readings.gpu_intel_dec_usage = Some(8);
    let selected = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::Value(8),
        clock(16),
        true,
    );
    assert!(selected.is_some());
    let Some(selected) = selected else { return };
    assert_eq!(selected.usage.map(|sample| sample.value), Some(vec![22]));
    assert_eq!(selected.decoder.map(|sample| sample.value), Some(vec![8]));
}

#[test]
fn history_gap_reexposes_buffer_and_disabled_page_does_nothing() {
    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs")];
    let hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut state = GpuHistoryState {
        source: Some(String::from("nvidia")),
        usage: vec![10, 20],
        decoder: vec![1, 2],
        latest_decoder: Some(2),
        sampled_at: Some(Duration::from_secs(5)),
        usage_sample: Some(MetricSample::new(vec![10, 20], Duration::from_secs(5))),
        decoder_sample: Some(MetricSample::new(vec![1, 2], Duration::from_secs(5))),
    };
    let readings = DisplaySnapshot::default();

    let history = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::TransientFailure,
        clock(10),
        false,
    );
    assert!(history.is_some());
    let Some(history) = history else { return };
    let Some(usage) = history.usage else {
        return;
    };
    assert_eq!(usage.value, vec![10, 20]);
    assert_eq!(history.decoder.map(|sample| sample.value), Some(vec![1, 2]));
    assert_eq!(usage.captured_at, Duration::from_secs(5));

    cfg.pages.order.clear();
    assert!(
        sample_gpu_history(
            &mut state,
            &cfg,
            &hw,
            &readings,
            DecoderOutcome::Unmeasured,
            clock(20),
            false,
        )
        .is_none()
    );
}

#[test]
fn initial_missing_gpu_sample_does_not_manufacture_history_samples() {
    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs")];
    let hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut state = GpuHistoryState::default();

    let result = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &DisplaySnapshot::default(),
        DecoderOutcome::Unmeasured,
        clock(10),
        true,
    );
    assert!(result.is_some());
    let Some(result) = result else { return };

    assert!(result.usage.is_none());
    assert!(result.decoder.is_none());
    assert!(state.usage.is_empty());
    assert!(state.decoder.is_empty());
    assert_eq!(state.sampled_at, Some(Duration::from_secs(10)));
}

#[test]
fn confirmed_gpu_removal_invalidates_history_owner() {
    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs")];
    let hw = HardwareInventory::default();
    let mut state = GpuHistoryState {
        source: Some(String::from("nvidia")),
        usage: vec![50],
        sampled_at: Some(Duration::from_secs(5)),
        usage_sample: Some(MetricSample::new(vec![50], Duration::from_secs(5))),
        ..GpuHistoryState::default()
    };

    let result = state.sample(
        &cfg,
        &hw,
        &DisplaySnapshot::default(),
        DecoderOutcome::Unmeasured,
        clock(10),
        true,
    );

    assert!(result.is_none());
    assert_eq!(state, GpuHistoryState::default());
}

#[test]
fn decoder_history_carries_failure_then_stops_after_confirmed_absence() {
    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs")];
    let hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut state = GpuHistoryState::default();
    let mut readings = DisplaySnapshot::default();

    let Some(initial) = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::Unmeasured,
        clock(10),
        true,
    ) else {
        panic!("expected GPU history owner");
    };
    assert!(initial.decoder.is_none());
    assert!(state.decoder.is_empty());

    readings.gpu_dec = Some(7);
    let _ = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::Value(7),
        clock(11),
        false,
    );
    readings.gpu_dec = None;
    let Some(first) = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::TransientFailure,
        clock(12),
        true,
    ) else {
        panic!("expected GPU history owner");
    };
    assert_eq!(first.decoder.map(|sample| sample.value), Some(vec![7]));
    assert!(first.usage.is_none());

    let Some(carried) = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::TransientFailure,
        clock(14),
        true,
    ) else {
        panic!("expected GPU history owner");
    };
    assert_eq!(carried.decoder.map(|sample| sample.value), Some(vec![7, 7]));
    assert!(state.usage.is_empty());

    let Some(absent) = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::ConfirmedAbsent,
        clock(16),
        true,
    ) else {
        panic!("expected GPU history owner");
    };
    assert_eq!(absent.decoder.map(|sample| sample.value), Some(vec![7, 7]));
    assert_eq!(state.latest_decoder, None);

    let _ = sample_gpu_history(
        &mut state,
        &cfg,
        &hw,
        &readings,
        DecoderOutcome::TransientFailure,
        clock(18),
        true,
    );
    assert_eq!(state.decoder, vec![7, 7]);
}
