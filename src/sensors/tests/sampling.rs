use super::*;

#[test]
fn timed_records_key_when_some_and_is_noop_when_none() {
    // None path: work runs, no Instant touched, no map needed.
    let mut none: Option<&mut Timings> = None;
    let value = timed(&mut none, "x", || 7);
    assert_eq!(value, 7);

    // Some path: key recorded with a non-negative elapsed.
    let mut timings: Timings = Timings::new();
    timed(&mut Some(&mut timings), "cpu_usage", || 1 + 1);
    assert!(timings.contains_key("cpu_usage"));
    assert!(timings["cpu_usage"] >= Duration::ZERO);

    // Repeated calls accumulate under the same key.
    let before = timings["cpu_usage"];
    timed(&mut Some(&mut timings), "cpu_usage", || 2 + 2);
    assert!(timings["cpu_usage"] >= before);
}

#[test]
fn collect_uses_attempt_local_capture_times_and_later_assembly_time() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/temp", "41000\n");
    tree.write("sys/fan", "1500\n");
    let cfg = cfg_panel(&["hd_temp", "fan_speed"]);
    let mut hw = HardwareInventory {
        hd_temp_paths: [(String::from("disk"), tree.sys().join("temp"))].into(),
        fan_paths: [(String::from("1"), tree.sys().join("fan"))].into(),
        ..HardwareInventory::default()
    };
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut fake_clock = FakeClock::default();
    let mut capture_clock = || fake_clock.tick();

    let readings = run_collect_with_clock(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        &mut capture_clock,
        false,
    );

    let temperature = lanes.disk.hd_temp_cache["disk"]
        .latest
        .as_ref()
        .expect("temperature sample");
    let fan = lanes.disk.fan_speed_cache["1"]
        .latest
        .as_ref()
        .expect("fan sample");
    assert!(temperature.captured_at > Duration::ZERO);
    assert!(fan.captured_at > temperature.captured_at);
    assert!(readings.assembled_at.monotonic > fan.captured_at);
}

#[test]
fn cpu_owner_timestamps_each_source_read_and_retains_failed_capture_time() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/cpu_temp", "41000\n");
    tree.write("sys/cpu_freq", "2800000\n");
    tree.write("sys/devices/system/cpu/intel_pstate/no_turbo", "0\n");
    let hw = HardwareInventory {
        cpu_temp_path: Some(tree.sys().join("cpu_temp")),
        cpu_freq_path: Some(tree.sys().join("cpu_freq")),
        cpu_turbo_path: Some(tree.sys().join("devices/system/cpu/intel_pstate/no_turbo")),
        cpu_turbo_supported: true,
        ..HardwareInventory::default()
    };
    let caps = BTreeSet::from([
        Capability::CpuTemperature,
        Capability::CpuFrequency,
        Capability::CpuTurbo,
        Capability::Uptime,
        Capability::LoadAverage,
    ]);
    let mut state = cpu::CpuState::default();
    let mut timings = None;
    assert_eq!(
        cpu::read_cpu_usage_once(&tree.proc(), &mut state),
        cpu::CpuUsageReadOutcome::Baseline
    );
    tree.write(
        "proc/stat",
        "cpu  30 0 20 100 0 0 0 0 0 0\n\
         cpu0 15 0 10 50 0 0 0 0 0 0\n\
         cpu1 15 0 10 50 0 0 0 0 0 0\n",
    );
    let mut fake_clock = FakeClock::default();
    let mut capture_clock = || fake_clock.tick();

    let result = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &hw,
        &caps,
        &mut capture_clock,
        &mut timings,
    );

    let captures = [
        result.usage.sample.expect("CPU usage").captured_at,
        result
            .temperature
            .sample
            .expect("CPU temperature")
            .captured_at,
        result
            .frequency_mhz
            .sample
            .expect("CPU frequency")
            .captured_at,
        result.turbo.sample.expect("CPU turbo").captured_at,
        result.uptime_seconds.sample.expect("uptime").captured_at,
        result
            .load_average
            .sample
            .expect("load average")
            .captured_at,
    ];
    assert_eq!(captures, [1, 2, 3, 4, 5, 6].map(Duration::from_secs));

    fs::remove_file(tree.sys().join("cpu_temp")).expect("remove CPU temperature source");
    let temperature_capture = state
        .temperature
        .latest
        .as_ref()
        .expect("retained CPU temperature")
        .captured_at;
    let failed = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &hw,
        &BTreeSet::from([Capability::CpuTemperature]),
        &mut capture_clock,
        &mut timings,
    );

    assert_eq!(
        failed
            .temperature
            .sample
            .expect("retained temperature")
            .captured_at,
        temperature_capture
    );
    assert_eq!(failed.temperature.status, AttemptStatus::Failed);
    assert_eq!(failed.temperature.failed_at, Some(Duration::from_secs(8)));
    assert_eq!(state.temperature.attempted_at, Some(Duration::from_secs(8)));
}

