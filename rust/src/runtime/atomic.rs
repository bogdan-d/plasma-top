//! Atomic file publication: write to a PID-unique tmp sibling, then rename.
//!
//! Ported from `src/daemon.py::_write_atomic` and `src/pagestate.py::set_page`.
//! `std::fs::rename` is atomic on the same filesystem (POSIX rename(2)
//! guarantee), so a reader sees either the previous file or the new one,
//! never a torn write. The tmp name embeds the PID so overlapping writers —
//! concurrent `pirostats page next` processes fired by a fast scroll — never
//! clobber a shared tmp mid-rename.

use std::io;
use std::path::Path;

/// Writes `contents` to `target` atomically.
///
/// The tmp file is a PID-qualified sibling of `target` so it shares the
/// filesystem (required for `rename` to be atomic) and never collides with
/// another writer's tmp. On success, the tmp is renamed onto `target` and
/// nothing is left behind. On any error after tmp creation, the tmp is
/// removed best-effort and the original error is returned.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] from `File::create`, `write_all`,
/// `flush`, or `rename`.
pub fn write_atomic(target: &Path, contents: &[u8]) -> io::Result<()> {
    let pid = std::process::id();
    let tmp = target.with_extension(format!("{pid}.tmp"));

    if let Err(error) = write_tmp(&tmp, contents) {
        // Best-effort cleanup; the original error wins. `remove_file` errors
        // (e.g. tmp was never created) are swallowed on purpose.
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    std::fs::rename(&tmp, target)
}

/// Writes `contents` to a freshly-truncated `tmp`, flushing before return so
/// the bytes are in the kernel page cache before the rename.
fn write_tmp(tmp: &Path, contents: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::File::create(tmp)?;
    file.write_all(contents)?;
    file.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io;
    use std::path::PathBuf;

    fn unique_target(label: &str) -> PathBuf {
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("pirostats-runtime-atomic-{label}-{pid}-{nanos}"))
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
}
