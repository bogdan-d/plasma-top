//! Integration tests for runtime path resolution parity with `src/runtime.py`.
//!
//! This binary is its own cargo integration-test process so the env mutations
//! here cannot race with tests in other `runtime_*.rs` binaries. Within this
//! binary the `ENV_GUARD` mutex serializes every env mutation plus the
//! path-resolution read that follows it.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use plasma_top::runtime::{
    geom_file, lock_file, npages_file, page_file, panel_file, runtime_dir, state_dir, tooltip_file,
};

static ENV_GUARD: Mutex<()> = Mutex::new(());

/// Acquires the binary-wide env mutex for the lifetime of the value.
fn lock_env() -> MutexGuard<'static, ()> {
    ENV_GUARD
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

#[test]
fn runtime_dir_honors_xdg_runtime_dir_when_set() {
    let _guard = lock_env();
    let custom = PathBuf::from("/tmp/plasma-top-rust-test-xdg-set");

    // SAFETY: `ENV_GUARD` is held; no other thread in this binary can read or
    // mutate `XDG_RUNTIME_DIR` until the guard drops. No worker threads are
    // spawned in this test.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &custom) };
    let resolved = runtime_dir();
    // SAFETY: same mutex contract as above; restoring env after the read.
    unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };

    assert_eq!(resolved, custom.join("plasma-top"));
}

#[test]
fn runtime_dir_treats_empty_xdg_as_unset() {
    let _guard = lock_env();

    // SAFETY: `ENV_GUARD` held; no concurrent env access in this binary.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", "") };
    let resolved = runtime_dir();
    // SAFETY: same mutex contract.
    unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };

    let s = resolved
        .to_str()
        .unwrap_or_else(|| panic!("non-utf8 path {resolved:?}"));
    assert!(
        s.starts_with("/tmp/plasma-top-"),
        "empty XDG should fall back, got {s}"
    );
}

#[test]
fn runtime_dir_falls_back_to_tmp_uid_when_xdg_unset() {
    let _guard = lock_env();

    // SAFETY: `ENV_GUARD` held; serialized env access within this binary.
    unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
    let resolved = runtime_dir();

    let s = resolved
        .to_str()
        .unwrap_or_else(|| panic!("non-utf8 path {resolved:?}"));
    assert!(
        s.starts_with("/tmp/plasma-top-"),
        "expected /tmp/plasma-top-<uid> fallback, got {s}"
    );
    // Sanity-check the uid suffix parses as a number.
    let suffix = &s["/tmp/plasma-top-".len()..];
    assert!(
        suffix.parse::<u32>().is_ok(),
        "fallback suffix should be a numeric uid, got {suffix}"
    );
}

#[test]
fn all_path_accessors_match_documented_layout() {
    let _guard = lock_env();
    let custom = PathBuf::from("/tmp/plasma-top-rust-test-layout");

    // SAFETY: `ENV_GUARD` held; isolated env mutation within this binary.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &custom) };
    // Resolve every accessor while the env is set — each call re-reads the
    // env (paths are functions, not constants), so we must capture them now.
    let runtime = runtime_dir();
    let state = state_dir();
    let panel = panel_file();
    let tooltip = tooltip_file();
    let geom = geom_file();
    let page = page_file();
    let npages = npages_file();
    let lock = lock_file();
    // SAFETY: same mutex contract; restoring after all reads.
    unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };

    assert_eq!(runtime, custom.join("plasma-top"));
    assert_eq!(state, runtime.join("state"));

    assert_eq!(panel, runtime.join("panel.html"));
    assert_eq!(tooltip, runtime.join("tooltip.html"));
    assert_eq!(geom, state.join("geom"));
    assert_eq!(page, state.join("page"));
    assert_eq!(npages, state.join("npages"));
    assert_eq!(lock, state.join("page.lock"));
}
