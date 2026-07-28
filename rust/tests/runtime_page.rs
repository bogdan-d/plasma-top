//! Integration tests for the page counter semantics ported from
//! `src/pagestate.py`: defaults, wrapping, and the flock-serialized
//! read-modify-write that prevents lost updates under concurrent wheel
//! processes.
//!
//! These tests redirect the runtime directory to a unique tmp subtree via
//! `XDG_RUNTIME_DIR` so they never touch a live runtime directory. The env
//! mutation is gated by `ENV_GUARD`, which serializes every mutation plus
//! the path reads that depend on it; worker threads are spawned only while
//! the env is stable, so they can read it safely.

use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use plasma_top::runtime::atomic::write_atomic;
use plasma_top::runtime::page::{PageDirection, npages, read_page, set_page, step_page};
use plasma_top::runtime::{ensure_dirs, npages_file, page_file};

static ENV_GUARD: Mutex<()> = Mutex::new(());

/// Per-test fixture: a unique runtime subtree under `/tmp`, exported via
/// `XDG_RUNTIME_DIR` for the duration of the test, then cleaned up.
struct RuntimeFixture {
    /// Held until `Drop` so env mutation stays serialized within this binary.
    _guard: MutexGuard<'static, ()>,
    dir: PathBuf,
}

impl RuntimeFixture {
    fn new(label: &str) -> io::Result<Self> {
        let guard = ENV_GUARD
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir =
            std::env::temp_dir().join(format!("plasma-top-rust-test-page-{label}-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir)?;
        // SAFETY: `ENV_GUARD` is held. Worker threads are only spawned after
        // this constructor returns (when the env is stable) and join before
        // `Drop` runs (which mutates env again). No concurrent env access.
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir) };
        ensure_dirs()?;
        Ok(Self { _guard: guard, dir })
    }

    fn state_dir(&self) -> PathBuf {
        self.dir.join("plasma-top").join("state")
    }
}

