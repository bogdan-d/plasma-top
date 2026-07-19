//! Tooltip page counter, shared by the daemon and the `page` wheel command.
//!
//! Ported from `src/pagestate.py`. The counter is a single integer in
//! [`page_file`]; the wrap bound is read from [`npages_file`]. Concurrent
//! wheel processes serialize on [`lock_file`] via flock(2) so the
//! read-modify-write never drops a notch — the "wheel skips pages / goes
//! dead" bug that the Python docstring warns about.
//!
//! `PageDirection` is defined locally rather than re-exported from `cli.rs`
//! to keep `runtime` free of a CLI-layer dependency: `cli` is a composition
//! root owned by the integration owner (see `plans/LANES.md`), and the
//! `DAEMON-CLI` lane wires `cli::PageDirection` -> [`PageDirection`] in Wave 5.

use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

use nix::fcntl::{Flock, FlockArg};

use crate::runtime::atomic::write_atomic;
use crate::runtime::{ensure_dirs, lock_file, npages_file, page_file};

/// Direction of a single page step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageDirection {
    /// Advance to the next page, wrapping against the page count.
    Next,
    /// Step to the previous page, wrapping against the page count.
    Prev,
}

impl PageDirection {
    /// Returns the signed delta applied to the counter (`+1` for next, `-1`
    /// for prev).
    #[must_use]
    pub const fn delta(self) -> i64 {
        match self {
            Self::Next => 1,
            Self::Prev => -1,
        }
    }
}

/// Reads the raw page counter, defaulting to `0` on a missing or unparseable
/// file. Matches `pagestate.read_page` exactly: parse the trimmed text as
/// `usize`; any IO or parse error yields `0`.
#[must_use]
pub fn read_page() -> usize {
    read_usize_default(&page_file(), 0)
}

/// Writes the counter atomically via a PID-unique tmp sibling + rename.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] if directory creation or the atomic
/// write fails.
pub fn set_page(n: usize) -> io::Result<()> {
    ensure_dirs()?;
    write_atomic(&page_file(), n.to_string().as_bytes())
}

/// Reads the published page count, defaulting to `1` when the file is absent
/// or unparseable. Matches `pagestate._npages`.
#[must_use]
pub fn npages() -> usize {
    read_usize_default(&npages_file(), 1)
}

/// Advances the counter by one notch in `direction`, wrapping against the
/// published page count. A no-op (`Ok(0)`) when no deep-dive pages are
/// configured (`npages <= 1`).
///
/// The whole read-modify-write is serialized across concurrent processes by
/// flock(2) on [`lock_file`], so a rapid scroll never drops a step. Within one
/// process the lock is held on a distinct open file description per call; that
/// still serializes correctly because flock locks are associated with the open
/// file description, not the process.
///
/// Returns a process exit code (always `0`, matching Python) on success.
///
/// # Errors
///
/// Returns the underlying [`io::Error`] if directory creation, the flock, or
/// the atomic write fails. A poisoned lock from a panic'ed prior holder is
/// treated like an absent lock and recovered from.
pub fn step_page(direction: PageDirection) -> io::Result<i32> {
    let n = npages();
    if n <= 1 {
        return Ok(0);
    }
    ensure_dirs()?;
    // Hold LOCK_EX for the read-modify-write. Dropping `Flock` releases the
    // lock and closes the underlying file descriptor automatically. On error
    // `Flock::lock` returns the original `File` so it can be inspected or
    // dropped intentionally; we drop it and surface only the errno.
    let lock = open_lock()?;
    let _guard = Flock::lock(lock, FlockArg::LockExclusive)
        .map_err(|(_file, errno)| io::Error::from_raw_os_error(errno as i32))?;
    let next = ((read_page() as i64) + direction.delta()).rem_euclid(n as i64);
    set_page(next as usize)?;
    Ok(0)
}

/// Opens the lock file writeable, creating it if absent. Does not truncate:
/// the file holds no contents, only the lock state.
fn open_lock() -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(lock_file())
}

/// Parses the trimmed file contents as `usize`, returning `default` on any
/// IO or parse error. Matches the Python `try/except (OSError, ValueError)`
/// fallback shape.
fn read_usize_default(path: &Path, default: usize) -> usize {
    let Ok(text) = std::fs::read_to_string(path) else {
        return default;
    };
    text.trim().parse::<usize>().unwrap_or(default)
}

#[cfg(test)]
mod tests {
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
        let path = Path::new("/nonexistent/pirostats-test-page-missing");
        assert_eq!(read_usize_default(path, 7), 7);
    }

    #[test]
    fn read_usize_default_returns_default_on_garbage() -> io::Result<()> {
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("pirostats-test-page-garbage-{pid}"));
        std::fs::write(&path, b"not-a-number")?;
        assert_eq!(read_usize_default(&path, 9), 9);
        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn read_usize_default_parses_trimmed_value() -> io::Result<()> {
        let pid = std::process::id();
        let path = std::env::temp_dir().join(format!("pirostats-test-page-trim-{pid}"));
        std::fs::write(&path, b"  42  \n")?;
        assert_eq!(read_usize_default(&path, 0), 42);
        let _ = std::fs::remove_file(path);
        Ok(())
    }
}
