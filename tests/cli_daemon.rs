//! Process-level CLI and isolated runtime integration checks.

use std::fs;
use std::path::PathBuf;
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
    assert!(help.contains("page        Switch the tooltip page"));

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
        "usage: plasma-top [-h] <command> ...\nplasma-top: error: argument <command>: invalid choice: 'unknown' (choose from 'daemon', 'render', 'probe', 'profiling', 'list-items', 'page', 'click', 'present', 'dismiss')\n"
    );
}

#[test]
fn sigterm_stops_daemon_with_zero_exit() {
    let root = temp_root("sigterm");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("runtime fixture");
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(binary())
        .args(["daemon", "--config"])
        .arg(manifest.join("config/config.toml"))
        .current_dir(&manifest)
        .env("XDG_RUNTIME_DIR", &root)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn daemon");

    let panel = root.join("plasma-top/panel.html");
    let startup_deadline = Instant::now() + Duration::from_secs(5);
    let ready = loop {
        if panel.is_file() {
            break true;
        }
        if child.try_wait().expect("query daemon startup").is_some()
            || Instant::now() >= startup_deadline
        {
            break false;
        }
        thread::sleep(Duration::from_millis(10));
    };
    if !ready {
        let _ = child.kill();
        let output = child.wait_with_output().expect("collect failed startup");
        let _ = fs::remove_dir_all(&root);
        panic!(
            "daemon did not publish within startup bound: status={:?}, stderr={}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }

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
}

#[test]
fn page_command_only_touches_isolated_state_subtree() {
    let root = temp_root("page");
    let state = root.join("plasma-top/state");
    fs::create_dir_all(&state).expect("state fixture");
    fs::write(state.join("npages"), "3").expect("npages fixture");
    fs::write(state.join("page"), "0").expect("page fixture");

    let output = Command::new(binary())
        .args(["page", "next"])
        .env("XDG_RUNTIME_DIR", &root)
        .output()
        .expect("spawn page");

    assert!(output.status.success(), "{:?}", output.stderr);
    assert_eq!(
        fs::read_to_string(state.join("page")).expect("read page"),
        "1"
    );
    let root_entries = fs::read_dir(root.join("plasma-top"))
        .expect("runtime root")
        .map(|entry| entry.expect("runtime entry").file_name())
        .collect::<Vec<_>>();
    assert_eq!(root_entries, vec!["state"]);
    fs::remove_dir_all(root).expect("cleanup fixture");
}
