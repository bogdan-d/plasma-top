#![allow(clippy::expect_used)]
use super::*;
use crate::domain::boundary::{
    BoundaryError, CommandOutput, CommandStatus, DbusOutput, DbusRequest, NotificationError,
    NotificationPayload,
};
use crate::error::CriticalService;
use std::cell::Cell;
use std::collections::VecDeque;

mod production_executor;

#[test]
fn rgb_and_luma_match_python_boundaries() {
    assert_eq!(parse_rgb("1, 2,3,255"), Some((1, 2, 3)));
    assert_eq!(parse_rgb("bad"), None);
    assert!(!is_light_rgb((127, 127, 127)));
    assert!(is_light_rgb((255, 255, 255)));
}

#[test]
fn critical_service_exit_is_a_daemon_error_after_cleanup() {
    let error = complete_daemon_run(Ok(()), Some(CriticalService::SystemDbus), false)
        .expect_err("critical system service exit must fail the daemon");
    assert_eq!(
        error.to_string(),
        "critical system D-Bus service exited unexpectedly"
    );
    assert!(matches!(
        error,
        Error::CriticalService(CriticalService::SystemDbus)
    ));
}

#[test]
fn shutdown_timeout_is_a_daemon_error_after_cleanup() {
    let error = complete_daemon_run(Ok(()), None, true)
        .expect_err("over-budget I/O shutdown must fail the daemon");
    assert!(matches!(error, Error::CriticalShutdownTimeout));
}

#[test]
fn timed_profile_preserves_report_with_shutdown_error() {
    let session = ProfileSession::default();
    let outcome = complete_timed_profile(
        &session,
        Duration::from_millis(25),
        "hidden",
        Ok(()),
        None,
        true,
    );

    assert!(outcome.report.starts_with("profile:\n"));
    assert!(outcome.report.contains("  scenario: hidden\n"));
    assert!(
        outcome
            .report
            .contains("  final_status: error (critical async I/O shutdown timed out)")
    );
    assert!(matches!(
        outcome.error,
        Some(Error::CriticalShutdownTimeout)
    ));
}

#[test]
fn profiling_cleanup_removes_temp_root_during_unwind() {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-profile-cleanup-test-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("profiling temp root");

    let unwind = std::panic::catch_unwind(|| {
        let _cleanup = ProfilingCleanup(root.clone());
        panic!("exercise cleanup guard");
    });

    assert!(unwind.is_err());
    assert!(!root.exists());
}

