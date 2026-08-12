use super::*;

#[test]
fn selected_process_diagnostic_runs_exactly_baseline_then_warm_scan() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("proc/42/stat", &proc_stat_line(42, "worker", 10, 5, 100));
    let mut cfg = Config::default();
    cfg.panel.sections.clear();
    cfg.tooltip.sections.clear();
    cfg.pages.order = vec![String::from("processes")];
    cfg.notifications.disk_usage = false;
    cfg.notifications.disk_smart = false;
    cfg.notifications.hd_temp = false;
    cfg.notifications.battery_sys = false;
    cfg.notifications.battery_mouse = false;
    cfg.notifications.battery_kbd = false;
    let mut hw = HardwareInventory::default();
    let mut owners = TestOwners::default();
    let mut schedule = crate::sensors::SerialSchedule::new(
        &cfg,
        &hw,
        &tree.proc(),
        crate::scheduler::SchedulerTime::ZERO,
        crate::scheduler::PageId::Processes,
    );
    let now = std::cell::Cell::new(Duration::ZERO);
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut capture_clock = || ClockSnapshot {
        monotonic: now.get(),
        wall: UNIX_EPOCH + now.get(),
    };
    let proc_root = tree.proc();
    let sys_root = tree.sys();
    let mut ctx = crate::sensors::CollectCtx {
        proc_root: &proc_root,
        sys_root: &sys_root,
        commands: &mut commands,
        dbus: &mut dbus,
        nvml: None,
        bolt: None,
        clock: &mut capture_clock,
        skip_slow: false,
    };
    let mut process_starts = 0_usize;
    let mut process_captures = Vec::new();
    let mut observe_start = |job: &crate::scheduler::JobId| {
        if job.kind == crate::scheduler::JobKind::PageProcesses {
            process_starts = process_starts.saturating_add(1);
        }
    };

    let readings = crate::diagnostics::capture_diagnostic_baseline_and_warm(
        || {
            let readings = schedule.sample(
                owners.refs(),
                &mut hw,
                &cfg,
                &mut ctx,
                None,
                Some(&mut observe_start),
            );
            process_captures.push(readings.top_process_full.is_some());
            readings
        },
        || {
            now.set(Duration::from_secs(1));
            tree.write("proc/42/stat", &proc_stat_line(42, "worker", 20, 5, 100));
        },
    );

    assert_eq!(process_starts, 2);
    assert_eq!(process_captures, [false, true]);
    assert!(readings.top_process_full.is_some());
}

#[test]
fn cpu_composite_retains_captures_on_baseline_and_partial_failure() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/hwmon/hwmon0/temp1_input", "52000\n");
    let cfg = cfg_panel(&["cpu_temp", "load_avg"]);
    let mut hw = HardwareInventory {
        cpu_temp_path: Some(tree.root.join("sys/class/hwmon/hwmon0/temp1_input")),
        ..HardwareInventory::default()
    };
    let mut owners = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let now = std::cell::Cell::new(1);
    let mut capture_clock = || clock(now.get());
    let proc_root = tree.proc();
    let sys_root = tree.sys();
    let mut ctx = crate::sensors::CollectCtx {
        proc_root: &proc_root,
        sys_root: &sys_root,
        commands: &mut commands,
        dbus: &mut dbus,
        nvml: None,
        bolt: None,
        clock: &mut capture_clock,
        skip_slow: false,
    };
    let job = crate::scheduler::JobId::singleton(
        crate::scheduler::OwnerId::Cpu,
        crate::scheduler::JobKind::Cpu,
    );
    let mut readings = DisplaySnapshot::default();
    let baseline = crate::sensors::execute_scheduled_job(
        &job,
        owners.refs(),
        &mut hw,
        &cfg,
        &mut ctx,
        &mut readings,
        None,
    );
    assert_eq!(
        baseline.completion,
        crate::scheduler::CompletionKind::Baseline
    );
    assert!(baseline.notification_ready);
    assert_eq!(
        crate::sensors::scheduled_capture_time(&job, owners.refs()),
        Some(Duration::from_secs(1))
    );

    tree.write("proc/stat", "malformed\n");
    now.set(2);
    let failed = crate::sensors::execute_scheduled_job(
        &job,
        owners.refs(),
        &mut hw,
        &cfg,
        &mut ctx,
        &mut readings,
        None,
    );

    assert_eq!(failed.completion, crate::scheduler::CompletionKind::Failed);
    assert!(failed.notification_ready);
    assert_eq!(failed.notifications.cpu_temp, Some(52));
    assert_eq!(
        failed.notifications.load_average.map(|load| load.one),
        Some(0.5)
    );
    assert_eq!(
        crate::sensors::scheduled_capture_time(&job, owners.refs()),
        Some(Duration::from_secs(2))
    );
}

