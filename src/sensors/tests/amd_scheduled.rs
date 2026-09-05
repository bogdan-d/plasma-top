use super::*;
use crate::domain::Metric;
use crate::domain::readings::{AmdGpuMemoryPaths, AmdGpuSource};
use crate::scheduler::{
    CompletionKind, ConfigGeneration, InventoryGeneration, JobId, JobKind, JobTicket, OwnerId,
    RunId, SourceIdentity,
};

fn fixture(tree: &TempTree) -> HardwareInventory {
    for (name, value) in [
        ("usage", "71"),
        ("codec", "23"),
        ("used", "1024"),
        ("total", "4096"),
        ("freq", "2700000000"),
        ("temp", "63000"),
        ("power", "42000000"),
        ("fan", "1800"),
    ] {
        tree.write(&format!("sys/amd/{name}"), value);
    }
    HardwareInventory {
        amd_gpu: Some(AmdGpuSource {
            pci_identity: String::from("0000:c3:00.0"),
            usage_path: Some(tree.sys().join("amd/usage")),
            codec_usage_path: Some(tree.sys().join("amd/codec")),
            memory_paths: Some(AmdGpuMemoryPaths {
                used: tree.sys().join("amd/used"),
                total: tree.sys().join("amd/total"),
            }),
            freq_path: Some(tree.sys().join("amd/freq")),
            temp_path: Some(tree.sys().join("amd/temp")),
            power_path: Some(tree.sys().join("amd/power")),
            fan_speed_path: Some(tree.sys().join("amd/fan")),
        }),
        ..HardwareInventory::default()
    }
}

fn ticket(kind: JobKind, metrics: &[Metric]) -> JobTicket {
    JobTicket {
        job: JobId::with_source(
            OwnerId::AmdGpu,
            kind,
            SourceIdentity::Device(String::from("amd:0000:c3:00.0")),
        ),
        metrics: metrics.iter().copied().collect(),
        run_id: RunId(1),
        config_generation: ConfigGeneration(1),
        inventory_generation: InventoryGeneration(1),
        history_deadline: None,
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    tree: &TempTree,
    owners: &mut TestOwners,
    hw: &mut HardwareInventory,
    cfg: &Config,
    readings: &mut DisplaySnapshot,
    ticket: &JobTicket,
    at: u64,
) -> crate::sensors::scheduled::JobExecution {
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut clock = || clock(at);
    let proc_root = tree.proc();
    let sys_root = tree.sys();
    let mut ctx = crate::sensors::CollectCtx {
        proc_root: &proc_root,
        sys_root: &sys_root,
        commands: &mut commands,
        dbus: &mut dbus,
        clock: &mut clock,
        nvml: None,
        bolt: None,
        skip_slow: false,
    };
    let result = crate::sensors::execute_scheduled_job(
        ticket,
        owners.refs(),
        hw,
        cfg,
        &mut ctx,
        readings,
        None,
    );
    assert!(commands.call_trace().is_empty());
    assert!(dbus.call_trace().is_empty());
    result
}

#[test]
fn grouped_attempts_read_only_demanded_fields_and_retain_failed_siblings() {
    let tree = TempTree::new();
    let mut hw = fixture(&tree);
    let mut owners = TestOwners::default();
    let cfg = amd_config();
    let mut readings = DisplaySnapshot::default();
    let usage = ticket(JobKind::AmdFast, &[Metric::GpuAmdUsage]);
    assert_eq!(
        run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &usage, 1).completion,
        CompletionKind::Captured
    );
    assert_eq!(readings.gpu_amd_usage, Some(71));
    assert_eq!(owners.amd_gpu.samples().memory.attempted_at, None);
    assert_eq!(owners.amd_gpu.samples().temperature.attempted_at, None);
    let fast = ticket(JobKind::AmdFast, crate::sensors::gpu_amd::FAST_METRICS);
    run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &fast, 2);
    assert_eq!(readings.gpu_amd_codec_usage, Some(23));
    assert_eq!(
        readings.gpu_amd_mem_usage.expect("VRAM").percent(),
        Some(25)
    );
    assert_eq!(readings.gpu_amd_freq, Some(2700));
    tree.write("sys/amd/usage", "broken");
    tree.write("sys/amd/codec", "64");
    let failed = run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &fast, 3);
    assert_eq!(failed.completion, CompletionKind::Failed);
    assert_eq!(readings.gpu_amd_usage, Some(71));
    assert_eq!(readings.gpu_amd_codec_usage, Some(64));
    assert_eq!(
        crate::sensors::scheduled_capture_time(&fast.job, owners.refs()),
        Some(Duration::from_secs(2))
    );
    assert!(!failed.notification_ready);
}

