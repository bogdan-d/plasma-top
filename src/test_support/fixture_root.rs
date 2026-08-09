//! Filesystem root for fixture-based tests.
//!
//! [`FixtureRoot`] is the virtual `/` for every test that needs to read procfs,
//! sysfs, or runtime-equivalent files. Tests build a [`FixtureRoot`] from a
//! fixture directory under `tests/fixtures/` and pass it to the production
//! filesystem readers (the future `sensors/source.rs`) so no test ever touches
//! the host `/proc` or `/sys`.

use std::path::{Path, PathBuf};

/// Resolved root of a fixture tree, typically under `tests/fixtures/`.
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
    /// `tests/fixtures/` tree; use [`FixtureRoot::new`] when the test
    /// owns a private tempdir.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Resolves the fixture root from the build environment.
    ///
    /// The root is `<CARGO_MANIFEST_DIR>/tests/fixtures`, where
    /// `CARGO_MANIFEST_DIR` is the repository root baked into the binary at
    /// compile time. This makes the root robust against the
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
            root: PathBuf::from("tests/fixtures"),
        }
    }
}

#[cfg(test)]
mod tests;