#[test]
fn cpu_optional_source_failures_are_explicit_at_unchanged_timestamp() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    tree.write("sys/cpu_temp", "41000\n");
    tree.write("sys/cpu_freq", "2800000\n");
    tree.write("sys/devices/system/cpu/intel_pstate/no_turbo", "0\n");
    let hw = HardwareInventory {
        cpu_temp_path: Some(tree.sys().join("cpu_temp")),
        cpu_freq_path: Some(tree.sys().join("cpu_freq")),
        cpu_turbo_path: Some(tree.sys().join("devices/system/cpu/intel_pstate/no_turbo")),
        cpu_turbo_supported: true,
        ..HardwareInventory::default()
    };
    let caps = BTreeSet::from([
        Capability::CpuTemperature,
        Capability::CpuFrequency,
        Capability::CpuTurbo,
        Capability::Uptime,
        Capability::LoadAverage,
    ]);
    let mut state = cpu::CpuState::default();
    let mut timings = None;

    let captured = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &hw,
        &caps,
        &mut fixed_clock(7),
        &mut timings,
    );
    fs::remove_file(tree.sys().join("cpu_temp")).expect("remove temperature");
    fs::remove_file(tree.sys().join("cpu_freq")).expect("remove frequency");
    fs::remove_file(tree.sys().join("devices/system/cpu/intel_pstate/no_turbo"))
        .expect("remove turbo");
    fs::remove_file(tree.proc().join("uptime")).expect("remove uptime");
    fs::remove_file(tree.proc().join("loadavg")).expect("remove load average");

    let failed = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &hw,
        &caps,
        &mut fixed_clock(7),
        &mut timings,
    );

    let patterns = [
        (
            captured.temperature.sample.map(|sample| sample.captured_at),
            failed.temperature.status,
            failed.temperature.sample.map(|sample| sample.captured_at),
            failed.temperature.attempted_at,
            failed.temperature.failed_at,
        ),
        (
            captured
                .frequency_mhz
                .sample
                .map(|sample| sample.captured_at),
            failed.frequency_mhz.status,
            failed.frequency_mhz.sample.map(|sample| sample.captured_at),
            failed.frequency_mhz.attempted_at,
            failed.frequency_mhz.failed_at,
        ),
        (
            captured.turbo.sample.map(|sample| sample.captured_at),
            failed.turbo.status,
            failed.turbo.sample.map(|sample| sample.captured_at),
            failed.turbo.attempted_at,
            failed.turbo.failed_at,
        ),
        (
            captured
                .uptime_seconds
                .sample
                .map(|sample| sample.captured_at),
            failed.uptime_seconds.status,
            failed
                .uptime_seconds
                .sample
                .map(|sample| sample.captured_at),
            failed.uptime_seconds.attempted_at,
            failed.uptime_seconds.failed_at,
        ),
        (
            captured
                .load_average
                .sample
                .map(|sample| sample.captured_at),
            failed.load_average.status,
            failed.load_average.sample.map(|sample| sample.captured_at),
            failed.load_average.attempted_at,
            failed.load_average.failed_at,
        ),
    ];
    for (captured_at, status, retained_at, attempted_at, failed_at) in patterns {
        assert_eq!(captured_at, Some(Duration::from_secs(7)));
        assert_eq!(status, AttemptStatus::Failed);
        assert_eq!(retained_at, captured_at);
        assert_eq!(attempted_at, Some(Duration::from_secs(7)));
        assert_eq!(failed_at, Some(Duration::from_secs(7)));
    }
}