#[test]
fn captured_route_switch_from_wifi_to_wired_clears_wireless_display_fields() {
    let tree = TempTree::new();
    tree.mkdir("sys/class/net/wlan0/wireless");
    tree.mkdir("sys/class/net/eth0");
    let cfg = cfg_panel(&["net_device", "wifi_ssid_signal"]);
    let mut hw = HardwareInventory::default();
    let mut owners = TestOwners::default();
    let mut readings = DisplaySnapshot::default();
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev wlan0 src 10.0.0.5\n",
        ),
    );
    commands.enqueue(
        "iw",
        ["dev", "wlan0", "link"],
        ok_cmd(
            "iw",
            &["dev", "wlan0", "link"],
            "SSID: Home\n\tsignal: -60 dBm\n",
        ),
    );
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev eth0 src 192.0.2.2\n",
        ),
    );
    let mut dbus = FakeDbus::new();
    let now = std::cell::Cell::new(Duration::ZERO);
    let mut capture_clock = || ClockSnapshot {
        monotonic: now.get(),
        wall: UNIX_EPOCH + now.get(),
    };
    let proc_root = tree.proc();
    let sys_root = tree.sys();
    let mut ctx = crate::sensors::CollectCtx {
        proc_root: &proc_root,
        sys_root: &sys_root,
        commands: &mut commands,
        dbus: &mut dbus,
        nvml: None,
        bolt: None,
        clock: &mut capture_clock,
        skip_slow: false,
    };
    let job = crate::scheduler::JobId::singleton(
        crate::scheduler::OwnerId::Network,
        crate::scheduler::JobKind::NetworkIdentity,
    );

    crate::sensors::execute_scheduled_job(
        &job,
        owners.refs(),
        &mut hw,
        &cfg,
        &mut ctx,
        &mut readings,
        None,
    );
    assert_eq!(readings.wifi_ssid.as_deref(), Some("Home"));
    assert_eq!(readings.wifi_signal_percent, Some(80));

    now.set(Duration::from_secs(10));
    crate::sensors::execute_scheduled_job(
        &job,
        owners.refs(),
        &mut hw,
        &cfg,
        &mut ctx,
        &mut readings,
        None,
    );

    assert_eq!(readings.net_device.as_deref(), Some("eth0"));
    assert_eq!(readings.wifi_ssid, None);
    assert_eq!(readings.wifi_signal_percent, None);
    assert_eq!(
        crate::sensors::scheduled_capture_time(&job, owners.refs()),
        Some(Duration::from_secs(10))
    );
}

#[test]
fn profiling_capture_time_uses_stale_cpu_auxiliary_sample() {
    let mut owners = TestOwners::default();
    owners.cpu.usage.record_value(50, Duration::from_secs(10));
    owners
        .cpu
        .temperature
        .record_value(60, Duration::from_secs(2));
    let job = crate::scheduler::JobId::singleton(
        crate::scheduler::OwnerId::Cpu,
        crate::scheduler::JobKind::Cpu,
    );

    assert_eq!(
        crate::sensors::scheduled_capture_time(&job, owners.refs()),
        Some(Duration::from_secs(2))
    );
}

