//! Test support skeleton shared by integration tests and downstream lanes.
//!
//! This module is gated behind the `test-support` cargo feature and is excluded
//! from production builds. The SCAFFOLD lane owns only this skeleton: it fixes
//! the module surface that downstream lanes extend so parallel work does not
//! collide on ad-hoc helpers.
//!
//! Lane ownership for the concrete implementations:
//!
//! | Submodule / type       | Owning lane  | Fills in                                   |
//! |------------------------|--------------|--------------------------------------------|
//! | [`FixtureRoot`]        | `FIXTURES`   | fixture filesystem root + deserializers    |
//! | [`FakeClock`]          | `FIXTURES`   | deterministic clock for histories/rates    |
//! | [`FakeCommandRunner`]  | `FIXTURES`   | argv-keyed command replies + call trace    |
//! | [`FakeDbus`]           | `FIXTURES`   | UPower/UDisks2/notify decoded fixtures     |
//! | [`FixtureLoader`]      | `FIXTURES`   | reads `rust/tests/fixtures/**` into types  |
//!
//! Until `FIXTURES` lands, these types compile as empty markers so the crate
//! continues to pass `--all-features` checks without committing to field shapes
//! that would have to be refactored later.

use std::path::PathBuf;

use crate::domain::boundary::{ClockSnapshot, CommandOutput, DbusOutput};

/// Resolved root of a fixture tree (typically under `rust/tests/fixtures/`).
///
/// `FIXTURES` populates this with deserialization helpers and per-boundary
/// subtree resolution (`proc/`, `sys/`, `run/`, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureRoot {
    /// Filesystem path that the test harness treats as the boundary root.
    pub root: PathBuf,
}

impl FixtureRoot {
    /// Creates a fixture root pointing at `root`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Returns a child path under the fixture root without touching the host.
    #[must_use]
    pub fn join(&self, relative: impl AsRef<std::path::Path>) -> PathBuf {
        self.root.join(relative)
    }
}

impl Default for FixtureRoot {
    fn default() -> Self {
        Self {
            root: PathBuf::from("rust/tests/fixtures"),
        }
    }
}

/// Controllable clock used by rate/history/state-machine tests.
///
/// `FIXTURES` extends this with advance/peek helpers and wires it into the
/// shared sensor/daemon boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FakeClock {
    /// Last snapshot handed to the code under test.
    pub now: ClockSnapshot,
}

impl FakeClock {
    /// Creates a clock pinned at `now`.
    #[must_use]
    pub const fn at(now: ClockSnapshot) -> Self {
        Self { now }
    }
}

/// Placeholder command-runner used by adapter tests.
///
/// `FIXTURES` replaces this with argv-keyed replies and an ordered call trace
/// so differential tests can assert exact request shapes and result handling.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FakeCommandRunner {
    /// Recorded outputs in the order they will be returned.
    pub outputs: Vec<CommandOutput>,
}

/// Placeholder D-Bus facade used by power/notify tests.
///
/// `FIXTURES` replaces this with decoded reply fixtures keyed by service +
/// method + object path so the bus never needs to be live during tests.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FakeDbus {
    /// Recorded outputs in the order they will be returned.
    pub outputs: Vec<DbusOutput>,
}

/// Loader for fixture manifests shared between Python oracle and Rust.
///
/// `FIXTURES` fills in deserialization into the frozen domain types and the
/// parity runner glue. The skeleton only fixes the path the loader reads from.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FixtureLoader {
    /// Root directory for fixture files.
    pub root: FixtureRoot,
}

impl FixtureLoader {
    /// Creates a loader rooted at `root`.
    #[must_use]
    pub fn new(root: FixtureRoot) -> Self {
        Self { root }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_root_default_points_at_shared_fixtures() {
        let root = FixtureRoot::default();

        assert_eq!(root.root, PathBuf::from("rust/tests/fixtures"));
        assert_eq!(
            root.join("proc/stat"),
            PathBuf::from("rust/tests/fixtures/proc/stat"),
        );
    }

    #[test]
    fn fake_clock_defaults_to_zeroed_snapshot() {
        let clock = FakeClock::default();

        assert_eq!(clock.now.monotonic, std::time::Duration::ZERO);
    }

    #[test]
    fn fixture_loader_carries_root() {
        let root = FixtureRoot::new(PathBuf::from("/tmp/example"));
        let loader = FixtureLoader::new(root);

        assert_eq!(loader.root.root, PathBuf::from("/tmp/example"));
    }
}