#[test]
fn turbo_source_replacement_invalidates_while_same_source_failure_retains() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let intel = tree.sys().join("devices/system/cpu/intel_pstate/no_turbo");
    let boost = tree.sys().join("devices/system/cpu/cpufreq/boost");
    tree.write("sys/devices/system/cpu/intel_pstate/no_turbo", "0\n");
    let caps = BTreeSet::from([Capability::CpuTurbo]);
    let mut hw = HardwareInventory {
        cpu_turbo_path: Some(intel.clone()),
        cpu_turbo_supported: true,
        ..HardwareInventory::default()
    };
    let mut state = cpu::CpuState::default();
    let mut timings = None;

    let captured = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &hw,
        &caps,
        &mut fixed_clock(1),
        &mut timings,
    );
    assert_eq!(captured.turbo.sample.map(|sample| sample.value), Some(true));

    fs::remove_file(&intel).expect("remove same turbo source");
    let transient = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &hw,
        &caps,
        &mut fixed_clock(2),
        &mut timings,
    );
    assert_eq!(transient.turbo.status, AttemptStatus::Failed);
    assert_eq!(
        transient.turbo.sample.map(|sample| sample.value),
        Some(true)
    );

    hw.cpu_turbo_path = Some(boost.clone());
    let replaced_failure = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &hw,
        &caps,
        &mut fixed_clock(3),
        &mut timings,
    );
    assert_eq!(replaced_failure.turbo.status, AttemptStatus::Failed);
    assert!(replaced_failure.turbo.sample.is_none());

    tree.write("sys/devices/system/cpu/cpufreq/boost", "1\n");
    let replacement = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &hw,
        &caps,
        &mut fixed_clock(4),
        &mut timings,
    );
    assert_eq!(
        replacement.turbo.sample.map(|sample| sample.value),
        Some(true)
    );

    hw.cpu_turbo_path = None;
    hw.cpu_turbo_supported = false;
    let removed = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &hw,
        &caps,
        &mut fixed_clock(5),
        &mut timings,
    );
    assert!(removed.turbo.sample.is_none());
    assert!(state.turbo_source.is_none());
}