#[test]
fn temperature_candidates_are_new_captures_even_when_a_sibling_fails() {
    let tree = TempTree::new();
    let mut hw = fixture(&tree);
    let mut owners = TestOwners::default();
    let cfg = amd_config();
    let mut readings = DisplaySnapshot::default();
    let slow = ticket(JobKind::AmdSlow, crate::sensors::gpu_amd::SLOW_METRICS);
    let first = run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &slow, 1);
    assert_eq!(first.notifications.gpu_amd_temp, Some(63));
    assert!(first.notification_ready);
    assert_eq!(readings.gpu_amd_power, Some(42));
    assert_eq!(readings.gpu_amd_fan_speed, Some(1800));
    tree.write("sys/amd/temp", "bad");
    tree.write("sys/amd/power", "51000000");
    let failed = run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &slow, 2);
    assert_eq!(failed.completion, CompletionKind::Failed);
    assert_eq!(readings.gpu_amd_temp, Some(63));
    assert_eq!(readings.gpu_amd_power, Some(51));
    assert!(!failed.notification_ready);
    tree.write("sys/amd/temp", "65000");
    tree.write("sys/amd/power", "bad");
    let partial = run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &slow, 3);
    assert_eq!(partial.completion, CompletionKind::Failed);
    assert_eq!(partial.notifications.gpu_amd_temp, Some(65));
    assert!(partial.notification_ready);
    let power = ticket(JobKind::AmdSlow, &[Metric::GpuAmdPower]);
    assert!(!run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &power, 4).notification_ready);
}

#[test]
fn lifecycle_clears_removed_fields_and_groups_without_erasing_siblings() {
    let tree = TempTree::new();
    let mut hw = fixture(&tree);
    let mut owners = TestOwners::default();
    let mut cfg = amd_config();
    let mut readings = DisplaySnapshot::default();
    let fast = ticket(JobKind::AmdFast, crate::sensors::gpu_amd::FAST_METRICS);
    let slow = ticket(JobKind::AmdSlow, crate::sensors::gpu_amd::SLOW_METRICS);
    run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &fast, 1);
    run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &slow, 1);
    crate::sensors::invalidate_scheduled_job(&fast.job, owners.refs(), &mut readings);
    assert_eq!(readings.gpu_amd_usage, None);
    assert_eq!(readings.gpu_amd_temp, Some(63));
    assert_eq!(
        owners
            .amd_gpu
            .samples()
            .temperature
            .latest
            .as_ref()
            .expect("retained slow")
            .captured_at,
        Duration::from_secs(1)
    );
    hw.amd_gpu.as_mut().expect("AMD").power_path = None;
    run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &slow, 2);
    assert_eq!(readings.gpu_amd_power, None);
    assert_eq!(readings.gpu_amd_temp, Some(63));
    cfg.tooltip.sections.clear();
    cfg.panel.sections.clear();
    cfg.pages.order.clear();
    run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &slow, 3);
    assert_eq!(readings.gpu_amd_temp, None);
    cfg = amd_config();
    let old_source = hw.amd_gpu.take().expect("source");
    crate::sensors::invalidate_scheduled_job(&slow.job, owners.refs(), &mut readings);
    run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &slow, 4);
    hw.amd_gpu = Some(old_source);
    run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &slow, 5);
    assert_eq!(readings.gpu_amd_temp, Some(63));
    hw.amd_gpu.as_mut().expect("AMD").pci_identity = String::from("0000:01:00.0");
    let obsolete = run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &slow, 6);
    assert_eq!(obsolete.completion, CompletionKind::ConfirmedAbsent);
    assert!(owners.amd_gpu.samples().temperature.latest.is_none());
}

