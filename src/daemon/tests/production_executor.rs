use super::*;
use crate::sensors::gpu_nvidia::{NvmlError, NvmlFacade, NvmlMetrics, NvmlOptionalMetric};
use std::cell::RefCell;
use std::rc::Rc;

struct SharedControl {
    now: Rc<Cell<Duration>>,
    paths: DaemonPaths,
    frames: Rc<RefCell<Vec<(Duration, String)>>>,
    stop_at: Option<Duration>,
}

struct ThemeTimeoutCommand {
    now: Rc<Cell<Duration>>,
    paths: DaemonPaths,
    calls: usize,
}

impl CommandRunner for ThemeTimeoutCommand {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        timeout: Duration,
    ) -> std::result::Result<CommandOutput, BoundaryError> {
        assert_eq!(program, Path::new("kreadconfig6"));
        assert_eq!(timeout, Duration::from_secs(2));
        assert!(
            fs::read_to_string(&self.paths.panel).is_ok_and(|html| html.contains("class=\"panel")),
            "theme command preceded panel publication"
        );
        assert!(
            fs::read_to_string(&self.paths.tooltip)
                .is_ok_and(|html| html.contains("class=\"tooltip")),
            "theme command preceded tooltip publication"
        );
        self.calls = self.calls.saturating_add(1);
        self.now.set(self.now.get().saturating_add(timeout));
        Err(BoundaryError::CommandFailed {
            program: program.to_path_buf(),
            args: args.to_vec(),
            detail: String::from("fake timeout"),
        })
    }
}

struct ReloadControl {
    now: Rc<Cell<Duration>>,
    config: PathBuf,
    replacement: String,
    reload_armed: Rc<Cell<bool>>,
    clock_logged: Cell<bool>,
    events: Rc<RefCell<Vec<&'static str>>>,
}

impl LoopControl for ReloadControl {
    fn snapshot(&mut self) -> ClockSnapshot {
        if self.reload_armed.get() && !self.clock_logged.replace(true) {
            self.events.borrow_mut().push("deadline-observed");
        }
        let now = self.now.get();
        ClockSnapshot {
            monotonic: now,
            wall: UNIX_EPOCH + now,
        }
    }

    fn sleep(&mut self, duration: Duration) {
        if !self.reload_armed.replace(true) {
            fs::write(&self.config, &self.replacement).expect("reload config mutation");
            self.now.set(Duration::from_millis(300));
        } else {
            self.now.set(self.now.get().saturating_add(duration));
        }
    }

    fn should_stop(&self) -> bool {
        false
    }
}

struct ThemeOrderingControl {
    now: Rc<Cell<Duration>>,
    paths: DaemonPaths,
    config: PathBuf,
    replacement: String,
    kdeglobals: PathBuf,
    armed: Rc<Cell<bool>>,
}

impl LoopControl for ThemeOrderingControl {
    fn snapshot(&mut self) -> ClockSnapshot {
        if !self.armed.get() && self.paths.panel.exists() {
            fs::write(&self.config, &self.replacement).expect("theme ordering config mutation");
            fs::write(
                &self.kdeglobals,
                "[Colors:Window]\nBackgroundNormal=255,255,255\n",
            )
            .expect("theme ordering KDE mutation");
            self.armed.set(true);
        }
        let now = self.now.get();
        ClockSnapshot {
            monotonic: now,
            wall: UNIX_EPOCH + now,
        }
    }

    fn sleep(&mut self, duration: Duration) {
        self.now.set(self.now.get().saturating_add(duration));
    }

    fn should_stop(&self) -> bool {
        false
    }
}

struct ThemeOrderingCommand {
    paths: DaemonPaths,
    armed: Rc<Cell<bool>>,
    calls: usize,
}

