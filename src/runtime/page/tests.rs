use super::*;
use std::io;

#[test]
fn next_delta_is_plus_one() {
    assert_eq!(PageDirection::Next.delta(), 1);
}

#[test]
fn prev_delta_is_minus_one() {
    assert_eq!(PageDirection::Prev.delta(), -1);
}

#[test]
fn read_usize_default_returns_default_on_missing_file() {
    let path = Path::new("/nonexistent/plasma-top-test-page-missing");
    assert_eq!(read_usize_default(path, 7), 7);
}

#[test]
fn read_usize_default_returns_default_on_garbage() -> io::Result<()> {
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("plasma-top-test-page-garbage-{pid}"));
    std::fs::write(&path, b"not-a-number")?;
    assert_eq!(read_usize_default(&path, 9), 9);
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[test]
fn read_usize_default_parses_trimmed_value() -> io::Result<()> {
    let pid = std::process::id();
    let path = std::env::temp_dir().join(format!("plasma-top-test-page-trim-{pid}"));
    std::fs::write(&path, b"  42  \n")?;
    assert_eq!(read_usize_default(&path, 0), 42);
    let _ = std::fs::remove_file(path);
    Ok(())
}