fn amd_config() -> Config {
    cfg_panel(&[
        "gpu_amd_usage",
        "gpu_amd_codec_usage",
        "gpu_amd_mem_usage",
        "gpu_amd_freq",
        "gpu_amd_temp",
        "gpu_amd_power",
        "gpu_amd_fan_speed",
    ])
}

#[test]
fn amd_history_deadline_selects_usage_and_codec_independently() {
    use crate::scheduler::{HistoryDeadline, SchedulerTime};
    let tree = TempTree::new();
    let mut hw = fixture(&tree);
    let mut owners = TestOwners::default();
    let mut cfg = amd_config();
    cfg.pages.order = vec![String::from("graphs")];
    let mut readings = DisplaySnapshot::default();
    let fast = ticket(JobKind::AmdFast, crate::sensors::gpu_amd::FAST_METRICS);
    run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &fast, 1);
    tree.write("sys/amd/usage", "broken");
    tree.write("sys/amd/codec", "64");
    run(&tree, &mut owners, &mut hw, &cfg, &mut readings, &fast, 3);
    let mut history = ticket(JobKind::GpuHistory, &[]);
    history.job.owner = OwnerId::GpuHistory;
    history.history_deadline = Some(HistoryDeadline::new(SchedulerTime::from_duration(
        Duration::from_secs(2),
    )));
    run(
        &tree,
        &mut owners,
        &mut hw,
        &cfg,
        &mut readings,
        &history,
        4,
    );
    assert_eq!(readings.gpu_usage_history, [71]);
    assert_eq!(readings.gpu_dec_history, [23]);
    history.history_deadline = Some(HistoryDeadline::new(SchedulerTime::from_duration(
        Duration::from_secs(4),
    )));
    run(
        &tree,
        &mut owners,
        &mut hw,
        &cfg,
        &mut readings,
        &history,
        5,
    );
    assert_eq!(readings.gpu_usage_history, [71, 71]);
    assert_eq!(readings.gpu_dec_history, [23, 64]);
    hw.amd_gpu.as_mut().expect("AMD").codec_usage_path = None;
    history.history_deadline = None;
    run(
        &tree,
        &mut owners,
        &mut hw,
        &cfg,
        &mut readings,
        &history,
        6,
    );
    assert_eq!(readings.gpu_dec_history, [23, 64]);
    assert_eq!(owners.gpu_history.latest_decoder, None);
}

#[test]
fn amd_alert_evaluation_uses_fresh_temperature_candidates_only() {
    use crate::domain::state::NotificationState;
    use crate::test_support::FakeNotificationFacade;
    let tree = TempTree::new();
    let mut hw = fixture(&tree);
    let mut owners = TestOwners::default();
    let mut cfg = amd_config();
    cfg.notifications.gpu_amd_temp = true;
    cfg.notify_thresholds.gpu_amd_temp = 60;
    cfg.notify_thresholds.temp_sustain_seconds = 1;
    let mut readings = DisplaySnapshot::default();
    let mut state = NotificationState::default();
    let mut facade = FakeNotificationFacade::new();
    let slow = ticket(JobKind::AmdSlow, crate::sensors::gpu_amd::SLOW_METRICS);
    let power = ticket(JobKind::AmdSlow, &[Metric::GpuAmdPower]);
    for (at, job, temp, power_value, expected) in [
        (1, &slow, "63000", "42000000", 0),
        (2, &slow, "broken", "42000000", 0),
        (3, &power, "65000", "42000000", 0),
        (4, &slow, "65000", "broken", 1),
    ] {
        tree.write("sys/amd/temp", temp);
        tree.write("sys/amd/power", power_value);
        let result = run(&tree, &mut owners, &mut hw, &cfg, &mut readings, job, at);
        let _ = crate::notify::check_and_notify(
            &result.notifications,
            &cfg,
            &mut state,
            &hw,
            Duration::from_secs(at),
            &mut facade,
        );
        assert_eq!(facade.calls().len(), expected, "at {at}");
    }
    assert_eq!(facade.calls()[0].body, "AMD GPU temperature 65C");
}