impl CommandRunner for ThemeOrderingCommand {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        timeout: Duration,
    ) -> std::result::Result<CommandOutput, BoundaryError> {
        assert_eq!(program, Path::new("kreadconfig6"));
        assert_eq!(timeout, Duration::from_secs(2));
        if self.armed.get() {
            assert!(
                fs::read_to_string(&self.paths.panel).is_ok_and(|html| html.contains("form-bar")),
                "theme reconciliation ran before the changed config was published"
            );
        }
        self.calls = self.calls.saturating_add(1);
        Ok(CommandOutput {
            program: program.to_path_buf(),
            args: args.to_vec(),
            status: CommandStatus::Exit(0),
            stdout: b"35,38,41\n".to_vec(),
            stderr: Vec::new(),
            truncation: Default::default(),
        })
    }
}

struct SlowReloadCommand {
    now: Rc<Cell<Duration>>,
    paths: DaemonPaths,
    reload_armed: Rc<Cell<bool>>,
    events: Rc<RefCell<Vec<&'static str>>>,
    route_calls: usize,
}

impl CommandRunner for SlowReloadCommand {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        timeout: Duration,
    ) -> std::result::Result<CommandOutput, BoundaryError> {
        if program == Path::new("kreadconfig6") {
            return Ok(CommandOutput {
                program: program.to_path_buf(),
                args: args.to_vec(),
                status: CommandStatus::Exit(0),
                stdout: b"35,38,41\n".to_vec(),
                stderr: Vec::new(),
                truncation: Default::default(),
            });
        }
        if program == Path::new("ip")
            && args.first().is_some_and(|arg| arg == "route")
            && self.reload_armed.get()
        {
            assert_eq!(timeout, crate::sensors::NETWORK_COMMAND_TIMEOUT);
            assert_eq!(self.events.borrow().first(), Some(&"deadline-observed"));
            assert!(
                fs::read_to_string(&self.paths.panel).is_ok_and(|html| html.contains("form-bar")),
                "slow inventory ran before the reloaded config was published"
            );
            self.events.borrow_mut().push("slow-inventory");
            self.route_calls = self.route_calls.saturating_add(1);
            self.now
                .set(self.now.get().saturating_add(Duration::from_secs(2)));
            return Ok(CommandOutput {
                program: program.to_path_buf(),
                args: args.to_vec(),
                status: CommandStatus::Exit(0),
                stdout: b"8.8.8.8 via 192.0.2.1 dev wlan0\n".to_vec(),
                stderr: Vec::new(),
                truncation: Default::default(),
            });
        }
        Err(BoundaryError::CommandFailed {
            program: program.to_path_buf(),
            args: args.to_vec(),
            detail: String::from("fixture absent"),
        })
    }
}

impl LoopControl for SharedControl {
    fn snapshot(&mut self) -> ClockSnapshot {
        let now = self.now.get();
        if let Ok(panel) = fs::read_to_string(&self.paths.panel) {
            self.frames.borrow_mut().push((now, panel));
        }
        ClockSnapshot {
            monotonic: now,
            wall: UNIX_EPOCH + now,
        }
    }

    fn sleep(&mut self, duration: Duration) {
        self.now.set(self.now.get().saturating_add(duration));
    }

    fn should_stop(&self) -> bool {
        self.stop_at.is_some_and(|stop| self.now.get() >= stop)
    }
}

struct FirstPaintDbus {
    now: Rc<Cell<Duration>>,
    paths: DaemonPaths,
    observed: Rc<Cell<bool>>,
}

impl DbusFacade for FirstPaintDbus {
    fn call(&mut self, request: DbusRequest) -> std::result::Result<DbusOutput, BoundaryError> {
        if !self.observed.get() {
            assert!(self.now.get() <= Duration::from_millis(200));
            assert!(
                self.paths.panel.exists(),
                "panel was not painted before D-Bus"
            );
            assert!(
                self.paths.tooltip.exists(),
                "retained tooltip was not painted before D-Bus"
            );
            self.observed.set(true);
            self.now.set(Duration::from_millis(500));
        }
        let (bus, service, path, interface, member) = request.metadata();
        Err(BoundaryError::DbusCallFailed {
            bus,
            service: service.to_owned(),
            path: path.to_owned(),
            interface: interface.to_owned(),
            member: member.to_owned(),
            detail: String::from("blocked fixture"),
        })
    }
}

struct NvidiaCommand {
    smi_calls: usize,
}

