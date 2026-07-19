//! Integration tests for the atomic publication primitive
//! ([`pirostats::runtime::atomic::write_atomic`]).
//!
//! These tests write to PID-and-nanos-unique targets under `std::env::temp_dir()`
//! and never touch the live runtime directory, so they need no env mutation.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use pirostats::runtime::atomic::write_atomic;

/// Returns a path that no other test in this binary or another process can
/// collide with: PID + nanosecond timestamp + label.
fn unique_target(label: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("pirostats-runtime-atomic-{label}-{pid}-{nanos}"))
}

/// Returns the PID-unique tmp path `write_atomic` would create for `target`.
fn expected_tmp(target: &Path) -> PathBuf {
    let pid = std::process::id();
    target.with_extension(format!("{pid}.tmp"))
}

#[test]
fn write_atomic_publishes_new_contents() -> io::Result<()> {
    let target = unique_target("publish");

    write_atomic(&target, b"hello")?;

    let read = std::fs::read(&target)?;
    assert_eq!(read, b"hello");
    let _ = std::fs::remove_file(target);
    Ok(())
}

#[test]
fn write_atomic_replaces_existing_target() -> io::Result<()> {
    let target = unique_target("replace");
    std::fs::write(&target, b"old")?;

    write_atomic(&target, b"new")?;

    let read = std::fs::read(&target)?;
    assert_eq!(read, b"new");
    let _ = std::fs::remove_file(target);
    Ok(())
}

#[test]
fn write_atomic_leaves_no_tmp_on_success() -> io::Result<()> {
    let target = unique_target("clean-success");

    write_atomic(&target, b"payload")?;

    assert!(
        !expected_tmp(&target).exists(),
        "tmp left behind at {:?}",
        expected_tmp(&target)
    );
    let _ = std::fs::remove_file(target);
    Ok(())
}

#[test]
fn write_atomic_preserves_target_and_cleans_tmp_on_failure() -> io::Result<()> {
    let dir = unique_target("readonly-dir");
    std::fs::create_dir_all(&dir)?;
    let target = dir.join("target");
    std::fs::write(&target, b"original")?;

    // Strip write permission from the parent so tmp creation fails with EACCES.
    let mut perms = std::fs::metadata(&dir)?.permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&dir, perms)?;

    let result = write_atomic(&target, b"new");

    // Restore write permission so the cleanup below can remove the directory.
    // `set_mode(0o755)` is rwxr-xr-x — owner-writable without exposing the
    // dir to other users (which `Permissions::set_readonly(false)` would do).
    let mut perms = std::fs::metadata(&dir)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&dir, perms)?;

    assert!(result.is_err(), "expected error, got {result:?}");
    assert_eq!(
        std::fs::read(&target)?,
        b"original",
        "target must be untouched"
    );
    assert!(
        !expected_tmp(&target).exists(),
        "tmp should be cleaned on failure"
    );

    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn write_atomic_does_not_create_target_when_tmp_write_fails() -> io::Result<()> {
    let dir = unique_target("missing-parent");
    // Intentionally do NOT create `dir`; `target.with_extension(...)` lives
    // inside it, so `File::create` fails with ENOENT.
    let target = dir.join("target");

    let result = write_atomic(&target, b"payload");

    assert!(result.is_err(), "expected error, got {result:?}");
    assert!(
        !target.exists(),
        "target should not appear when tmp creation fails"
    );
    assert!(
        !expected_tmp(&target).exists(),
        "no tmp should remain when File::create failed"
    );
    Ok(())
}
