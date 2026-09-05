#![allow(clippy::expect_used)]

use super::*;
use crate::domain::Metric;
use crate::domain::readings::AmdGpuSource;
use crate::scheduler::{
    ConfigGeneration, HistoryDeadline, InventoryGeneration, RunId, SchedulerTime,
};
use crate::test_support::{FakeCommandRunner, FakeDbus};

fn input(kind: JobKind, hw: &HardwareInventory) -> JobInput {
    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs")];
    JobInput {
        ticket: JobTicket {
            job: JobId::with_source(
                if kind == JobKind::GpuHistory {
                    OwnerId::GpuHistory
                } else {
                    OwnerId::AmdGpu
                },
                kind,
                SourceIdentity::Device(String::from("amd:0000:c3:00.0")),
            ),
            metrics: [Metric::GpuAmdUsage, Metric::GpuAmdCodecUsage].into(),
            run_id: RunId(1),
            config_generation: ConfigGeneration(1),
            inventory_generation: InventoryGeneration(1),
            history_deadline: None,
        },
        cfg,
        hw: hw.clone(),
        readings: DisplaySnapshot::default(),
        active: Vec::new(),
        selected_index: 0,
        css: String::new(),
        style_generation: 1,
        render_generation: 1,
        resolved_mounts: Vec::new(),
        gpu_decoder_outcome: gpu_history::DecoderOutcome::Unmeasured,
        gpu_history_point: None,
    }
}

#[test]
fn amd_worker_partial_failure_feeds_codec_history_without_slow_interference() {
    let root = std::env::temp_dir().join(format!("plasma-amd04-worker-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("fixture root");
    let usage = root.join("usage");
    let codec = root.join("codec");
    std::fs::write(&usage, "71").expect("usage fixture");
    std::fs::write(&codec, "23").expect("codec fixture");
    let hw = HardwareInventory {
        amd_gpu: Some(AmdGpuSource {
            pci_identity: String::from("0000:c3:00.0"),
            usage_path: Some(usage.clone()),
            codec_usage_path: Some(codec.clone()),
            ..AmdGpuSource::default()
        }),
        ..HardwareInventory::default()
    };
    let clock = ProductionClock::default();
    let roots = FilesystemRoots::default();
    let mut worker = WorkerState::new(OwnerId::AmdGpu, FakeCommandRunner::new(), FakeDbus::new());
    let first = worker.execute(input(JobKind::AmdFast, &hw), &roots, &clock);
    let first_point = first.gpu_history_point.expect("first history point");
    assert_eq!(
        first_point.1.value,
        (Some(71), Some(23), gpu_history::DecoderOutcome::Value(23))
    );
    std::fs::write(&usage, "broken").expect("failed usage");
    std::fs::write(&codec, "64").expect("new codec");
    let second = worker.execute(input(JobKind::AmdFast, &hw), &roots, &clock);
    assert_eq!(second.completion, CompletionKind::Failed);
    assert_eq!(
        second.decoder_outcome,
        Some(gpu_history::DecoderOutcome::Value(64))
    );
    let second_point = second.gpu_history_point.expect("partial history point");
    assert_eq!(second_point.1.value.0, Some(71));
    assert_eq!(second_point.1.value.1, Some(64));
    assert!(second_point.1.captured_at > first_point.1.captured_at);
    let slow = worker.execute(input(JobKind::AmdSlow, &hw), &roots, &clock);
    assert!(slow.decoder_outcome.is_none());
    assert!(slow.gpu_history_point.is_none());

    let mut history = WorkerState::new(
        OwnerId::GpuHistory,
        FakeCommandRunner::new(),
        FakeDbus::new(),
    );
    let mut eligible = input(JobKind::GpuHistory, &hw);
    eligible.readings = second.readings.clone();
    eligible.gpu_history_point = Some(first_point.1.clone());
    eligible.ticket.history_deadline = Some(HistoryDeadline::new(SchedulerTime::from_duration(
        first_point.1.captured_at,
    )));
    let result = history.execute(eligible, &roots, &clock);
    assert_eq!(result.readings.gpu_usage_history, [71]);
    assert_eq!(result.readings.gpu_dec_history, [23]);
    let mut eligible = input(JobKind::GpuHistory, &hw);
    eligible.gpu_history_point = Some(second_point.1);
    let result = history.execute(eligible, &roots, &clock);
    assert_eq!(result.readings.gpu_dec_history, [23, 64]);
    let mut removed = hw.clone();
    removed.amd_gpu.as_mut().expect("AMD").codec_usage_path = None;
    let result = history.execute(input(JobKind::GpuHistory, &removed), &roots, &clock);
    assert_eq!(result.readings.gpu_dec_history, [23, 64]);
    assert_eq!(history.owners.gpu_history.latest_decoder, None);
    std::fs::remove_dir_all(root).expect("fixture cleanup");
}

#[test]
fn amd_history_worker_does_not_leak_future_values_into_first_deadline() {
    let hw = HardwareInventory {
        amd_gpu: Some(AmdGpuSource {
            pci_identity: String::from("0000:c3:00.0"),
            codec_usage_path: Some("/amd/codec".into()),
            ..AmdGpuSource::default()
        }),
        ..HardwareInventory::default()
    };
    let mut input = input(JobKind::GpuHistory, &hw);
    input.readings.gpu_amd_usage = Some(99);
    input.readings.gpu_amd_codec_usage = Some(88);
    input.gpu_decoder_outcome = gpu_history::DecoderOutcome::Value(88);
    input.ticket.history_deadline = Some(HistoryDeadline::new(SchedulerTime::ZERO));
    let mut worker = WorkerState::new(
        OwnerId::GpuHistory,
        FakeCommandRunner::new(),
        FakeDbus::new(),
    );
    let result = worker.execute(
        input,
        &FilesystemRoots::default(),
        &ProductionClock::default(),
    );
    assert!(result.readings.gpu_usage_history.is_empty());
    assert!(result.readings.gpu_dec_history.is_empty());
}