impl CommandRunner for NvidiaCommand {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        _timeout: Duration,
    ) -> std::result::Result<CommandOutput, BoundaryError> {
        if program == Path::new("nvidia-smi") {
            self.smi_calls = self.smi_calls.saturating_add(1);
            return Ok(CommandOutput {
                program: program.to_path_buf(),
                args: args.to_vec(),
                status: CommandStatus::Exit(0),
                stdout: b"60, 50, 30, 40, 5\n".to_vec(),
                stderr: Vec::new(),
                truncation: Default::default(),
            });
        }
        Err(BoundaryError::CommandFailed {
            program: program.to_path_buf(),
            args: args.to_vec(),
            detail: String::from("fixture absent"),
        })
    }
}

struct ScriptedNvml {
    now: Rc<Cell<Duration>>,
    result: std::result::Result<NvmlMetrics, NvmlError>,
    first_delay: Duration,
    calls: usize,
}

impl NvmlFacade for ScriptedNvml {
    fn read_device_zero(&mut self) -> std::result::Result<NvmlMetrics, NvmlError> {
        self.calls = self.calls.saturating_add(1);
        if self.calls == 1 {
            self.now
                .set(self.now.get().saturating_add(self.first_delay));
        }
        self.result
    }
}

fn nvml_metrics(temp: i32) -> NvmlMetrics {
    NvmlMetrics {
        temp_celsius: temp,
        usage_percent: 40,
        memory_percent: 20,
        decoder: NvmlOptionalMetric::NotSupported,
        fan: NvmlOptionalMetric::NotSupported,
    }
}

fn add_nvidia_device(roots: &FilesystemRoots) {
    let device = roots.sys_root.join("bus/pci/devices/0000:01:00.0");
    fs::create_dir_all(&device).expect("PCI fixture");
    fs::write(device.join("vendor"), "0x10de\n").expect("vendor fixture");
    fs::write(device.join("class"), "0x030000\n").expect("class fixture");
}

fn configure_nvidia(config: &Path) {
    fs::write(
        config,
        "[display]\npoll_interval = 0.25\n[panel]\norder = [\"gpu\"]\n[panel.gpu]\nitems = [\"gpu_nvidia_temp\"]\n[tooltip]\norder = []\n[pages]\norder = []\n",
    )
    .expect("NVIDIA config fixture");
}

fn shared_control(now: Rc<Cell<Duration>>, paths: &DaemonPaths) -> SharedControl {
    SharedControl {
        now,
        paths: paths.clone(),
        frames: Rc::new(RefCell::new(Vec::new())),
        stop_at: None,
    }
}

