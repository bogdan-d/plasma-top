use super::*;

#[test]
fn collect_skip_slow_skips_top_process_nvidia_and_bolt() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write(
        "proc/100/stat",
        "100 (firefox) R 0 0 0 0 0 0 0 0 0 1000 0 0 0 20 0 1 0 0 0 200 x\n",
    );
    let mut cfg = cfg_panel(&["top_process", "gpu_nvidia_temp"]);
    cfg.battery.mouse_bolt = Some(1);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    hw.hd_temp_paths.clear();

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut bolt = FakeBolt::new(vec![]);
    let readings = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        Some(&mut bolt),
        clock(0),
        true,
    );
    // skip_slow → no top_process, no nvidia-smi, no bolt query.
    assert!(readings.top_process.is_none());
    assert!(readings.top_process_full.is_none());
    assert!(readings.gpu_temp.is_none());
    assert!(readings.battery_mouse.is_none());
    assert_eq!(bolt.calls, 0);
    assert!(commands.call_trace().is_empty());
}

#[test]
fn collect_without_skip_slow_reads_slow_sensors() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write(
        "proc/100/stat",
        &proc_stat_line(100, "firefox", 1000, 0, 200),
    );
    let cfg = cfg_panel(&["top_process"]);
    let mut hw = HardwareInventory::default();

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    // First non-skip poll seeds prev (top_process_full None); scan still ran.
    let r1 = run_collect(
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
    assert!(r1.top_process_full.is_none());
    // Bump jiffies; second poll yields a real row → the slow sensor ran.
    tree.write(
        "proc/100/stat",
        &proc_stat_line(100, "firefox", 2000, 0, 200),
    );
    let r2 = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(1),
        false,
    );
    assert!(r2.top_process_full.is_some());
}

// ── collect: NVIDIA NVML selection ───────────────────────────────────────────

#[test]
fn collect_nvidia_nvml_success_uses_facade_and_skips_smi() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut nvml = FakeNvml::new(vec![Ok(NvidiaMetrics {
        temp_celsius: Some(65),
        usage_percent: Some(70),
        memory_percent: Some(40),
        decoder_percent: None,
        fan_percent: Some(35),
    })]);
    let readings = run_collect(
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
    assert_eq!(readings.gpu_temp, Some(65));
    assert_eq!(readings.gpu_usage, Some(70));
    assert_eq!(readings.gpu_fan, Some(35));
    assert_eq!(nvml.calls, 1);
    assert!(
        commands.call_trace().is_empty(),
        "no nvidia-smi on NVML success"
    );
}

#[test]
fn collect_nvidia_init_failure_falls_back_until_confirmed_removal() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
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
            "60, 50, 30, 40, 5\n",
        ),
    );
    let mut dbus = FakeDbus::new();
    let mut nvml = FakeNvml::new(vec![Err(NvmlError::Init)]);
    let readings = run_collect(
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
    // NVML init failed → nvidia-smi fallback; CSV order temp,usage,mem,dec,fan.
    assert_eq!(readings.gpu_temp, Some(60));
    assert_eq!(readings.gpu_usage, Some(50));
    assert_eq!(readings.gpu_mem, Some(30));
    assert_eq!(readings.gpu_dec, Some(5));
    assert_eq!(readings.gpu_fan, Some(40));
    assert_eq!(nvml.calls, 1);
    assert_eq!(commands.call_trace().len(), 1);
    assert_eq!(
        commands.call_trace()[0].timeout,
        crate::sensors::gpu_nvidia::NVIDIA_SMI_TIMEOUT
    );
}

#[test]
fn collect_nvidia_absent_facade_falls_back_to_smi() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
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
            "70, 60, 40, 50, 4\n",
        ),
    );
    let mut dbus = FakeDbus::new();
    // nvml = None: matches Python with python-nvidia-ml-py absent.
    let readings = run_collect(
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
    assert_eq!(readings.gpu_temp, Some(70));
    assert_eq!(readings.gpu_fan, Some(50));
    assert_eq!(commands.call_trace().len(), 1);
}

