use super::*;

use crate::sensors::gpu_nvidia::{NvmlMetrics, NvmlOptionalMetric};

fn mandatory(temp: i32) -> NvmlMetrics {
    NvmlMetrics {
        temp_celsius: temp,
        usage_percent: 40,
        memory_percent: 20,
        decoder: NvmlOptionalMetric::NotSupported,
        fan: NvmlOptionalMetric::NotSupported,
    }
}

fn enqueue_smi(commands: &mut FakeCommandRunner, stdout: &str) {
    commands.enqueue(
        "nvidia-smi",
        [
            "--query-gpu=temperature.gpu,utilization.gpu,utilization.memory,fan.speed,utilization.decoder",
            "--format=csv,noheader,nounits",
        ],
        ok_cmd(
            "nvidia-smi",
            &[
                "--query-gpu=temperature.gpu,utilization.gpu,utilization.memory,fan.speed,utilization.decoder",
                "--format=csv,noheader,nounits",
            ],
            stdout,
        ),
    );
}

#[allow(clippy::too_many_arguments)]
fn run_nvidia_output<'a>(
    lanes: &'a mut TestOwners,
    hw: &'a mut HardwareInventory,
    cfg: &'a Config,
    tree: &'a TempTree,
    commands: &'a mut FakeCommandRunner,
    dbus: &'a mut FakeDbus,
    nvml: Option<&'a mut FakeNvml>,
    at: ClockSnapshot,
) -> crate::sensors::CollectionOutput {
    let mut capture_clock = || at;
    let proc_root = tree.proc();
    let sys_root = tree.sys();
    let mut ctx = crate::sensors::CollectCtx {
        proc_root: &proc_root,
        sys_root: &sys_root,
        commands,
        dbus,
        nvml: nvml.map(|facade| facade as &mut dyn NvmlFacade),
        bolt: None,
        clock: &mut capture_clock,
        skip_slow: false,
    };
    collect_with_notifications(lanes.refs(), hw, cfg, &mut ctx, None)
}

#[test]
fn transient_nvml_failure_throttles_fallback_and_keeps_source_diagnostics() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    enqueue_smi(&mut commands, "60, 50, 30, 40, 5\n");
    let mut dbus = FakeDbus::new();
    let mut nvml = FakeNvml::typed(vec![
        Err(NvmlError::Read),
        Err(NvmlError::Read),
        Ok(mandatory(52)),
    ]);

    let first = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(0),
        false,
    );
    let throttled = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(1),
        false,
    );
    let recovered = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(2),
        false,
    );

    assert_eq!(first.gpu_temp, Some(60));
    assert_eq!(throttled.gpu_temp, Some(60));
    assert_eq!(recovered.gpu_temp, Some(52));
    assert_eq!(commands.call_trace().len(), 1);
    assert_eq!(
        lanes.nvidia.cache.nvml_failed_at,
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        lanes.nvidia.cache.fallback_attempted_at,
        Some(Duration::ZERO)
    );
    assert_eq!(lanes.nvidia.cache.fallback_failed_at, None);
}

#[test]
fn successful_fallback_does_not_erase_same_tick_nvml_failure() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    enqueue_smi(&mut commands, "61, 50, 30, 40, 5\n");
    let mut dbus = FakeDbus::new();
    let mut nvml = FakeNvml::typed(vec![Err(NvmlError::Read)]);

    let output = run_nvidia_output(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree,
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        clock(7),
    );

    assert_eq!(output.display.gpu_temp, Some(61));
    assert_eq!(output.notifications.gpu_temp, Some(61));
    assert_eq!(
        lanes.nvidia.cache.nvml_failed_at,
        Some(Duration::from_secs(7))
    );
    assert_eq!(
        lanes.nvidia.cache.fallback_attempted_at,
        Some(Duration::from_secs(7))
    );
    assert_eq!(lanes.nvidia.cache.fallback_failed_at, None);
    assert_eq!(lanes.nvidia.cache.failed_at, Some(Duration::from_secs(7)));
}

