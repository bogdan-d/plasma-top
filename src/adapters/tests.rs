#![allow(clippy::expect_used)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::domain::boundary::{CommandRunner, CommandStatus};

use super::*;

fn shell(
    runner: &mut ProductionCommandRunner,
    script: &str,
    timeout: Duration,
) -> crate::domain::boundary::CommandOutput {
    runner
        .run(
            Path::new("/bin/sh"),
            &[OsString::from("-c"), OsString::from(script)],
            timeout,
        )
        .expect("shell command")
}

#[test]
fn command_timeout_kills_without_waiting_for_natural_exit() {
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut runner = io.commands();
    let started = Instant::now();
    let error = runner
        .run(
            Path::new("/bin/sh"),
            &[OsString::from("-c"), OsString::from("sleep 5")],
            Duration::from_millis(30),
        )
        .expect_err("sleep must time out");
    assert!(error.to_string().contains("timed out after 0.030s"));
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[test]
fn timeout_covers_descendant_held_pipes_after_direct_child_exits() {
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut runner = io.commands();
    let started = Instant::now();
    let error = runner
        .run(
            Path::new("/bin/sh"),
            &[OsString::from("-c"), OsString::from("sleep 2 &")],
            Duration::from_millis(30),
        )
        .expect_err("descendant-held pipe must time out");
    assert!(error.to_string().contains("timed out after 0.030s"));
    assert!(started.elapsed() < Duration::from_millis(500));
}

#[test]
fn command_status_maps_exit_and_signal() {
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut runner = io.commands();
    assert_eq!(
        shell(&mut runner, "exit 23", Duration::from_secs(1)).status,
        CommandStatus::Exit(23)
    );
    assert_eq!(
        shell(&mut runner, "kill -TERM $$", Duration::from_secs(1)).status,
        CommandStatus::Signal(15)
    );
}

#[test]
fn stdout_and_stderr_are_drained_concurrently_without_pipe_deadlock() {
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut runner = io.commands();
    let output = shell(
        &mut runner,
        "head -c 400000 /dev/zero & head -c 400000 /dev/zero >&2 & wait",
        Duration::from_secs(2),
    );
    assert_eq!(output.status, CommandStatus::Exit(0));
    assert_eq!(output.stdout.len(), 400_000);
    assert_eq!(output.stderr.len(), 400_000);
    assert!(!output.truncation.is_truncated());
}

#[test]
fn combined_output_is_bounded_with_deterministic_stdout_first_allocation() {
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut runner = io.commands();
    let output = shell(
        &mut runner,
        "head -c 700000 /dev/zero & head -c 800000 /dev/zero >&2 & wait",
        Duration::from_secs(2),
    );
    assert_eq!(output.stdout.len(), 700_000);
    assert_eq!(output.stderr.len(), 1024 * 1024 - 700_000);
    assert_eq!(output.stdout.len() + output.stderr.len(), 1024 * 1024);
    assert_eq!(output.truncation.stdout_bytes, 0);
    assert_eq!(
        output.truncation.stderr_bytes,
        800_000 - (1024 * 1024 - 700_000) as u64
    );
}

#[test]
fn timeout_kills_process_group_descendant() {
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut runner = io.commands();
    let pid_file = unique_path("descendant-pid");
    let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
    let _ = runner.run(
        Path::new("/bin/sh"),
        &[OsString::from("-c"), OsString::from(script)],
        Duration::from_millis(100),
    );
    let pid = std::fs::read_to_string(&pid_file)
        .expect("descendant pid")
        .trim()
        .parse::<u32>()
        .expect("numeric pid");
    let deadline = Instant::now() + Duration::from_secs(1);
    while process_exists(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!process_exists(pid), "descendant {pid} survived timeout");
    let _ = std::fs::remove_file(pid_file);
}

#[test]
fn shutdown_kills_active_group_replies_and_rejects_late_requests() {
    let mut io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut running = io.commands();
    let thread = std::thread::spawn(move || {
        running.run(
            Path::new("/bin/sh"),
            &[OsString::from("-c"), OsString::from("sleep 30")],
            Duration::from_secs(60),
        )
    });
    std::thread::sleep(Duration::from_millis(30));
    let started = Instant::now();
    io.shutdown();
    assert!(started.elapsed() <= Duration::from_millis(500));
    assert!(thread.join().expect("request thread").is_err());
    let mut late = io.commands();
    let error = late
        .run(Path::new("/bin/true"), &[], Duration::from_secs(1))
        .expect_err("late request must fail");
    assert!(error.to_string().contains("shut down"));
}

fn unique_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("plasma-top-{label}-{}", std::process::id()))
}

fn process_exists(pid: u32) -> bool {
    std::fs::metadata(format!("/proc/{pid}")).is_ok()
}