#[test]
fn cleanup_preserves_last_good_tooltip() {
    let (root, _roots, paths, _config_path) = integration_tree();
    fs::create_dir_all(&paths.state).expect("runtime state");
    fs::write(&paths.panel, "panel").expect("panel");
    fs::write(&paths.tooltip, "tooltip").expect("tooltip");
    fs::write(&paths.page, "0").expect("page");
    fs::write(&paths.npages, "1").expect("npages");

    cleanup(&paths);

    assert!(!paths.panel.exists());
    assert!(fs::read_to_string(&paths.tooltip).is_ok_and(|text| text == "tooltip"));
    assert!(!paths.page.exists());
    assert!(!paths.npages.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn css_comments_and_whitespace_are_stripped() {
    let dir = std::env::temp_dir().join(format!("plasma-top-css-{}", std::process::id()));
    let _ = fs::create_dir_all(&dir);
    let path = dir.join("style.css");
    fs::write(&path, "/* note */\n.a {\n color: red;\n}").expect("write fixture");
    assert_eq!(read_css_file(&path), ".a { color: red; }");
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn footer_is_inserted_inside_tooltip_root() {
    assert_eq!(
        insert_before_tooltip_close("<div class=\"tooltip\">x</div>".into(), "p"),
        "<div class=\"tooltip\">xp</div>"
    );
}

struct AbsentCommand {
    calls: Cell<usize>,
}

impl CommandRunner for AbsentCommand {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        _timeout: Duration,
    ) -> std::result::Result<CommandOutput, BoundaryError> {
        self.calls.set(self.calls.get().saturating_add(1));
        Err(BoundaryError::CommandFailed {
            program: program.to_path_buf(),
            args: args.to_vec(),
            detail: "fixture absent".into(),
        })
    }
}

struct AbsentDbus;

impl DbusFacade for AbsentDbus {
    fn call(&mut self, request: DbusRequest) -> std::result::Result<DbusOutput, BoundaryError> {
        let (bus, service, path, interface, member) = request.metadata();
        Err(BoundaryError::DbusCallFailed {
            bus,
            service: service.to_owned(),
            path: path.to_owned(),
            interface: interface.to_owned(),
            member: member.to_owned(),
            detail: "fixture absent".into(),
        })
    }
}

struct RecordingNotifications;

impl NotificationFacade for RecordingNotifications {
    fn send(
        &mut self,
        _payload: &NotificationPayload,
    ) -> std::result::Result<(), NotificationError> {
        Ok(())
    }
}

struct CountingNotifications {
    calls: usize,
}

impl NotificationFacade for CountingNotifications {
    fn send(
        &mut self,
        _payload: &NotificationPayload,
    ) -> std::result::Result<(), NotificationError> {
        self.calls = self.calls.saturating_add(1);
        Ok(())
    }
}

struct FakeControl {
    now: Duration,
    paths: DaemonPaths,
    config: PathBuf,
    sleeps: usize,
    saw_panel: bool,
    saw_tooltip: bool,
    saw_page_one: bool,
    io_events: VecDeque<IoEvent>,
    drained_io_events: usize,
}

impl LoopControl for FakeControl {
    fn snapshot(&mut self) -> ClockSnapshot {
        ClockSnapshot {
            monotonic: self.now,
            wall: UNIX_EPOCH + self.now,
        }
    }

    fn sleep(&mut self, duration: Duration) {
        self.now = self.now.saturating_add(duration);
        self.sleeps = self.sleeps.saturating_add(1);
        self.saw_panel |=
            fs::read_to_string(&self.paths.panel).is_ok_and(|html| html.contains("class=\"panel"));
        self.saw_tooltip |= fs::read_to_string(&self.paths.tooltip)
            .is_ok_and(|html| html.contains("class=\"tooltip"));
        if self.sleeps == 1 {
            fs::write(&self.paths.page, "1").expect("step fixture page");
            fs::write(&self.config, "[display]\npoll_interval = 0.05\n")
                .expect("write invalid cadence");
        } else if fs::read_to_string(&self.paths.tooltip)
            .is_ok_and(|html| html.contains("CPU CORES"))
        {
            self.saw_page_one = true;
        }
    }

    fn should_stop(&self) -> bool {
        false
    }

    fn drain_io_events(&mut self) -> Vec<IoEvent> {
        let events = self.io_events.drain(..).collect::<Vec<_>>();
        self.drained_io_events = self.drained_io_events.saturating_add(events.len());
        events
    }
}

fn integration_tree() -> (PathBuf, FilesystemRoots, DaemonPaths, PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-daemon-{}-{:?}",
        std::process::id(),
        thread::current().id()
    ));
    let proc_root = root.join("proc");
    let sys_root = root.join("sys");
    fs::create_dir_all(&proc_root).expect("proc fixture root");
    fs::create_dir_all(&sys_root).expect("sys fixture root");
    fs::write(proc_root.join("stat"), "cpu  1 0 1 8 0 0 0 0\n").expect("proc stat");
    fs::write(
        proc_root.join("meminfo"),
        "MemTotal: 1024000 kB\nMemAvailable: 512000 kB\nSwapTotal: 0 kB\nSwapFree: 0 kB\n",
    )
    .expect("meminfo");
    let runtime = root.join("run/plasma-top");
    let state = runtime.join("state");
    let paths = DaemonPaths {
        runtime: runtime.clone(),
        state: state.clone(),
        panel: runtime.join("panel.html"),
        tooltip: runtime.join("tooltip.html"),
        page: state.join("page"),
        npages: state.join("npages"),
        geom: state.join("geom"),
        plasma_config: root.join("appletsrc"),
        kdeglobals: root.join("kdeglobals"),
    };
    let config = root.join("config.toml");
    fs::write(
        &config,
        "[display]\npoll_interval = 0.25\n[panel]\norder = []\n[tooltip]\norder = []\n[pages]\norder = [\"cpu_cores\"]\n",
    )
    .expect("config fixture");
    let roots = FilesystemRoots {
        runtime_root: Some(runtime),
        cache_root: Some(root.join("cache")),
        config_root: Some(root.join("config")),
        proc_root,
        sys_root,
    };
    (root, roots, paths, config)
}

