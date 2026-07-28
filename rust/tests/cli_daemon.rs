//! Process-level CLI and isolated runtime integration checks.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_plasma-top"))
}

fn temp_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("plasma-top-cli-{label}-{}", std::process::id()))
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
        "usage: plasma-top [-h] <command> ...\nplasma-top: error: argument <command>: invalid choice: 'unknown' (choose from 'daemon', 'render', 'probe', 'profiling', 'list-items', 'page', 'click')\n"
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
