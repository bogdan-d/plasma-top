#![allow(clippy::expect_used)]
use super::*;
use crate::domain::boundary::{
    BoundaryError, CommandOutput, DbusOutput, DbusRequest, NotificationError, NotificationPayload,
};
use crate::domain::boundary::{DbusFacade, NotificationFacade};
use crate::domain::state::NotificationState;
use crate::error::CriticalService;
use crate::notify::check_and_notify;
use crate::sensors::process::ProcessState;
use crate::sensors::{
    CollectCtx, OwnerRefs, collect_with_notifications, cpu, disk, external, gpu_history, gpu_intel,
    gpu_nvidia, memory, network, power,
};
use std::cell::Cell;
use std::thread;

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

fn integration_tree() -> (PathBuf, FilesystemRoots) {
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
    let roots = FilesystemRoots {
        cache_root: Some(root.join("cache")),
        config_root: Some(root.join("config")),
        proc_root,
        sys_root,
    };
    (root, roots)
}

#[test]
fn failed_carried_temperature_and_load_do_not_complete_sustained_alerts() {
    let (root, roots) = integration_tree();
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
    let mut amd_npu_owner = crate::sensors::npu_amd::AmdNpuState::default();
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
            amd_npu: &mut amd_npu_owner,
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
            amd_npu: &mut amd_npu_owner,
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