impl Drop for RuntimeFixture {
    fn drop(&mut self) {
        // SAFETY: `ENV_GUARD` still held via `_guard`; no worker threads are
        // alive because `Drop` only runs after the test body returns.
        unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Writes `bytes` to `path` via the production atomic primitive, used by tests
/// to seed state files exactly as the daemon would.
fn seed(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic(path, bytes)
}

#[test]
fn read_page_defaults_to_zero_when_file_absent() -> io::Result<()> {
    let _fix = RuntimeFixture::new("missing-page")?;
    let _ = std::fs::remove_file(page_file());
    assert_eq!(read_page(), 0);
    Ok(())
}

#[test]
fn read_page_defaults_to_zero_on_garbage() -> io::Result<()> {
    let _fix = RuntimeFixture::new("garbage-page")?;
    seed(&page_file(), b"not-a-number")?;
    assert_eq!(read_page(), 0);
    Ok(())
}

#[test]
fn npages_defaults_to_one_when_file_absent() -> io::Result<()> {
    let _fix = RuntimeFixture::new("missing-npages")?;
    let _ = std::fs::remove_file(npages_file());
    assert_eq!(npages(), 1);
    Ok(())
}

#[test]
fn npages_defaults_to_one_on_garbage() -> io::Result<()> {
    let _fix = RuntimeFixture::new("garbage-npages")?;
    seed(&npages_file(), b"garbage")?;
    assert_eq!(npages(), 1);
    Ok(())
}

#[test]
fn set_page_round_trips_through_read_page() -> io::Result<()> {
    let _fix = RuntimeFixture::new("roundtrip")?;
    set_page(7)?;
    assert_eq!(read_page(), 7);
    set_page(0)?;
    assert_eq!(read_page(), 0);
    Ok(())
}

#[test]
fn step_page_is_noop_when_npages_le_one() -> io::Result<()> {
    let _fix = RuntimeFixture::new("npages-one")?;
    seed(&npages_file(), b"1")?;
    seed(&page_file(), b"3")?;

    let code = step_page(PageDirection::Next)?;

    assert_eq!(code, 0);
    assert_eq!(read_page(), 3, "counter must not advance when npages <= 1");
    Ok(())
}

#[test]
fn step_page_is_noop_when_npages_file_absent() -> io::Result<()> {
    let _fix = RuntimeFixture::new("npages-absent")?;
    let _ = std::fs::remove_file(npages_file());
    seed(&page_file(), b"3")?;

    let code = step_page(PageDirection::Next)?;

    assert_eq!(code, 0);
    assert_eq!(read_page(), 3);
    Ok(())
}

#[test]
fn step_page_wraps_next_and_prev() -> io::Result<()> {
    let _fix = RuntimeFixture::new("wrap")?;
    seed(&npages_file(), b"3")?;
    seed(&page_file(), b"0")?;

    step_page(PageDirection::Next)?;
    assert_eq!(read_page(), 1);
    step_page(PageDirection::Next)?;
    assert_eq!(read_page(), 2);
    step_page(PageDirection::Next)?;
    assert_eq!(read_page(), 0, "next must wrap at npages");

    step_page(PageDirection::Prev)?;
    assert_eq!(read_page(), 2, "prev must wrap to last page");
    Ok(())
}

/// Reproduces the lost-update bug that flock prevents: many concurrent wheel
/// processes advancing the counter must observe every step. Without the lock
/// the read-modify-write races and the final counter falls short of `THREADS`.
///
/// Per `conc-scoped-threads` we use [`std::thread::scope`] so the workers can
/// borrow the runtime fixture by reference.
#[test]
fn step_page_never_drops_a_notch_under_concurrency() -> io::Result<()> {
    const THREADS: usize = 32;
    const NPAGES: usize = 5;

    let fix = RuntimeFixture::new("concurrency")?;
    seed(&npages_file(), NPAGES.to_string().as_bytes())?;
    seed(&page_file(), b"0")?;

    std::thread::scope(|s| {
        for _ in 0..THREADS {
            s.spawn(|| {
                if let Err(error) = step_page(PageDirection::Next) {
                    panic!("step_page failed under concurrency: {error}");
                }
            });
        }
    });

    let expected = THREADS % NPAGES;
    let observed = read_page();
    assert_eq!(
        observed, expected,
        "after {THREADS} serialized next-steps against npages={NPAGES}, \
         counter must equal {expected} (no lost updates); got {observed}"
    );
    // Keep the fixture alive until after the scope joins.
    drop(fix);
    Ok(())
}

#[test]
fn step_page_serializes_concurrent_next_and_prev() -> io::Result<()> {
    // Equal numbers of next and prev steps must return the counter to its
    // starting value — only true if every step is applied exactly once.
    const PAIRS: usize = 16;
    const NPAGES: usize = 7;

    let fix = RuntimeFixture::new("mixed")?;
    seed(&npages_file(), NPAGES.to_string().as_bytes())?;
    seed(&page_file(), b"3")?;
    let baseline = read_page();

    std::thread::scope(|s| {
        for round in 0..PAIRS {
            s.spawn(|| {
                if let Err(error) = step_page(PageDirection::Next) {
                    panic!("step_page(next) failed: {error}");
                }
            });
            // Bind `round` so the borrow checker is satisfied; we don't read it.
            let _ = round;
            s.spawn(|| {
                if let Err(error) = step_page(PageDirection::Prev) {
                    panic!("step_page(prev) failed: {error}");
                }
            });
        }
    });

    assert_eq!(
        read_page(),
        baseline,
        "balanced next/prev steps must cancel out when serialized"
    );
    drop(fix);
    Ok(())
}

#[test]
fn set_page_returns_err_when_state_dir_is_readonly() -> io::Result<()> {
    let fix = RuntimeFixture::new("readonly")?;
    // Pre-create a page file we can later verify survived intact.
    seed(&page_file(), b"original")?;

    let state = fix.state_dir();
    let mut perms = std::fs::metadata(&state)?.permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&state, perms)?;

    let result = set_page(99);

    // Restore write permission so the fixture's `Drop` can remove the dir.
    // `set_mode(0o755)` is rwxr-xr-x — owner-writable without making the dir
    // world-writable (which `Permissions::set_readonly(false)` would do).
    let mut perms = std::fs::metadata(&state)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&state, perms)?;

    assert!(result.is_err(), "expected error, got {result:?}");
    assert_eq!(
        std::fs::read_to_string(page_file())?,
        "original",
        "page counter must be untouched when the write failed"
    );
    Ok(())
}