#[test]
fn grouped_command_battery_intel_and_external_reads_get_distinct_capture_times() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    for battery in ["BAT0", "BAT1"] {
        tree.write(
            &format!("sys/class/power_supply/{battery}/capacity"),
            "80\n",
        );
        tree.write(
            &format!("sys/class/power_supply/{battery}/status"),
            "Full\n",
        );
    }
    tree.write("sys/class/backlight/panel/brightness", "50\n");
    tree.write("sys/class/backlight/panel/max_brightness", "100\n");
    tree.write("sys/intel_freq", "1200\n");
    tree.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tree.write(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
    );
    tree.write("run/updates", "7\n");
    tree.write("run/server", "1\n");
    let mut cfg = cfg_panel(&[
        "net_device",
        "battery_sys",
        "battery_mouse",
        "battery_kbd",
        "gpu_intel_freq",
        "gpu_intel_usage",
        "screen_brightness",
        "system_updates",
        "server_check",
    ]);
    cfg.system_updates.file = tree.root.join("run/updates").to_string_lossy().into_owned();
    cfg.server_check.file = tree.root.join("run/server").to_string_lossy().into_owned();
    let mut hw = HardwareInventory {
        battery_sys_ids: vec![
            String::from("/org/freedesktop/UPower/devices/battery_BAT0"),
            String::from("/org/freedesktop/UPower/devices/battery_BAT1"),
        ],
        battery_mouse_id: Some(String::from("/battery_mouse")),
        battery_kbd_id: Some(String::from("/battery_keyboard")),
        intel_gpu_pci: Some(String::from("0000:00:02.0")),
        intel_gpu_freq_path: Some(tree.sys().join("intel_freq")),
        has_backlight: true,
        ..HardwareInventory::default()
    };
    let mut commands = FakeCommandRunner::new();
    commands.enqueue(
        IP,
        ["route", "get", "8.8.8.8"],
        ok_cmd(
            IP,
            &["route", "get", "8.8.8.8"],
            "8.8.8.8 dev eth0 src 10.0.0.1\n",
        ),
    );
    let mut dbus = FakeDbus::new();
    for (path, model) in [
        ("/battery_mouse", "Mouse"),
        ("/battery_keyboard", "Keyboard"),
    ] {
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            getall_reply(path, &[("Percentage", "75"), ("Model", model)]),
        );
    }
    let mut lanes = TestOwners::default();
    lanes.intel_gpu.source_pci = Some(String::from("0000:00:02.0"));
    assert_eq!(
        gpu_intel::read_intel_gpu_metrics_once(
            &tree.proc(),
            &mut lanes.intel_gpu,
            "0000:00:02.0",
            clock(0),
        ),
        gpu_intel::IntelGpuReadOutcome::Baseline
    );
    let mut fake_clock = FakeClock::default();
    let mut capture_clock = || fake_clock.tick();

    let readings = run_collect_with_clock(
        &mut lanes,
        &mut hw,
        &cfg,
        &tree.proc(),
        &tree.sys(),
        &mut commands,
        &mut dbus,
        None,
        None,
        &mut capture_clock,
        false,
    );

    let captures = [
        lanes
            .network
            .info
            .latest
            .as_ref()
            .expect("network info")
            .captured_at,
        lanes.power.battery_sys_cache[&hw.battery_sys_ids[0]]
            .sampled_at
            .expect("first battery"),
        lanes.power.battery_sys_cache[&hw.battery_sys_ids[1]]
            .sampled_at
            .expect("second battery"),
        lanes.power.battery_mouse_cache.sampled_at.expect("mouse"),
        lanes.power.battery_kbd_cache.sampled_at.expect("keyboard"),
        lanes
            .intel_gpu
            .frequency
            .latest
            .as_ref()
            .expect("Intel frequency")
            .captured_at,
        lanes
            .intel_gpu
            .usage
            .latest
            .as_ref()
            .expect("Intel usage")
            .captured_at,
        lanes
            .external
            .brightness
            .latest
            .as_ref()
            .expect("brightness")
            .captured_at,
        lanes
            .external
            .updates
            .latest
            .as_ref()
            .expect("updates")
            .captured_at,
        lanes
            .external
            .server
            .latest
            .as_ref()
            .expect("server")
            .captured_at,
    ];
    assert!(captures.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(readings.assembled_at.monotonic > captures[captures.len() - 1]);
    assert_eq!(commands.call_trace().len(), 1);
    assert_eq!(dbus.call_trace().len(), 2);
}

#[test]
fn cpu_owner_invalid_delta_retains_sample_without_fabricating_history() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut cfg = cfg_panel(&["cpu_usage"]);
    cfg.display.history_interval = crate::domain::Cadence::from_millis(2000);
    let mut state = cpu::CpuState::default();
    let mut timings = None;

    let first = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &HardwareInventory::default(),
        &Default::default(),
        &mut fixed_clock(0),
        &mut timings,
    );
    assert_eq!(first.usage.status, AttemptStatus::Baseline);
    assert!(first.usage.sample.is_none());
    tree.write(
        "proc/stat",
        "cpu  30 0 20 100 0 0 0 0 0 0\n\
         cpu0 15 0 10 50 0 0 0 0 0 0\n\
         cpu1 15 0 10 50 0 0 0 0 0 0\n",
    );
    let second = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &HardwareInventory::default(),
        &Default::default(),
        &mut fixed_clock(1),
        &mut timings,
    );
    let measured = second.usage.sample.as_ref().expect("measured CPU sample");
    cpu::append_cpu_history(&mut state, &cfg, Duration::from_secs(1), measured.value);
    let invalid = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &HardwareInventory::default(),
        &Default::default(),
        &mut fixed_clock(2),
        &mut timings,
    );

    assert!(first.history.is_none());
    assert_eq!(second.usage.status, AttemptStatus::Captured);
    assert_eq!(invalid.usage.status, AttemptStatus::Baseline);
    let invalid_sample = invalid.usage.sample.expect("retained CPU sample");
    assert_eq!(invalid_sample.captured_at, Duration::from_secs(1));
    let invalid_history = invalid.history.expect("retained CPU history");
    assert_eq!(invalid_history.value.len(), 1);
    assert_eq!(invalid_history.captured_at, Duration::from_secs(1));
    assert_eq!(state.usage.failed_at, None);

    tree.write(
        "proc/stat",
        "cpu  50 0 30 120 0 0 0 0 0 0\n\
         cpu0 25 0 15 60 0 0 0 0 0 0\n\
         cpu1 25 0 15 60 0 0 0 0 0 0\n",
    );
    let recovered = attempt_cpu(
        &mut state,
        &tree.proc(),
        &tree.sys(),
        &HardwareInventory::default(),
        &Default::default(),
        &mut fixed_clock(3),
        &mut timings,
    );
    assert_eq!(recovered.usage.status, AttemptStatus::Captured);
    assert_eq!(state.cpu_history, vec![measured.value]);
}