fn process_stat_line(pid: u32, total_jiffies: u64) -> String {
    let mut fields = vec![String::from("0"); 22];
    fields[11] = total_jiffies.to_string();
    fields[12] = String::from("0");
    fields[21] = String::from("100");
    format!("{pid} (worker) {}\n", fields.join(" "))
}

#[test]
fn process_page_merge_uses_attempt_time_and_stamps_each_completed_snapshot() {
    let (root, roots, _paths, _config) = integration_tree();
    let pid_dir = roots.proc_root.join("7");
    fs::create_dir_all(&pid_dir).expect("process fixture");
    fs::write(pid_dir.join("stat"), process_stat_line(7, 200)).expect("process stat");
    let mut process = ProcessState::default();
    process.proc_prev_times.insert(7, 100);
    process.proc_prev_sample_at = Some(Duration::from_secs(1));
    let mut readings = DisplaySnapshot {
        assembled_at: ClockSnapshot {
            monotonic: Duration::from_secs(2),
            wall: UNIX_EPOCH + Duration::from_secs(2),
        },
        ..DisplaySnapshot::default()
    };
    let mut snapshots = VecDeque::from([clock_snapshot(10), clock_snapshot(11)]);

    merge_process_page_sample(&mut readings, &roots.proc_root, &mut process, &mut || {
        snapshots.pop_front().expect("scripted snapshot")
    });

    assert_eq!(
        readings
            .top_process_full
            .as_ref()
            .map(|rows| rows[0].cpu_percent),
        Some(11),
        "the process diff must use the clock sampled immediately before its scan"
    );
    assert_eq!(readings.assembled_at, clock_snapshot(11));

    fs::write(pid_dir.join("stat"), process_stat_line(7, 300)).expect("updated process stat");
    let mut snapshots = VecDeque::from([clock_snapshot(20), clock_snapshot(21)]);
    merge_process_page_sample(&mut readings, &roots.proc_root, &mut process, &mut || {
        snapshots.pop_front().expect("scripted wake snapshot")
    });
    assert_eq!(readings.assembled_at, clock_snapshot(21));

    fs::remove_dir_all(&pid_dir).expect("remove process fixture");
    let mut snapshots = VecDeque::from([clock_snapshot(30), clock_snapshot(31)]);
    merge_process_page_sample(&mut readings, &roots.proc_root, &mut process, &mut || {
        snapshots.pop_front().expect("scripted empty snapshot")
    });
    assert!(
        readings.top_process_full.is_none(),
        "a successful empty page scan must clear stale process rows"
    );
    assert_eq!(readings.assembled_at, clock_snapshot(31));
    let _ = fs::remove_dir_all(root);
}

fn clock_snapshot(seconds: u64) -> ClockSnapshot {
    ClockSnapshot {
        monotonic: Duration::from_secs(seconds),
        wall: UNIX_EPOCH + Duration::from_secs(seconds),
    }
}

struct SequencedCommand {
    results: VecDeque<std::result::Result<CommandOutput, BoundaryError>>,
}

impl CommandRunner for SequencedCommand {
    fn run(
        &mut self,
        _program: &Path,
        _args: &[OsString],
        _timeout: Duration,
    ) -> std::result::Result<CommandOutput, BoundaryError> {
        self.results.pop_front().expect("queued command result")
    }
}