#[test]
fn collect_nvidia_read_failure_retries_nvml_next_poll() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
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
            "55, 45, 25, 35, 2\n",
        ),
    );
    let mut dbus = FakeDbus::new();
    let mut nvml = FakeNvml::new(vec![
        Err(NvmlError::Read), // first poll: read fails → smi fallback
        Ok(NvidiaMetrics {
            // second poll: NVML retried (0s TTL)
            temp_celsius: Some(50),
            usage_percent: Some(20),
            memory_percent: Some(10),
            decoder_percent: Some(3),
            fan_percent: None,
        }),
    ]);
    let r1 = run_collect(
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
    assert_eq!(r1.gpu_temp, Some(55)); // smi fallback
    let r2 = run_collect(
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
    assert_eq!(r2.gpu_temp, Some(50)); // NVML recovered
    assert_eq!(nvml.calls, 2);
    assert_eq!(
        commands.call_trace().len(),
        1,
        "smi only on the failing poll"
    );
}

#[test]
fn nvidia_same_tick_failure_reports_failed_and_retains_sample() {
    let mut state = crate::sensors::gpu_nvidia::NvidiaState::default();
    let mut nvml = FakeNvml::new(vec![
        Ok(NvidiaMetrics {
            temp_celsius: Some(60),
            ..NvidiaMetrics::default()
        }),
        Err(NvmlError::Read),
    ]);
    let captured_status =
        crate::sensors::attempt_nvml_nvidia(&mut state, &mut nvml, Duration::from_secs(10));
    let captured = crate::sensors::nvidia_result(&state, captured_status);
    let failed_status =
        crate::sensors::attempt_nvml_nvidia(&mut state, &mut nvml, Duration::from_secs(10));
    let failed = crate::sensors::nvidia_result(&state, failed_status);

    assert_eq!(captured.reading.status, AttemptStatus::Captured);
    assert_eq!(failed.reading.status, AttemptStatus::Failed);
    let retained = failed.reading.sample.expect("retained NVIDIA sample");
    assert_eq!(retained.captured_at, Duration::from_secs(10));
    assert_eq!(retained.value.temp_celsius, Some(60));
    assert_eq!(state.cache.attempted_at, Some(Duration::from_secs(10)));
    assert_eq!(state.cache.failed_at, Some(Duration::from_secs(10)));
}

#[test]
fn collect_nvidia_transient_failure_retains_capture_and_records_attempt() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut nvml = FakeNvml::new(vec![
        Ok(NvidiaMetrics {
            temp_celsius: Some(60),
            ..NvidiaMetrics::default()
        }),
        Err(NvmlError::Read),
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
    assert_eq!(first.gpu_temp, Some(60));

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
    assert_eq!(failed.gpu_temp, Some(60));
    assert_eq!(lanes.nvidia.cache.sampled_at, Some(Duration::ZERO));
    assert_eq!(
        lanes.nvidia.cache.attempted_at,
        Some(Duration::from_secs(1)),
    );
    assert_eq!(lanes.nvidia.cache.failed_at, Some(Duration::from_secs(1)));
    assert_eq!(
        commands.call_trace().len(),
        1,
        "failed SMI fallback attempted"
    );

    hw.has_nvidia = false;
    let removed = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(2),
        false,
    );
    assert_eq!(removed.gpu_temp, None);
    assert_eq!(lanes.nvidia.cache.sampled_at, None);
}

#[test]
fn nvidia_smi_missing_mandatory_fields_retains_valid_sample() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["gpu_nvidia_temp"]);
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    for output in ["60, 40, 20, 10, 5\n", "N/A, N/A, 20, 10, 5\n"] {
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
                output,
            ),
        );
    }
    let mut dbus = FakeDbus::new();
    let first = run_collect(
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
    let invalid = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(3),
        false,
    );
    assert_eq!(first.gpu_temp, Some(60));
    assert_eq!(invalid.gpu_temp, first.gpu_temp);
    assert_eq!(invalid.gpu_usage, first.gpu_usage);
    assert_eq!(lanes.nvidia.cache.sampled_at, Some(Duration::ZERO));
    assert_eq!(
        lanes.nvidia.cache.attempted_at,
        Some(Duration::from_secs(3))
    );
}

// ── collect: Bolt battery (peripheral path) ──────────────────────────────────

