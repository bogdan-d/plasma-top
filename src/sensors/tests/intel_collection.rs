use super::*;

#[test]
fn collect_intel_gpu_freq_and_usage_read_when_capable() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/drm/card0/gt_act_freq_mhz", "1300\n");
    // fdinfo fixture: one client with render/video engine counters.
    tree.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\ndrm-engine-video:\t0 ns\n",
    );
    let mut cfg = cfg_panel(&["gpu_intel_freq", "gpu_intel_usage", "gpu_intel_dec_usage"]);
    cfg.pages.order = vec![String::from("graphs")];
    let mut hw = HardwareInventory {
        intel_gpu_pci: Some("0000:00:02.0".to_owned()),
        ..HardwareInventory::default()
    };
    hw.intel_gpu_freq_path = Some(tree.sys().join("class/drm/card0/gt_act_freq_mhz"));

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
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
    assert_eq!(r1.gpu_intel_freq, Some(1300));
    assert_eq!(r1.gpu_intel_usage, None);
    assert_eq!(r1.gpu_intel_dec_usage, None);
    assert!(r1.gpu_usage_history.is_empty());
    assert!(r1.gpu_dec_history.is_empty());

    // After the 30s usage-TTL the cache expires; advance to t=31 so the diff
    // recomputes. Render advances one full core over 31s → 100% (capped 99);
    // video advances half → 50%.
    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\n\
         drm-engine-render:\t31000000000 ns\ndrm-engine-video:\t15500000000 ns\n",
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
        clock(31),
        false,
    );
    assert_eq!(r2.gpu_intel_usage, Some(99));
    assert_eq!(r2.gpu_intel_dec_usage, Some(50));
}

#[test]
fn intel_first_baseline_retries_next_pass_then_uses_normal_ttl() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
    );
    let cfg = cfg_panel(&["gpu_intel_usage"]);
    let mut hw = HardwareInventory {
        intel_gpu_pci: Some("0000:00:02.0".to_owned()),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
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
    assert_eq!(first.gpu_intel_usage, None);
    // The baseline has no valid usage sample, so the next demanded pass retries promptly.
    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t1000000000 ns\n",
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
        clock(5),
        false,
    );
    assert_eq!(r2.gpu_intel_usage, Some(20));

    // Once a measured delta exists, changes remain cached for the normal 30-second cadence.
    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t6000000000 ns\n",
    );
    let r3 = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(6),
        false,
    );
    assert_eq!(r3.gpu_intel_usage, Some(20));
    assert_eq!(
        lanes.intel_gpu.usage_cache_sample_at,
        Some(Duration::from_secs(5))
    );

    let refreshed = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(35),
        false,
    );
    assert_eq!(refreshed.gpu_intel_usage, Some(16));
    assert_eq!(
        lanes.intel_gpu.usage_cache_sample_at,
        Some(Duration::from_secs(35))
    );
}

#[test]
fn intel_root_failure_retains_sample_and_counter_baseline() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
    );
    let mut state = gpu_intel::IntelGpuState::default();
    let mut timings = None;
    let _ = attempt_intel_usage(
        &mut state,
        &tree.proc(),
        "0000:00:02.0",
        clock(0),
        &mut timings,
    );
    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t1000000000 ns\n",
    );
    let valid = attempt_intel_usage(
        &mut state,
        &tree.proc(),
        "0000:00:02.0",
        clock(31),
        &mut timings,
    );
    assert_eq!(valid.reading.status, AttemptStatus::Captured);
    let baseline = state.engine_prev.clone();

    let failed = attempt_intel_usage(
        &mut state,
        &tree.root.join("missing"),
        "0000:00:02.0",
        clock(62),
        &mut timings,
    );
    assert_eq!(failed.reading.status, AttemptStatus::Failed);
    assert_eq!(state.engine_prev, baseline);
    assert_eq!(
        failed
            .reading
            .sample
            .expect("retained Intel sample")
            .captured_at,
        Duration::from_secs(31)
    );

    let empty_proc = tree.root.join("empty-proc");
    fs::create_dir_all(&empty_proc).expect("empty proc root");
    let empty = attempt_intel_usage(
        &mut state,
        &empty_proc,
        "0000:00:02.0",
        clock(93),
        &mut timings,
    );
    assert_eq!(empty.reading.status, AttemptStatus::Captured);
    assert!(state.engine_prev.is_empty());
}

#[test]
fn intel_noncomparable_reads_retain_sample_and_wait_for_second_baseline_read() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
    );
    let mut cfg = cfg_panel(&["gpu_intel_usage"]);
    cfg.pages.order = vec![String::from("graphs")];
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    let mut hw = HardwareInventory {
        intel_gpu_pci: Some(String::from("0000:00:02.0")),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
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
    assert!(first.gpu_usage_history.is_empty());

    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t31000000000 ns\n",
    );
    let captured = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(31),
        false,
    );
    assert_eq!(captured.gpu_intel_usage, Some(99));
    assert_eq!(captured.gpu_usage_history, vec![99]);

    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t10 ns\n",
    );
    let rollback = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(62),
        false,
    );
    assert_eq!(rollback.gpu_intel_usage, Some(99));
    assert_eq!(rollback.gpu_usage_history, vec![99]);
    assert_eq!(lanes.intel_gpu.usage.failed_at, None);

    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t500000010 ns\n",
    );
    let promptly_recovered = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(63),
        false,
    );
    assert_eq!(promptly_recovered.gpu_intel_usage, Some(50));
    assert_eq!(promptly_recovered.gpu_usage_history, vec![99, 50]);

    let cached = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(64),
        false,
    );
    assert_eq!(cached.gpu_intel_usage, Some(50));
    assert_eq!(cached.gpu_usage_history, vec![99, 50, 50]);

    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t15500000010 ns\n",
    );
    let recovered = run_collect(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(93),
        false,
    );
    assert_eq!(recovered.gpu_intel_usage, Some(50));
    assert_eq!(recovered.gpu_usage_history, vec![99, 50, 50, 50]);
}

// ── collect: battery_sys cache + UPower fallback ─────────────────────────────