#[test]
fn profiling_capture_time_uses_stale_memory_auxiliary_sample() {
    let mut owners = TestOwners::default();
    owners.memory.usage.record_value(
        memory::MemoryUsage {
            percent: 50,
            used_gib: 2,
            total_gib: 4,
        },
        Duration::from_secs(10),
    );
    owners
        .memory
        .swap_usage
        .record_value(25, Duration::from_secs(3));
    let job = crate::scheduler::JobId::singleton(
        crate::scheduler::OwnerId::Memory,
        crate::scheduler::JobKind::Memory,
    );

    assert_eq!(
        crate::sensors::scheduled_capture_time(&job, owners.refs()),
        Some(Duration::from_secs(3))
    );
}

fn execute_history_job(
    job: crate::scheduler::JobId,
    run_id: u64,
    owners: &mut TestOwners,
    hw: &mut HardwareInventory,
    cfg: &Config,
    readings: &mut DisplaySnapshot,
) -> crate::scheduler::CompletionKind {
    let ticket = crate::scheduler::JobTicket {
        run_id: crate::scheduler::RunId(run_id),
        job,
        config_generation: crate::scheduler::ConfigGeneration(1),
        inventory_generation: crate::scheduler::InventoryGeneration(1),
        history_deadline: Some(crate::scheduler::HistoryDeadline::new(
            crate::scheduler::SchedulerTime::from_duration(Duration::from_secs(1)),
        )),
    };
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut capture_clock = fixed_clock(3);
    let mut ctx = crate::sensors::CollectCtx {
        proc_root: Path::new("/nonexistent"),
        sys_root: Path::new("/nonexistent"),
        commands: &mut commands,
        dbus: &mut dbus,
        nvml: None,
        bolt: None,
        clock: &mut capture_clock,
        skip_slow: false,
    };
    crate::sensors::execute_scheduled_job(&ticket, owners.refs(), hw, cfg, &mut ctx, readings, None)
        .completion
}

#[test]
fn history_executor_carries_sample_captured_before_ticket_deadline() {
    let cfg = Config::default();
    let mut hw = HardwareInventory::default();
    let mut owners = TestOwners::default();
    let sample = |percent, captured_at| {
        (
            memory::MemoryUsage {
                percent,
                used_gib: 1,
                total_gib: 4,
            },
            captured_at,
        )
    };
    let (value, captured_at) = sample(25, Duration::from_millis(500));
    owners.memory.usage.record_value(value, captured_at);
    let (value, captured_at) = sample(75, Duration::from_millis(1_500));
    owners.memory.usage.record_value(value, captured_at);
    let mut readings = DisplaySnapshot::default();

    let completion = execute_history_job(
        crate::scheduler::JobId::singleton(
            crate::scheduler::OwnerId::Memory,
            crate::scheduler::JobKind::MemoryHistory,
        ),
        1,
        &mut owners,
        &mut hw,
        &cfg,
        &mut readings,
    );

    assert_eq!(completion, crate::scheduler::CompletionKind::Captured);
    assert_eq!(owners.memory.mem_history, vec![25]);
    assert_eq!(
        owners.memory.mem_history_sample_at,
        Some(Duration::from_secs(1))
    );
}

#[test]
fn history_executor_rejects_first_sample_captured_after_ticket_deadline() {
    let cfg = Config::default();
    let mut hw = HardwareInventory::default();
    let mut owners = TestOwners::default();
    owners.memory.usage.record_value(
        memory::MemoryUsage {
            percent: 75,
            used_gib: 3,
            total_gib: 4,
        },
        Duration::from_millis(1_500),
    );
    let mut readings = DisplaySnapshot::default();

    let completion = execute_history_job(
        crate::scheduler::JobId::singleton(
            crate::scheduler::OwnerId::Memory,
            crate::scheduler::JobKind::MemoryHistory,
        ),
        1,
        &mut owners,
        &mut hw,
        &cfg,
        &mut readings,
    );

    assert_eq!(completion, crate::scheduler::CompletionKind::Baseline);
    assert!(owners.memory.mem_history.is_empty());
}