#[test]
fn collect_bolt_mouse_battery_when_configured_and_no_upower_id() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut cfg = cfg_panel(&["battery_mouse"]);
    cfg.battery.mouse_bolt = Some(1);
    cfg.battery.mouse_name = Some("MX Mouse".to_owned());
    let mut hw = HardwareInventory::default(); // no UPower mouse id

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut bolt = FakeBolt::new(vec![Ok(Some(BoltBattery {
        name: String::from("MX Master"),
        level: 88,
    }))]);
    let readings = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        Some(&mut bolt),
        clock(0),
        false,
    );
    let mouse = readings.battery_mouse.expect("bolt reading");
    assert_eq!(mouse.charge_percent, 88);
    assert_eq!(mouse.name, "MX Mouse"); // name override wins
    assert_eq!(bolt.calls, 1);
}

#[test]
fn collect_bolt_unsupported_feature_uses_normal_completion_cadence() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut cfg = cfg_panel(&["battery_mouse", "battery_kbd"]);
    cfg.battery.mouse_bolt = Some(1);
    cfg.battery.kbd_bolt = Some(2);
    cfg.battery.mouse_name = Some("Mouse".to_owned());
    cfg.battery.kbd_name = Some("Keyboard".to_owned());
    let mut hw = HardwareInventory::default();
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut bolt = FakeBolt::new(vec![
        Ok(Some(BoltBattery {
            name: String::new(),
            level: 80,
        })),
        Ok(Some(BoltBattery {
            name: String::new(),
            level: 70,
        })),
        Ok(None),
        Ok(None),
    ]);

    let _ = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        Some(&mut bolt),
        clock(0),
        false,
    );
    let no_level = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        Some(&mut bolt),
        clock(3600),
        false,
    );
    let suppressed = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        Some(&mut bolt),
        clock(3601),
        false,
    );

    assert_eq!(bolt.calls, 4);
    assert!(no_level.battery_mouse.is_none());
    assert!(no_level.battery_kbd.is_none());
    assert!(suppressed.battery_mouse.is_none());
    assert!(suppressed.battery_kbd.is_none());
    assert_eq!(lanes.power.battery_mouse_cache.sampled_at, None);
    assert_eq!(lanes.power.battery_kbd_cache.sampled_at, None);
    assert_eq!(
        lanes.power.battery_mouse_cache.bolt_completed_at,
        Some(Duration::from_secs(3600))
    );
    assert_eq!(
        lanes.power.battery_kbd_cache.bolt_completed_at,
        Some(Duration::from_secs(3600))
    );
}

#[test]
fn collect_bolt_hid_error_retries_without_waiting_for_ttl() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut cfg = cfg_panel(&["battery_mouse"]);
    cfg.battery.mouse_bolt = Some(1);
    cfg.battery.mouse_name = Some("Mouse".to_owned());
    let mut hw = HardwareInventory::default();
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut bolt = FakeBolt::new(vec![
        Ok(Some(BoltBattery {
            name: String::new(),
            level: 80,
        })),
        Err(crate::domain::boundary::BoundaryError::CommandFailed {
            program: PathBuf::from("bolt"),
            args: Vec::new(),
            detail: "HID write failed".to_owned(),
        }),
        Ok(Some(BoltBattery {
            name: String::new(),
            level: 75,
        })),
    ]);

    let _ = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        Some(&mut bolt),
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
        None,
        Some(&mut bolt),
        clock(3600),
        false,
    );
    let retried = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        Some(&mut bolt),
        clock(3601),
        false,
    );

    assert_eq!(failed.battery_mouse.expect("retained").charge_percent, 80);
    assert_eq!(retried.battery_mouse.expect("recovered").charge_percent, 75);
    assert_eq!(bolt.calls, 3);
    assert_eq!(
        lanes.power.battery_mouse_cache.sampled_at,
        Some(Duration::from_secs(3601))
    );
    assert_eq!(
        lanes.power.battery_mouse_cache.bolt_completed_at,
        Some(Duration::from_secs(3601))
    );
}

#[test]
fn collect_upower_periph_battery_reads_via_dbus() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["battery_mouse"]);
    let mut hw = HardwareInventory::default();
    hw.battery_mouse_id = Some("/battery_hidpp_mouse".to_owned());

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        "/battery_hidpp_mouse",
        "org.freedesktop.DBus.Properties",
        "GetAll",
        getall_reply(
            "/battery_hidpp_mouse",
            &[("Percentage", "77"), ("Model", "MX")],
        ),
    );
    let readings = run_collect(
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
    let mouse = readings.battery_mouse.expect("periph reading");
    assert_eq!(mouse.charge_percent, 77);
    assert_eq!(mouse.name, "MX");
}