#[test]
fn config_page_removal_prunes_command_owner_before_failed_readd() {
    let (root, _roots, paths, _config_path) = integration_tree();
    fs::create_dir_all(&paths.state).expect("runtime state");
    let mut configured = Config::default();
    configured.pages.order = vec![String::from("fastfetch")];
    let mut active = build_pages(&configured.pages.order);
    let page = active[1];
    let mut cache = PageCommandCache::new();
    let mut lookup = CommandLookup::new();
    lookup.insert("fastfetch", "/usr/bin/fastfetch");
    let mut runner = SequencedCommand {
        results: VecDeque::from([Ok(CommandOutput {
            program: PathBuf::from("/usr/bin/fastfetch"),
            args: Vec::new(),
            status: CommandStatus::Exit(0),
            stdout: b"pre-removal\n".to_vec(),
            stderr: Vec::new(),
            truncation: Default::default(),
        })]),
    };
    assert_eq!(
        crate::page_commands::run_command(&page, &mut runner, &lookup, &mut cache, Duration::ZERO,),
        "pre-removal"
    );

    let mut process = ProcessState::default();
    let mut removed = Config::default();
    removed.pages.order.clear();
    replace_page_registry(&paths, &removed, &mut active, &mut cache, &mut process)
        .expect("remove page registry");
    assert!(cache.state("fastfetch").is_none());

    replace_page_registry(&paths, &configured, &mut active, &mut cache, &mut process)
        .expect("re-add page registry");
    runner.results.push_back(Err(BoundaryError::CommandFailed {
        program: PathBuf::from("/usr/bin/fastfetch"),
        args: Vec::new(),
        detail: String::from("first failure"),
    }));
    assert_eq!(
        crate::page_commands::run_command(
            &active[1],
            &mut runner,
            &lookup,
            &mut cache,
            Duration::from_secs(1),
        ),
        "fastfetch: first failure"
    );
    assert!(
        cache
            .state("fastfetch")
            .is_some_and(|state| state.latest.is_none())
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn config_process_page_removal_resets_sample_and_baseline_before_failed_readd() {
    let (root, roots, paths, _config_path) = integration_tree();
    fs::create_dir_all(&paths.state).expect("runtime state");
    let mut configured = Config::default();
    configured.pages.order = vec![String::from("processes")];
    let mut active = build_pages(&configured.pages.order);
    let mut process = ProcessState::default();
    process
        .page
        .record_value(Vec::new(), Duration::from_secs(1));
    process.page_proc_prev_times.insert(7, 42);
    process.page_proc_prev_sample_at = Some(Duration::from_secs(1));

    let mut removed = Config::default();
    removed.pages.order.clear();
    replace_page_registry(
        &paths,
        &removed,
        &mut active,
        &mut PageCommandCache::new(),
        &mut process,
    )
    .expect("remove process page");
    assert!(process.page.latest.is_none());
    assert!(process.page_proc_prev_times.is_empty());
    assert_eq!(process.page_proc_prev_sample_at, None);

    replace_page_registry(
        &paths,
        &configured,
        &mut active,
        &mut PageCommandCache::new(),
        &mut process,
    )
    .expect("re-add process page");
    let failed = crate::sensors::process::read_top_process_page_attempt(
        &roots.proc_root.join("missing"),
        &mut process,
        ClockSnapshot {
            monotonic: Duration::from_secs(2),
            wall: UNIX_EPOCH,
        },
    );
    assert_eq!(
        failed.status,
        crate::sensors::process::ProcessPageStatus::Failed
    );
    assert!(failed.sample.is_none());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn failed_carried_temperature_and_load_do_not_complete_sustained_alerts() {
    let (root, roots, _paths, _config) = integration_tree();
    let temp_path = roots.sys_root.join("cpu-temp");
    fs::write(&temp_path, "85000\n").expect("CPU temperature fixture");
    fs::write(roots.proc_root.join("loadavg"), "0.0 0.0 2.0 1/1 1\n").expect("load fixture");
    let mut cfg = Config::default();
    cfg.notifications.disk_usage = false;
    cfg.notifications.disk_smart = false;
    cfg.notifications.cpu_temp = true;
    cfg.notifications.gpu_nvidia_temp = false;
    cfg.notifications.hd_temp = false;
    cfg.notifications.battery_sys = false;
    cfg.notifications.battery_mouse = false;
    cfg.notifications.battery_kbd = false;
    cfg.notifications.server_check = false;
    cfg.notifications.load_avg = true;
    cfg.notify_thresholds.cpu_temp = 80;
    cfg.notify_thresholds.temp_sustain_seconds = 60;
    cfg.notify_thresholds.load_avg_15 = 1.0;
    cfg.notify_thresholds.load_avg_minutes = 1;
    let mut hw = HardwareInventory {
        cpu_temp_path: Some(temp_path.clone()),
        cpu_count: 1,
        ..HardwareInventory::default()
    };
    let mut cpu_owner = cpu::CpuState::default();
    let mut memory_owner = memory::MemoryState::default();
    let mut network_owner = network::NetworkState::default();
    let mut disk_owner = disk::DiskState::default();
    let mut process_owner = ProcessState::default();
    let mut amd_gpu_owner = crate::sensors::gpu_amd::AmdGpuState::default();
    let mut intel_gpu_owner = gpu_intel::IntelGpuState::default();
    let mut power_owner = power::PowerState::default();
    let mut nvidia_owner = gpu_nvidia::NvidiaState::default();
    let mut gpu_history_owner = gpu_history::GpuHistoryState::default();
    let mut external_owner = external::ExternalState::default();
    let mut commands = AbsentCommand {
        calls: Cell::new(0),
    };
    let mut dbus = AbsentDbus;

    let mut first_clock = || ClockSnapshot {
        monotonic: Duration::ZERO,
        wall: UNIX_EPOCH,
    };
    let mut first_ctx = CollectCtx::new(&roots, &mut commands, &mut dbus, &mut first_clock);
    let first = collect_with_notifications(
        OwnerRefs {
            cpu: &mut cpu_owner,
            memory: &mut memory_owner,
            network: &mut network_owner,
            disk: &mut disk_owner,
            process: &mut process_owner,
            amd_gpu: &mut amd_gpu_owner,
            intel_gpu: &mut intel_gpu_owner,
            power: &mut power_owner,
            nvidia: &mut nvidia_owner,
            gpu_history: &mut gpu_history_owner,
            external: &mut external_owner,
        },
        &mut hw,
        &cfg,
        &mut first_ctx,
        None,
    );
    let mut state = NotificationState::default();
    let mut notifications = CountingNotifications { calls: 0 };
    let _ = check_and_notify(
        &first.notifications,
        &cfg,
        &mut state,
        &hw,
        Duration::ZERO,
        &mut notifications,
    );

    fs::remove_file(temp_path).expect("remove temperature source");
    fs::remove_file(roots.proc_root.join("loadavg")).expect("remove load source");
    let mut failed_clock = || ClockSnapshot {
        monotonic: Duration::from_secs(60),
        wall: UNIX_EPOCH + Duration::from_secs(60),
    };
    let mut failed_ctx = CollectCtx::new(&roots, &mut commands, &mut dbus, &mut failed_clock);
    let failed = collect_with_notifications(
        OwnerRefs {
            cpu: &mut cpu_owner,
            memory: &mut memory_owner,
            network: &mut network_owner,
            disk: &mut disk_owner,
            process: &mut process_owner,
            amd_gpu: &mut amd_gpu_owner,
            intel_gpu: &mut intel_gpu_owner,
            power: &mut power_owner,
            nvidia: &mut nvidia_owner,
            gpu_history: &mut gpu_history_owner,
            external: &mut external_owner,
        },
        &mut hw,
        &cfg,
        &mut failed_ctx,
        None,
    );
    assert_eq!(failed.display.cpu_temp, Some(85));
    assert_eq!(
        failed.display.load_average.map(|load| load.fifteen),
        Some(2.0)
    );
    assert_eq!(failed.notifications.cpu_temp, None);
    assert_eq!(failed.notifications.load_average, None);

    let _ = check_and_notify(
        &failed.notifications,
        &cfg,
        &mut state,
        &hw,
        Duration::from_secs(60),
        &mut notifications,
    );
    assert_eq!(notifications.calls, 0);
    assert_eq!(state.cpu_temp.since, Some(Duration::ZERO));
    assert_eq!(state.load_avg.since, Some(Duration::ZERO));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn isolated_lifecycle_paints_wakes_keeps_last_good_and_cleans_up() {
    let (root, roots, paths, config) = integration_tree();
    let mut commands = AbsentCommand {
        calls: Cell::new(0),
    };
    let mut dbus = AbsentDbus;
    let mut notifications = RecordingNotifications;
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        nvml: None,
        bolt: None,
    };
    let mut control = FakeControl {
        now: Duration::ZERO,
        paths: paths.clone(),
        config: config.clone(),
        sleeps: 0,
        saw_panel: false,
        saw_tooltip: false,
        saw_page_one: false,
        io_events: VecDeque::from([
            IoEvent::UpowerChanged,
            IoEvent::UpowerChanged,
            IoEvent::PrepareForSleep(true),
            IoEvent::PrepareForSleep(false),
        ]),
        drained_io_events: 0,
    };

    let result = run_daemon_with(
        Some(&config),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        Some(2),
    );

    assert!(result.is_ok(), "{result:?}");
    assert!(control.saw_panel && control.saw_tooltip);
    assert_eq!(control.drained_io_events, 4);
    assert!(control.saw_page_one, "page wake did not republish tooltip");
    assert!(
        commands.calls.get() > 0,
        "production call path not exercised"
    );
    for path in [&paths.panel, &paths.page, &paths.npages] {
        assert!(!path.exists(), "cleanup left {}", path.display());
    }
    assert!(paths.tooltip.exists(), "cleanup removed retained tooltip");
    let _ = fs::remove_dir_all(root);
}
