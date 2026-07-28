//! Filesystem root for fixture-based tests.
//!
//! [`FixtureRoot`] is the virtual `/` for every test that needs to read procfs,
//! sysfs, or runtime-equivalent files. Tests build a [`FixtureRoot`] from a
//! fixture directory under `rust/tests/fixtures/` and pass it to the production
//! filesystem readers (the future `sensors/source.rs`) so no test ever touches
//! the host `/proc` or `/sys`.

use std::path::{Path, PathBuf};

/// Resolved root of a fixture tree, typically under `rust/tests/fixtures/`.
///
/// Acts as a virtual `/` for fixture-based tests: every host boundary (`proc/`,
/// `sys/`, `run/`) is mapped under this root so no test ever touches the real
/// filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixtureRoot {
    /// Filesystem path that the test harness treats as the boundary root.
    pub root: PathBuf,
}

impl FixtureRoot {
    /// Creates a fixture root pointing at `root`.
    ///
    /// Prefer [`FixtureRoot::from_env`] when a test wants the canonical
    /// `rust/tests/fixtures/` tree; use [`FixtureRoot::new`] when the test
    /// owns a private tempdir.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Resolves the fixture root from the build environment.
    ///
    /// The root is `<CARGO_MANIFEST_DIR>/tests/fixtures`, where
    /// `CARGO_MANIFEST_DIR` is the crate's manifest directory (`rust/`) baked
    /// into the binary at compile time. This makes the root robust against the
    /// test's runtime working directory: `cargo test` can be invoked from the
    /// repo root or the crate dir and the resolved path is the same.
    ///
    /// Tests that walk the bundled fixture tree should always use this
    /// constructor; tests that own a tempdir should use [`FixtureRoot::new`].
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            root: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests")
                .join("fixtures"),
        }
    }

    /// Returns a child path under the fixture root without touching the host.
    #[must_use]
    pub fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }

    /// Returns the `proc/` subtree of the fixture root (Linux procfs analog).
    #[must_use]
    pub fn proc(&self) -> PathBuf {
        self.root.join("proc")
    }

    /// Returns the `sys/` subtree of the fixture root (Linux sysfs analog).
    #[must_use]
    pub fn sys(&self) -> PathBuf {
        self.root.join("sys")
    }

    /// Returns the `run/` subtree (the runtime-equivalent fixture directory).
    ///
    /// Production code reads its runtime files from `$XDG_RUNTIME_DIR/plasma-top`;
    /// tests stage the same layout under `<fixture_root>/run/plasma-top` and
    /// point the runtime lane at this path.
    #[must_use]
    pub fn run(&self) -> PathBuf {
        self.root.join("run")
    }
}

impl Default for FixtureRoot {
    fn default() -> Self {
        Self {
            root: PathBuf::from("rust/tests/fixtures"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_points_at_shared_fixtures_not_host_root() {
        let root = FixtureRoot::default();

        // Invariant: the default root is the shared fixture tree under the
        // repo, never the host `/`. This is the load-bearing assertion that
        // prevents fixture tests from accidentally reading real `/proc`/`/sys`.
        assert_eq!(root.root, PathBuf::from("rust/tests/fixtures"));
        assert_ne!(root.root, PathBuf::from("/"));
        assert_eq!(
            root.join("proc/stat"),
            PathBuf::from("rust/tests/fixtures/proc/stat"),
        );
    }

    #[test]
    fn boundary_subtrees_are_direct_children_of_root() {
        let root = FixtureRoot::new(PathBuf::from("/tmp/example"));

        assert_eq!(root.proc(), PathBuf::from("/tmp/example/proc"));
        assert_eq!(root.sys(), PathBuf::from("/tmp/example/sys"));
        assert_eq!(root.run(), PathBuf::from("/tmp/example/run"));
    }

    #[test]
    fn join_preserves_arbitrary_relative_paths() {
        let root = FixtureRoot::new(PathBuf::from("/tmp/example"));

        assert_eq!(
            root.join("sys/class/hwmon/hwmon0/name"),
            PathBuf::from("/tmp/example/sys/class/hwmon/hwmon0/name"),
        );
    }

    #[test]
    fn from_env_resolves_under_manifest_dir() {
        let root = FixtureRoot::from_env();

        // The root must be an absolute path so it resolves regardless of the
        // test's runtime working directory (cargo test, IDE runner, etc.).
        assert!(
            root.root.is_absolute(),
            "from_env must produce an absolute path, got {}",
            root.root.display(),
        );
        let expected = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures");
        assert_eq!(root.root, expected);
    }

    #[test]
    fn from_env_points_at_existing_fixture_tree() {
        let root = FixtureRoot::from_env();

        // Guards against accidental fixture-tree relocation: if the directory
        // moves, this test fails before downstream lanes observe the breakage.
        assert!(
            root.root.is_dir(),
            "fixture tree must exist at {}",
            root.root.display(),
        );
    }
}
