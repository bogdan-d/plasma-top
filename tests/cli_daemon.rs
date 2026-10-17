//! Process-level CLI checks. Every real daemon process runs on a private session bus.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_plasma-top"))
}

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("plasma-top-cli-{label}-{}", std::process::id()))
}

const BUS_NAME: &str = "com.github.bogdan_d.PlasmaTop";

/// Private `dbus-daemon` that never touches the live session.
struct PrivateBus {
    address: String,
    daemon: Child,
}

impl PrivateBus {
    fn start() -> Self {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon must be installed for daemon process tests");
        let mut address = String::new();
        BufReader::new(daemon.stdout.take().expect("bus stdout"))
            .read_line(&mut address)
            .expect("bus address");
        Self {
            address: address.trim().to_owned(),
            daemon,
        }
    }

    /// Runs `plasma-top` with this bus as its session bus and `home` as an isolated home.
    fn command(&self, home: &Path) -> Command {
        let mut command = Command::new(binary());
        command
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("DBUS_SESSION_BUS_ADDRESS", &self.address)
            .env("HOME", home);
        command
    }

    fn name_owner(&self) -> Option<String> {
        let output = Command::new("dbus-send")
            .arg(format!("--bus={}", self.address))
            .args([
                "--print-reply=literal",
                "--dest=org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus.GetNameOwner",
            ])
            .arg(format!("string:{BUS_NAME}"))
            .output()
            .expect("query name owner");
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn panel_html(&self) -> String {
        let output = Command::new("dbus-send")
            .arg(format!("--bus={}", self.address))
            .args([
                "--print-reply=literal",
                &format!("--dest={BUS_NAME}"),
                "/com/github/bogdan_d/PlasmaTop",
                "org.freedesktop.DBus.Properties.Get",
                "string:com.github.bogdan_d.PlasmaTop1",
                "string:PanelHtml",
            ])
            .output()
            .expect("query panel property");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}

/// Starts a daemon on `bus` and waits until it serves a panel.
fn start_daemon(bus: &PrivateBus, home: &Path) -> Child {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut child = bus
        .command(home)
        .args(["daemon", "--config"])
        .arg(manifest.join("config/config.toml"))
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn daemon");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if bus.panel_html().contains("class=\"panel") {
            return child;
        }
        if child.try_wait().expect("query daemon startup").is_some() || Instant::now() >= deadline {
            let _ = child.kill();
            let output = child.wait_with_output().expect("collect failed startup");
            panic!(
                "daemon did not publish within startup bound: status={:?}, stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            );
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().expect("query daemon status").is_some() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn no_command_prints_help_and_list_items_has_stable_order() {
    let help = Command::new(binary()).output().expect("spawn help");
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).expect("utf8 help");
    assert!(help.contains("daemon      Production loop"));

    let items = Command::new(binary())
        .arg("list-items")
        .output()
        .expect("spawn list-items");
    assert!(items.status.success());
    let items = String::from_utf8(items.stdout).expect("utf8 items");
    assert!(items.starts_with("Available items (metric[:form] → where it can go):\n\n"));
    assert!(
        items.find("battery_kbd").expect("first item") < items.find("uptime").expect("last item")
    );
}

#[test]
fn invalid_command_is_stderr_and_failure() {
    let output = Command::new(binary())
        .arg("unknown")
        .output()
        .expect("spawn invalid command");
    assert!(!output.status.success());
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).expect("utf8 error"),
        "usage: plasma-top [-h] <command> ...\nplasma-top: error: argument <command>: invalid choice: 'unknown' (choose from 'daemon', 'render', 'probe', 'profiling', 'list-items', 'config', 'click')\n"
    );
}

#[test]
fn sigterm_stops_daemon_with_zero_exit_and_releases_its_name() {
    let root = temp_root("sigterm");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("home fixture");
    let bus = PrivateBus::start();
    let mut child = start_daemon(&bus, &root);
    assert!(bus.name_owner().is_some());

    let pid = i32::try_from(child.id()).expect("daemon pid fits pid_t");
    kill(Pid::from_raw(pid), Signal::SIGTERM).expect("send SIGTERM");
    let exited_in_time = wait_for_exit(&mut child, Duration::from_secs(2));
    if !exited_in_time {
        let _ = child.kill();
    }
    let output = child.wait_with_output().expect("collect daemon exit");
    let _ = fs::remove_dir_all(&root);

    assert!(exited_in_time, "daemon exceeded SIGTERM shutdown bound");
    assert!(
        output.status.success(),
        "SIGTERM exit was {:?}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(bus.name_owner(), None);
}

#[test]
fn second_daemon_fails_clearly_and_diagnostics_run_beside_the_first() {
    let root = temp_root("second-daemon");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("home fixture");
    let bus = PrivateBus::start();
    let mut first = start_daemon(&bus, &root);
    let owner = bus.name_owner();

    let started = Instant::now();
    let second = bus
        .command(&root)
        .args(["daemon", "--config", "config/config.toml"])
        .output()
        .expect("spawn second daemon");
    let render = bus
        .command(&root)
        .args(["render", "--config", "config/config.toml"])
        .output()
        .expect("spawn render");
    let probe = bus
        .command(&root)
        .args(["probe", "--config", "config/config.toml"])
        .output()
        .expect("spawn probe");
    let after = bus.name_owner();
    let _ = first.kill();
    let _ = first.wait();
    let _ = fs::remove_dir_all(&root);

    assert!(!second.status.success());
    assert!(started.elapsed() < Duration::from_secs(10));
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(
        stderr.contains("another PlasmaTop daemon owns com.github.bogdan_d.PlasmaTop"),
        "{stderr}"
    );
    assert!(render.status.success(), "{:?}", render.stderr);
    assert!(probe.status.success(), "{:?}", probe.stderr);
    assert!(owner.is_some());
    assert_eq!(after, owner, "nothing may replace the serving daemon");
}

#[test]
fn timed_profiling_runs_async_scheduler_without_owning_the_bus_name() {
    let root = temp_root("profiling");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("home fixture");

    let bus = PrivateBus::start();
    let output = bus
        .command(&root)
        .args([
            "profiling",
            "--config",
            "config/config.toml",
            "--duration",
            "0.05",
            "--scenario",
            "hidden",
        ])
        .output()
        .expect("spawn timed profiling");

    assert!(
        output.status.success(),
        "timed profiling failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).expect("utf8 profiling report");
    assert!(
        report.starts_with("profile:\n"),
        "unexpected report prefix: {report}"
    );
    for field in [
        "  duration_seconds: 0.050\n",
        "  scenario: hidden\n",
        "jobs:\n",
        "timing:\n",
        "  maximum_sample_age:",
        "  wake_lateness:",
        "  first_paint_latency:",
        "  publication_lateness:",
        "  skipped_display_deadlines:",
        "  stimuli: disabled",
        "  write_time: not_applicable",
        "  shutdown_duration:",
        "  shutdown_over_500ms: 0",
        "  final_status: ok",
    ] {
        assert!(
            report.contains(field),
            "missing report field {field:?}: {report}"
        );
    }
    assert!(
        !report.contains("[boot]"),
        "profiling leaked boot logs: {report}"
    );
    assert!(!report.contains("event_control_latency:"));
    assert!(!report.contains("render_tooltip_activated:"));
    assert!(!report.contains("render_page_changed:"));
    assert_eq!(bus.name_owner(), None, "profiling never owns the name");
    fs::remove_dir_all(root).expect("cleanup fixture");
}

#[test]
fn timed_profiling_stimuli_report_publication_latency_without_mutating_config() {
    let root = temp_root("profiling-stimuli");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("profiling fixture");
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let config = root.join("config.toml");
    let original = fs::read(manifest.join("config/config.toml")).expect("shipped config");
    fs::write(&config, &original).expect("isolated input config");

    let bus = PrivateBus::start();
    let output = bus
        .command(&root)
        .args([
            "profiling",
            "--config",
            config.to_str().expect("utf8 fixture path"),
            "--duration",
            "2.5",
            "--scenario",
            "main",
            "--stimuli",
        ])
        .output()
        .expect("spawn profiling stimuli");

    assert!(
        output.status.success(),
        "timed profiling failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8(output.stdout).expect("utf8 profiling report");
    for field in [
        "  stimuli: enabled",
        "  aggregate_scope: scenario plus profiling stimuli",
        "event_page_latency: p50=",
        "event_control_latency: p50=",
        "event_config_latency: p50=",
        "stimulus_actions: planned=",
        "stimulus_requested_state:",
        "stimulus_observed_state:",
        "shutdown_duration:",
        "final_status: ok",
    ] {
        assert!(report.contains(field), "missing {field:?}: {report}");
    }
    assert_eq!(fs::read(&config).expect("input config remains"), original);
    assert_eq!(bus.name_owner(), None, "profiling never owns the name");
    fs::remove_dir_all(root).expect("cleanup fixture");
}

#[test]
fn timed_profiling_rejects_unconfigured_page() {
    let output = Command::new(binary())
        .args([
            "profiling",
            "--config",
            "config/config.toml",
            "--duration",
            "0.01",
            "--scenario",
            "not_a_page",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("spawn invalid profiling scenario");

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("profiling scenario page 'not_a_page' is not configured in pages.order")
    );
}

#[test]
fn timed_profiling_rejects_full_alias_before_startup() {
    let output = Command::new(binary())
        .args(["profiling", "--duration", "0.01", "--scenario", "full"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("spawn full profiling alias");

    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported alias: 'full'"));
}
