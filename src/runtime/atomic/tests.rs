use super::*;
use std::io;
use std::path::PathBuf;

fn unique_target(label: &str) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("plasma-top-runtime-atomic-{label}-{pid}-{nanos}"))
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
    let pid = std::process::id();
    let expected_tmp = target.with_extension(format!("{pid}.tmp"));

    write_atomic(&target, b"payload")?;
    assert!(
        !expected_tmp.exists(),
        "tmp left behind at {expected_tmp:?}"
    );

    let _ = std::fs::remove_file(target);
    Ok(())
}