#[test]
fn typed_cpu_network_and_intel_histories_carry_predeadline_samples() {
    let mut cfg = Config::default();
    cfg.pages.order.push(String::from("graphs"));
    let pci = String::from("0000:00:02.0");
    let mut hw = HardwareInventory {
        net_device: Some(String::from("eth0")),
        intel_gpu_pci: Some(pci.clone()),
        ..HardwareInventory::default()
    };
    let mut owners = TestOwners::default();
    owners
        .cpu
        .usage
        .record_value(10, Duration::from_millis(500));
    owners
        .cpu
        .usage
        .record_value(90, Duration::from_millis(1_500));
    owners
        .network
        .rate
        .record_value((100, 200), Duration::from_millis(500));
    owners
        .network
        .rate
        .record_value((900, 800), Duration::from_millis(1_500));
    owners.intel_gpu.source_pci = Some(pci.clone());
    owners.intel_gpu.usage.record_value(
        std::collections::BTreeMap::from([(String::from("render"), 20)]),
        Duration::from_millis(500),
    );
    owners.intel_gpu.usage.record_value(
        std::collections::BTreeMap::from([(String::from("render"), 80)]),
        Duration::from_millis(1_500),
    );
    let mut readings = DisplaySnapshot::default();

    for (run_id, job) in [
        (
            1,
            crate::scheduler::JobId::singleton(
                crate::scheduler::OwnerId::Cpu,
                crate::scheduler::JobKind::CpuHistory,
            ),
        ),
        (
            2,
            crate::scheduler::JobId::singleton(
                crate::scheduler::OwnerId::Network,
                crate::scheduler::JobKind::NetworkHistory,
            ),
        ),
        (
            3,
            crate::scheduler::JobId::with_source(
                crate::scheduler::OwnerId::GpuHistory,
                crate::scheduler::JobKind::GpuHistory,
                crate::scheduler::SourceIdentity::Device(format!("intel:{pci}")),
            ),
        ),
    ] {
        assert_eq!(
            execute_history_job(job, run_id, &mut owners, &mut hw, &cfg, &mut readings,),
            crate::scheduler::CompletionKind::Captured
        );
    }

    assert_eq!(owners.cpu.cpu_history, [10]);
    assert_eq!(owners.network.net_up_history(), [100]);
    assert_eq!(owners.network.net_down_history(), [200]);
    assert_eq!(owners.gpu_history.usage, [20]);
}

#[test]
fn selected_nvidia_history_carries_predeadline_sample() {
    let mut cfg = Config::default();
    cfg.pages.order.push(String::from("graphs"));
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut owners = TestOwners::default();
    let mut nvml = FakeNvml::new(vec![
        Ok(NvidiaMetrics {
            usage_percent: Some(20),
            decoder_percent: Some(30),
            ..NvidiaMetrics::default()
        }),
        Ok(NvidiaMetrics {
            usage_percent: Some(80),
            decoder_percent: Some(70),
            ..NvidiaMetrics::default()
        }),
    ]);
    let mut commands = FakeCommandRunner::new();
    for captured_at in [Duration::from_millis(500), Duration::from_millis(1_500)] {
        let _ = crate::sensors::gpu_nvidia::read_nvidia(
            &mut owners.nvidia.cache,
            Some(&mut nvml),
            &mut commands,
            ClockSnapshot {
                monotonic: captured_at,
                wall: UNIX_EPOCH + captured_at,
            },
        );
    }
    let mut readings = DisplaySnapshot::default();

    let completion = execute_history_job(
        crate::scheduler::JobId::with_source(
            crate::scheduler::OwnerId::GpuHistory,
            crate::scheduler::JobKind::GpuHistory,
            crate::scheduler::SourceIdentity::Device(String::from("nvidia")),
        ),
        1,
        &mut owners,
        &mut hw,
        &cfg,
        &mut readings,
    );

    assert_eq!(completion, crate::scheduler::CompletionKind::Captured);
    assert_eq!(owners.gpu_history.usage, [20]);
    assert_eq!(owners.gpu_history.decoder, [30]);
}