#[test]
fn collect_upower_failure_retains_sample_and_records_attempt() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["battery_mouse"]);
    let mut hw = HardwareInventory {
        battery_mouse_id: Some("/battery_hidpp_mouse".to_owned()),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        "/battery_hidpp_mouse",
        "org.freedesktop.DBus.Properties",
        "GetAll",
        getall_reply(
            "/battery_hidpp_mouse",
            &[("Percentage", "77"), ("Model", "MX")],
        ),
    );

    let first = run_collect(
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
    assert_eq!(
        first.battery_mouse.expect("first sample").charge_percent,
        77
    );

    let failed = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(30),
        false,
    );
    assert_eq!(
        failed
            .battery_mouse
            .expect("retained sample")
            .charge_percent,
        77
    );
    assert_eq!(
        lanes.power.battery_mouse_cache.sampled_at,
        Some(Duration::ZERO)
    );
    assert_eq!(
        lanes.power.battery_mouse_cache.attempted_at,
        Some(Duration::from_secs(30)),
    );
    assert_eq!(
        lanes.power.battery_mouse_cache.failed_at,
        Some(Duration::from_secs(30)),
    );
}

// ── collect: SMART path ──────────────────────────────────────────────────────

#[test]
fn collect_disk_smart_uses_per_drive_ttl_and_udisks2_calls() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["disk_smart:pair"]); // smart defaults true
    let mut hw = HardwareInventory::default();
    hw.disk_smart_drives.insert(
        "nvme0n1".to_owned(),
        crate::domain::readings::SmartDisk {
            object_path: "/drives/NVMe_1".to_owned(),
            interface: crate::domain::readings::DiskSmartInterface::Nvme,
            rotational: false,
        },
    );

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    // SmartUpdate + Properties.Get(SmartCriticalWarning="") → healthy.
    dbus.enqueue(
        SYSTEM,
        "org.freedesktop.UDisks2",
        "/drives/NVMe_1",
        "org.freedesktop.UDisks2.NVMe.Controller",
        "SmartUpdate",
        DbusOutput {
            bus: SYSTEM,
            service: "org.freedesktop.UDisks2".to_owned(),
            object_path: "/drives/NVMe_1".to_owned(),
            interface: "org.freedesktop.UDisks2.NVMe.Controller".to_owned(),
            member: "SmartUpdate".to_owned(),
            body: vec![],
        },
    );
    dbus.enqueue(
        SYSTEM,
        "org.freedesktop.UDisks2",
        "/drives/NVMe_1",
        "org.freedesktop.DBus.Properties",
        "Get",
        DbusOutput {
            bus: SYSTEM,
            service: "org.freedesktop.UDisks2".to_owned(),
            object_path: "/drives/NVMe_1".to_owned(),
            interface: "org.freedesktop.DBus.Properties".to_owned(),
            member: "Get".to_owned(),
            body: vec![String::new()],
        },
    );
    let readings = run_collect(
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
    assert_eq!(
        readings.disk_smart.get("nvme0n1").copied().flatten(),
        Some(true)
    );
    // Within TTL (1h default for SSD): second poll reuses the cache → no new calls.
    let dbus_calls_after_first = dbus.call_trace().len();
    let _ = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(10),
        false,
    );
    assert_eq!(dbus.call_trace().len(), dbus_calls_after_first);

    // A due failed refresh keeps the valid health sample but records the attempt separately.
    let failed = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(3600),
        false,
    );
    assert_eq!(failed.disk_smart["nvme0n1"], Some(true));
    assert_eq!(dbus.call_trace().len(), dbus_calls_after_first + 1);
    let smart = lanes.disk.smart_cache.get("nvme0n1").expect("smart state");
    assert_eq!(
        smart.latest.as_ref().expect("valid SMART").captured_at,
        Duration::ZERO
    );
    assert_eq!(smart.attempted_at, Some(Duration::from_secs(3600)));
}

// ── collect: history coordination ────────────────────────────────────────────