#[test]
fn cpu_core_noncomparable_read_retains_sample_and_recovers_without_failure() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut state = cpu::CpuState::default();
    let mut timings = None;

    let first = attempt_cpu_cores(&mut state, &tree.proc(), clock(0), &mut timings);
    assert_eq!(first.usage.status, AttemptStatus::Baseline);
    tree.write(
        "proc/stat",
        "cpu  30 0 20 100 0 0 0 0 0 0\n\
         cpu0 15 0 10 50 0 0 0 0 0 0\n\
         cpu1 15 0 10 50 0 0 0 0 0 0\n",
    );
    let captured = attempt_cpu_cores(&mut state, &tree.proc(), clock(1), &mut timings);
    let sample = captured.usage.sample.clone().expect("CPU core sample");
    cpu::append_cpu_core_history(
        &mut state,
        &Config::default(),
        Duration::from_secs(1),
        &sample.value,
    );

    let baseline = attempt_cpu_cores(&mut state, &tree.proc(), clock(2), &mut timings);
    assert_eq!(baseline.usage.status, AttemptStatus::Baseline);
    assert_eq!(baseline.usage.sample, Some(sample));
    assert_eq!(baseline.usage.failed_at, None);
    assert_eq!(
        baseline.history.as_ref().map(|history| history.captured_at),
        Some(Duration::from_secs(1))
    );

    tree.write(
        "proc/stat",
        "cpu  50 0 30 120 0 0 0 0 0 0\n\
         cpu0 25 0 15 60 0 0 0 0 0 0\n\
         cpu1 25 0 15 60 0 0 0 0 0 0\n",
    );
    let recovered = attempt_cpu_cores(&mut state, &tree.proc(), clock(3), &mut timings);
    assert_eq!(recovered.usage.status, AttemptStatus::Captured);
    assert_eq!(
        recovered
            .history
            .as_ref()
            .map(|history| history.captured_at),
        Some(Duration::from_secs(1))
    );
}

#[test]
fn cpu_noncomparable_collection_does_not_append_aggregate_or_core_history() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut cfg = cfg_panel(&["cpu_usage"]);
    cfg.pages.order = vec![String::from("cpu_cores")];
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    let mut hw = HardwareInventory::default();
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();

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
        "proc/stat",
        "cpu  30 0 20 100 0 0 0 0 0 0\n\
         cpu0 15 0 10 50 0 0 0 0 0 0\n\
         cpu1 15 0 10 50 0 0 0 0 0 0\n",
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
        clock(1),
        false,
    );
    let aggregate_history = captured.cpu_history;
    let core_history = captured.cpu_core_history.expect("CPU core history");

    let baseline = run_collect(
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
    assert_eq!(baseline.cpu_history, aggregate_history);
    assert_eq!(baseline.cpu_core_history, Some(core_history));
    assert_eq!(lanes.cpu.usage.failed_at, None);
    assert_eq!(lanes.cpu.core_usage.failed_at, None);
}

#[test]
fn initial_failed_attempts_have_no_typed_history_samples() {
    let tree = TempTree::new();
    tree.mkdir("proc");
    let mut cpu_state = cpu::CpuState::default();
    let mut memory_state = memory::MemoryState::default();
    let mut network_state = crate::sensors::network::NetworkState::default();
    let mut timings = None;

    let cpu = attempt_cpu(
        &mut cpu_state,
        &tree.proc(),
        &tree.sys(),
        &HardwareInventory::default(),
        &BTreeSet::new(),
        &mut fixed_clock(1),
        &mut timings,
    );
    let memory = attempt_memory(
        &mut memory_state,
        &tree.proc(),
        false,
        &mut fixed_clock(1),
        &mut timings,
    );
    let network = attempt_network_speed(
        &mut network_state,
        &tree.sys(),
        "eth0",
        clock(1),
        &mut timings,
    );

    assert_eq!(cpu.usage.status, AttemptStatus::Failed);
    assert!(cpu.history.is_none());
    assert_eq!(memory.usage.status, AttemptStatus::Failed);
    assert!(memory.history.is_none());
    assert_eq!(network.reading.status, AttemptStatus::Failed);
    assert!(network.up_history.is_none());
    assert!(network.down_history.is_none());
}