#[test]
fn theme_command_timeout_starts_after_panel_and_tooltip_first_paint() {
    let (root, roots, paths, config) = integration_tree();
    let now = Rc::new(Cell::new(Duration::ZERO));
    let mut control = shared_control(Rc::clone(&now), &paths);
    let mut commands = ThemeTimeoutCommand {
        now,
        paths: paths.clone(),
        calls: 0,
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

    run_daemon_with(
        Some(&config),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        Some(1),
    )
    .expect("daemon run");

    assert_eq!(commands.calls, 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reload_observes_elapsed_deadline_and_applies_config_before_slow_inventory() {
    let (root, roots, paths, config) = integration_tree();
    fs::write(
        &config,
        "[display]\npoll_interval = 0.25\n[panel]\norder = [\"main\"]\n[panel.main]\nitems = [\"mem_usage\"]\n[tooltip]\norder = []\n[pages]\norder = []\n",
    )
    .expect("initial reload config");
    let replacement = String::from(
        "[display]\npoll_interval = 0.25\n[panel]\norder = [\"main\"]\n[panel.main]\nitems = [\"mem_usage:bar\"]\n[tooltip]\norder = [\"network\"]\n[tooltip.network]\nitems = [\"net_device\"]\n[pages]\norder = []\n",
    );
    let now = Rc::new(Cell::new(Duration::ZERO));
    let reload_armed = Rc::new(Cell::new(false));
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut control = ReloadControl {
        now: Rc::clone(&now),
        config: config.clone(),
        replacement,
        reload_armed: Rc::clone(&reload_armed),
        clock_logged: Cell::new(false),
        events: Rc::clone(&events),
    };
    let mut commands = SlowReloadCommand {
        now,
        paths: paths.clone(),
        reload_armed,
        events: Rc::clone(&events),
        route_calls: 0,
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

    run_daemon_with(
        Some(&config),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        Some(1),
    )
    .expect("daemon run");

    assert_eq!(commands.route_calls, 2);
    assert_eq!(
        events.borrow().as_slice(),
        ["deadline-observed", "slow-inventory", "slow-inventory"]
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn reload_detection_precedes_initial_theme_reconciliation() {
    let (root, roots, paths, config) = integration_tree();
    fs::write(
        &config,
        "[display]\npoll_interval = 0.25\n[panel]\norder = [\"main\"]\n[panel.main]\nitems = [\"mem_usage\"]\n[tooltip]\norder = []\n[pages]\norder = []\n",
    )
    .expect("initial theme ordering config");
    let replacement = String::from(
        "[display]\npoll_interval = 0.25\n[panel]\norder = [\"main\"]\n[panel.main]\nitems = [\"mem_usage:bar\"]\n[tooltip]\norder = []\n[pages]\norder = []\n",
    );
    let now = Rc::new(Cell::new(Duration::ZERO));
    let armed = Rc::new(Cell::new(false));
    let mut control = ThemeOrderingControl {
        now,
        paths: paths.clone(),
        config,
        replacement,
        kdeglobals: paths.kdeglobals.clone(),
        armed: Rc::clone(&armed),
    };
    let mut commands = ThemeOrderingCommand {
        paths: paths.clone(),
        armed: Rc::clone(&armed),
        calls: 0,
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

    let config_path = control.config.clone();
    run_daemon_with(
        Some(&config_path),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        Some(1),
    )
    .expect("daemon run");

    assert!(armed.get());
    assert_eq!(commands.calls, 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn hardware_only_panel_uses_local_inventory_and_real_startup_sample() {
    let (root, roots, paths, config) = integration_tree();
    let hwmon = roots.sys_root.join("class/hwmon/hwmon0");
    fs::create_dir_all(&hwmon).expect("hwmon fixture");
    fs::write(hwmon.join("name"), "nct6775\n").expect("hwmon name");
    fs::write(hwmon.join("fan1_input"), "1234\n").expect("fan reading");
    fs::write(
        &config,
        "[display]\npoll_interval = 0.25\n[panel]\norder = [\"thermal\"]\n[panel.thermal]\nitems = [\"fan_speed\"]\n[tooltip]\norder = []\n[pages]\norder = []\n[sensors]\nfan1_speed = \"nct6775|fan1_input\"\n",
    )
    .expect("hardware-only config");
    let now = Rc::new(Cell::new(Duration::ZERO));
    let mut control = shared_control(now, &paths);
    let frames = Rc::clone(&control.frames);
    let mut commands = NvidiaCommand { smi_calls: 0 };
    let mut dbus = AbsentDbus;
    let mut notifications = RecordingNotifications;
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        nvml: None,
        bolt: None,
    };

    run_daemon_with(
        Some(&config),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        Some(1),
    )
    .expect("daemon run");

    assert!(
        frames
            .borrow()
            .iter()
            .any(|(_, panel)| panel.contains("1234")),
        "local fan job did not execute before publication"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn blocking_startup_boundary_observes_first_panel_and_retained_tooltip_by_two_hundred_ms() {
    let (root, roots, paths, config) = integration_tree();
    let now = Rc::new(Cell::new(Duration::ZERO));
    let observed = Rc::new(Cell::new(false));
    let mut commands = NvidiaCommand { smi_calls: 0 };
    let mut dbus = FirstPaintDbus {
        now: Rc::clone(&now),
        paths: paths.clone(),
        observed: Rc::clone(&observed),
    };
    let mut notifications = RecordingNotifications;
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        nvml: None,
        bolt: None,
    };
    let mut control = shared_control(now, &paths);

    run_daemon_with(
        Some(&config),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        Some(1),
    )
    .expect("daemon run");

    assert!(observed.get());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn late_attempt_is_committed_only_after_its_overdue_publication() {
    let (root, roots, paths, config) = integration_tree();
    configure_nvidia(&config);
    add_nvidia_device(&roots);
    let now = Rc::new(Cell::new(Duration::ZERO));
    let mut control = shared_control(Rc::clone(&now), &paths);
    let frames = Rc::clone(&control.frames);
    let mut commands = NvidiaCommand { smi_calls: 0 };
    let mut dbus = AbsentDbus;
    let mut notifications = RecordingNotifications;
    let mut nvml = ScriptedNvml {
        now,
        result: Ok(nvml_metrics(77)),
        first_delay: Duration::from_millis(300),
        calls: 0,
    };
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        nvml: Some(&mut nvml),
        bolt: None,
    };

    run_daemon_with(
        Some(&config),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        Some(2),
    )
    .expect("daemon run");

    let frames = frames.borrow();
    assert!(
        frames
            .iter()
            .any(|(at, html)| *at <= Duration::from_millis(200) && html.contains("class=\"panel")),
        "the real slow panel blocker prevented the 200 ms timeout paint"
    );
    assert!(
        frames
            .iter()
            .filter(|(at, _)| *at <= Duration::from_millis(300))
            .all(|(_, html)| !html.contains("77")),
        "the overdue frame observed a post-deadline sample"
    );
    assert!(
        frames
            .iter()
            .any(|(at, html)| *at >= Duration::from_millis(500) && html.contains("77")),
        "the committed sample was not visible at the following deadline"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn repeated_nvml_failures_within_fallback_freshness_run_smi_once() {
    let (root, roots, paths, config) = integration_tree();
    configure_nvidia(&config);
    add_nvidia_device(&roots);
    let now = Rc::new(Cell::new(Duration::ZERO));
    let mut control = shared_control(Rc::clone(&now), &paths);
    let mut commands = NvidiaCommand { smi_calls: 0 };
    let mut dbus = AbsentDbus;
    let mut notifications = RecordingNotifications;
    let mut nvml = ScriptedNvml {
        now,
        result: Err(NvmlError::Read),
        first_delay: Duration::ZERO,
        calls: 0,
    };
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        nvml: Some(&mut nvml),
        bolt: None,
    };

    run_daemon_with(
        Some(&config),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        Some(4),
    )
    .expect("daemon run");

    assert!(
        nvml.calls > 1,
        "fixture did not produce repeated NVML failures"
    );
    assert_eq!(commands.smi_calls, 1);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn poll_limit_one_waits_for_one_post_start_display_deadline() {
    let (root, roots, paths, config) = integration_tree();
    let now = Rc::new(Cell::new(Duration::ZERO));
    let mut control = shared_control(Rc::clone(&now), &paths);
    let mut commands = NvidiaCommand { smi_calls: 0 };
    let mut dbus = AbsentDbus;
    let mut notifications = RecordingNotifications;
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        nvml: None,
        bolt: None,
    };

    run_daemon_with(
        Some(&config),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        Some(1),
    )
    .expect("daemon run");

    assert!(now.get() >= Duration::from_millis(250));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stop_observed_after_overdue_attempt_prevents_follow_up_chain() {
    let (root, roots, paths, config) = integration_tree();
    configure_nvidia(&config);
    add_nvidia_device(&roots);
    let now = Rc::new(Cell::new(Duration::ZERO));
    let mut control = shared_control(Rc::clone(&now), &paths);
    control.stop_at = Some(Duration::from_millis(300));
    let mut commands = NvidiaCommand { smi_calls: 0 };
    let mut dbus = AbsentDbus;
    let mut notifications = RecordingNotifications;
    let mut nvml = ScriptedNvml {
        now,
        result: Ok(nvml_metrics(77)),
        first_delay: Duration::from_millis(300),
        calls: 0,
    };
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        nvml: Some(&mut nvml),
        bolt: None,
    };

    run_daemon_with(
        Some(&config),
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        None,
    )
    .expect("daemon run");

    assert_eq!(nvml.calls, 1);
    let _ = fs::remove_dir_all(root);
}