#[test]
fn cached_fallback_failure_keeps_retained_display_out_of_notifications() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    enqueue_smi(&mut commands, "60, 50, 30, 40, 5\n");
    let mut dbus = FakeDbus::new();

    let captured = run_nvidia_output(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree,
        &mut commands,
        &mut dbus,
        None,
        clock(0),
    );
    let failed = run_nvidia_output(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree,
        &mut commands,
        &mut dbus,
        None,
        clock(3),
    );
    let cached = run_nvidia_output(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree,
        &mut commands,
        &mut dbus,
        None,
        clock(4),
    );

    assert_eq!(captured.notifications.gpu_temp, Some(60));
    assert_eq!(failed.display.gpu_temp, Some(60));
    assert!(failed.notifications.gpu_temp.is_none());
    assert_eq!(cached.display.gpu_temp, Some(60));
    assert!(cached.notifications.gpu_temp.is_none());
    assert!(lanes.nvidia.cache.fallback_latest_attempt_failed);
}

#[test]
fn confirmed_nvidia_remove_readd_resets_backend_selection() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    enqueue_smi(&mut commands, "60, 50, 30, 40, 5\n");
    let mut dbus = FakeDbus::new();
    let mut nvml = FakeNvml::typed(vec![Err(NvmlError::Init), Ok(mandatory(48))]);
    let _ = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(0),
        false,
    );
    assert!(lanes.nvidia.cache.nvml_init_failed);

    hw.has_nvidia = false;
    let _ = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(1),
        false,
    );
    assert!(!lanes.nvidia.cache.nvml_init_failed);

    hw.has_nvidia = true;
    let readded = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(2),
        false,
    );
    assert_eq!(readded.gpu_temp, Some(48));
    assert_eq!(nvml.calls, 2);
}

#[test]
fn nvml_optional_failures_retain_success_and_not_supported_clears_it() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_dec_usage", "gpu_nvidia_fan_speed"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut nvml = FakeNvml::typed(vec![
        Ok(NvmlMetrics {
            decoder: NvmlOptionalMetric::Value(12),
            fan: NvmlOptionalMetric::Value(34),
            ..mandatory(50)
        }),
        Ok(NvmlMetrics {
            decoder: NvmlOptionalMetric::Failed,
            fan: NvmlOptionalMetric::Failed,
            ..mandatory(51)
        }),
        Ok(mandatory(52)),
    ]);

    let first = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(0),
        false,
    );
    let failed = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(1),
        false,
    );
    let unsupported = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(2),
        false,
    );

    assert_eq!((first.gpu_dec, first.gpu_fan), (Some(12), Some(34)));
    assert_eq!((failed.gpu_dec, failed.gpu_fan), (Some(12), Some(34)));
    assert_eq!(
        lanes.nvidia.cache.decoder.failed_at,
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        lanes.nvidia.cache.fan.failed_at,
        Some(Duration::from_secs(1))
    );
    assert_eq!((unsupported.gpu_dec, unsupported.gpu_fan), (None, None));
    assert!(lanes.nvidia.cache.decoder.latest.is_none());
    assert!(lanes.nvidia.cache.fan.latest.is_none());
}

#[test]
fn fallback_optional_values_seed_nvml_retention_and_not_supported_clears() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut cfg = cfg_panel(&["gpu_nvidia_dec_usage", "gpu_nvidia_fan_speed"]);
    cfg.pages.order = vec![String::from("graphs")];
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    enqueue_smi(&mut commands, "60, 50, 30, 40, 5\n");
    let mut dbus = FakeDbus::new();

    let fallback = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(0),
        false,
    );
    let mut nvml = FakeNvml::typed(vec![
        Ok(NvmlMetrics {
            decoder: NvmlOptionalMetric::Failed,
            fan: NvmlOptionalMetric::Failed,
            ..mandatory(51)
        }),
        Ok(mandatory(52)),
    ]);
    let retained = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(1),
        false,
    );
    assert_eq!((fallback.gpu_dec, fallback.gpu_fan), (Some(5), Some(40)));
    assert_eq!((retained.gpu_dec, retained.gpu_fan), (Some(5), Some(40)));
    assert_eq!(retained.gpu_dec_history, vec![5, 5]);
    assert_eq!(
        lanes.nvidia.cache.decoder.failed_at,
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        lanes.nvidia.cache.fan.failed_at,
        Some(Duration::from_secs(1))
    );

    let cleared = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        Some(&mut nvml),
        None,
        clock(2),
        false,
    );
    assert_eq!((cleared.gpu_dec, cleared.gpu_fan), (None, None));
    assert_eq!(cleared.gpu_dec_history, vec![5, 5]);
}
