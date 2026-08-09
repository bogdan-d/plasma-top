use super::*;

#[test]
fn collect_empty_capability_set_only_reads_cpu_and_mem() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["cpu_usage", "mem_usage"]); // no caps

    let mut lanes = TestOwners::default();
    let mut hw = HardwareInventory::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();

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

    assert_eq!(readings.cpu_usage, None); // first counter read is a baseline
    assert!(readings.cpu_history.is_empty());
    assert_eq!(readings.mem_usage, Some(25));
    assert!(readings.cpu_temp.is_none());
    assert!(readings.net_up_bps.is_none());
    assert!(readings.battery_sys.is_empty());
    // Zero unrequested calls: no commands, no D-Bus.
    assert!(commands.call_trace().is_empty());
    assert!(dbus.call_trace().is_empty());
}

// ── collect: individual capabilities ─────────────────────────────────────────

#[test]
fn collect_cpu_temp_reads_hwmon_when_capable() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
    tree.write("sys/class/hwmon/hwmon0/temp1_input", "52000\n");
    let cfg = cfg_panel(&["cpu_temp"]);
    let mut hw = discover_hardware(
        &tree.sys(),
        &tree.proc(),
        &cfg,
        &mut FakeDbus::new(),
        &mut FakeCommandRunner::new(),
        2,
    );

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
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
    assert_eq!(readings.cpu_temp, Some(52));
}

#[test]
fn collect_cpu_freq_and_turbo_read_when_capable() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write(
        "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
        "2800000\n",
    );
    tree.write("sys/devices/system/cpu/intel_pstate/no_turbo", "0\n");
    let cfg = cfg_panel(&["cpu_freq"]); // pulls CpuFrequency + CpuTurbo
    let mut hw = discover_hardware(
        &tree.sys(),
        &tree.proc(),
        &cfg,
        &mut FakeDbus::new(),
        &mut FakeCommandRunner::new(),
        2,
    );

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
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
    assert_eq!(readings.cpu_freq_mhz, Some(2800.0));
    assert_eq!(readings.cpu_turbo, Some(true));
}

#[test]
fn collect_uptime_loadavg_swap_read_when_capable() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let cfg = cfg_panel(&["uptime", "load_avg", "swap_usage"]);
    let mut hw = HardwareInventory::default();

    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
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
    assert_eq!(readings.uptime_seconds, Some(12345));
    assert_eq!(
        readings.load_average.map(|l| (l.one, l.five, l.fifteen)),
        Some((0.50, 0.40, 0.30))
    );
    assert_eq!(readings.swap_usage, Some(75)); // (1M - 256M)/1M = 75%
}

#[test]
fn collect_top_process_populates_panel_and_full_rows() {
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
    // First poll seeds prev (top_process_full None); second diff.
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
        clock(0),
        false,
    );
    tree.write(
        "proc/100/stat",
        &proc_stat_line(100, "firefox", 1100, 0, 200),
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
        clock(1),
        false,
    );
    let full = readings.top_process_full.expect("full rows");
    assert_eq!(full[0].command, "firefox");
    assert!(full[0].cpu_percent > 0);
    let panel = readings.top_process.expect("panel rows");
    assert_eq!(panel.len(), 1);
    assert_eq!(panel[0].command, "firefox");

    let mut disabled = Config::default();
    disabled.panel.sections.clear();
    disabled.tooltip.sections.clear();
    disabled.pages.order.clear();
    let _ = run_collect(
        &mut lanes,
        &mut hw,
        &disabled,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        clock(2),
        false,
    );
    assert!(lanes.process.panel.latest.is_none());
    assert!(lanes.process.proc_prev_times.is_empty());
}

#[test]
fn process_baseline_empty_and_failure_have_distinct_retention() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write(
        "proc/100/stat",
        &proc_stat_line(100, "firefox", 100, 0, 200),
    );
    let mut state = process::ProcessState::default();

    let baseline = attempt_process(&mut state, &tree.proc(), clock(0));
    assert_eq!(baseline.reading.status, AttemptStatus::Baseline);
    assert!(baseline.reading.sample.is_none());

    tree.write(
        "proc/100/stat",
        &proc_stat_line(100, "firefox", 300, 0, 200),
    );
    let valid = attempt_process(&mut state, &tree.proc(), clock(1));
    assert_eq!(valid.reading.status, AttemptStatus::Captured);
    assert!(valid.reading.sample.is_some());

    let failed = attempt_process(&mut state, &tree.root.join("missing"), clock(16));
    assert_eq!(failed.reading.status, AttemptStatus::Failed);
    assert_eq!(
        failed
            .reading
            .sample
            .as_ref()
            .expect("retained process rows")
            .captured_at,
        Duration::from_secs(1)
    );
    assert_eq!(state.panel.attempted_at, Some(Duration::from_secs(16)));
    assert!(sample_due(
        state.panel.latest.as_ref().map(|sample| sample.captured_at),
        Duration::from_secs(17),
        process::TOP_PROCESS_TTL,
    ));

    fs::remove_dir_all(tree.root.join("proc/100")).expect("remove process");
    let empty = attempt_process(&mut state, &tree.proc(), clock(17));
    assert_eq!(empty.reading.status, AttemptStatus::Absent);
    assert!(empty.reading.sample.is_none());
    assert!(state.panel.latest.is_none());
}

#[test]
fn panel_process_nonpositive_elapsed_retains_rows_and_pid_reuse_rebaselines() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write(
        "proc/100/stat",
        &proc_stat_line(100, "firefox", 100, 0, 200),
    );
    let mut state = process::ProcessState::default();

    let _ = attempt_process(&mut state, &tree.proc(), clock(0));
    tree.write(
        "proc/100/stat",
        &proc_stat_line(100, "firefox", 300, 0, 200),
    );
    let captured = attempt_process(&mut state, &tree.proc(), clock(1));
    let sample = captured.reading.sample.clone().expect("process sample");

    tree.write(
        "proc/100/stat",
        &proc_stat_line(100, "firefox", 400, 0, 200),
    );
    let zero_elapsed = attempt_process(&mut state, &tree.proc(), clock(1));
    assert_eq!(zero_elapsed.reading.status, AttemptStatus::Baseline);
    assert_eq!(zero_elapsed.reading.sample, Some(sample));
    assert_eq!(zero_elapsed.reading.failed_at, None);
    assert_eq!(state.proc_prev_times.get(&100), Some(&300));
    assert_eq!(state.proc_prev_sample_at, Some(Duration::from_secs(1)));

    tree.write("proc/100/stat", &proc_stat_line(100, "firefox", 10, 0, 200));
    let rollback = attempt_process(&mut state, &tree.proc(), clock(2));
    assert_eq!(rollback.reading.status, AttemptStatus::Absent);
    assert!(rollback.reading.sample.is_none());
    assert_eq!(state.proc_prev_times.get(&100), Some(&10));
    assert_eq!(state.proc_prev_sample_at, Some(Duration::from_secs(2)));

    tree.write(
        "proc/100/stat",
        &proc_stat_line(100, "firefox", 500, 0, 200),
    );
    let recovered = attempt_process(&mut state, &tree.proc(), clock(3));
    assert_eq!(recovered.reading.status, AttemptStatus::Captured);
    assert_eq!(
        recovered
            .reading
            .sample
            .map(|sample| sample.value.summary[0].cpu_percent),
        Some(490),
        "the reused PID must become comparable against its committed baseline"
    );
}