#[test]
fn cpu_and_core_failures_append_retained_values_at_due_history_deadline() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut cfg = cfg_panel(&["cpu_usage"]);
    cfg.pages.order = vec![String::from("cpu_cores")];
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    let mut hw = HardwareInventory::default();
    let mut lanes = TestOwners::default();
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();

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
        "proc/stat",
        "cpu  30 0 20 100 0 0 0 0 0 0\n\
         cpu0 15 0 10 50 0 0 0 0 0 0\n\
         cpu1 15 0 10 50 0 0 0 0 0 0\n",
    );
    let valid = run_collect(
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
    let history = valid.cpu_history.clone();
    let core_history = valid.cpu_core_history.clone();

    tree.write("proc/stat", "malformed\n");
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
        clock(2),
        false,
    );

    assert_eq!(failed.cpu_usage, valid.cpu_usage);
    assert_eq!(failed.cpu_core_usage, valid.cpu_core_usage);
    assert_eq!(failed.cpu_history.len(), history.len() + 1);
    assert_eq!(failed.cpu_history.last().copied(), valid.cpu_usage);
    let failed_core_history = failed.cpu_core_history.expect("failed core history");
    let valid_core_usage = valid.cpu_core_usage.expect("valid core usage");
    let previous_core_history = core_history.expect("previous core history");
    for ((carried, previous), usage) in failed_core_history
        .iter()
        .zip(previous_core_history.iter())
        .zip(valid_core_usage)
    {
        assert_eq!(carried.len(), previous.len() + 1);
        assert_eq!(carried.last().copied(), Some(usage));
    }
    assert_eq!(
        lanes
            .cpu
            .usage
            .latest
            .as_ref()
            .expect("CPU sample")
            .captured_at,
        Duration::from_secs(1)
    );
    assert_eq!(lanes.cpu.usage.attempted_at, Some(Duration::from_secs(2)));
    assert_eq!(
        lanes
            .cpu
            .core_usage
            .latest
            .as_ref()
            .expect("core sample")
            .captured_at,
        Duration::from_secs(1)
    );
}

#[test]
fn swap_failure_retains_but_confirmed_zero_clears_sample() {
    let tree = TempTree::new();
    baseline_proc(&tree);
    let mut state = memory::MemoryState::default();
    let mut timings = None;

    let first = attempt_memory(
        &mut state,
        &tree.proc(),
        true,
        &mut fixed_clock(0),
        &mut timings,
    );
    assert_eq!(first.swap_usage.sample.expect("swap sample").value, 75);

    tree.write(
        "proc/meminfo",
        "MemTotal: 2097152 kB\nMemFree: 524288 kB\nSwapTotal: 1048576 kB\n",
    );
    let failed = attempt_memory(
        &mut state,
        &tree.proc(),
        true,
        &mut fixed_clock(1),
        &mut timings,
    );
    assert_eq!(failed.swap_usage.status, AttemptStatus::Failed);
    assert_eq!(failed.swap_usage.sample.expect("retained swap").value, 75);
    assert_eq!(state.swap_usage.attempted_at, Some(Duration::from_secs(1)));

    tree.write(
        "proc/meminfo",
        "MemTotal: 2097152 kB\nMemFree: 524288 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n",
    );
    let absent = attempt_memory(
        &mut state,
        &tree.proc(),
        true,
        &mut fixed_clock(2),
        &mut timings,
    );
    assert_eq!(absent.swap_usage.status, AttemptStatus::Absent);
    assert!(absent.swap_usage.sample.is_none());
    assert!(state.swap_usage.latest.is_none());
}

// ── discover_hardware ────────────────────────────────────────────────────────
