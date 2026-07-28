//! Asset root resolution: the shipped tree and the user's writable XDG dir.
//!
//! Mirrors the Python `CODE_ROOT` / `XDG_DIR` constants from `src/config.py`,
//! adapted for the Rust binary model. Python resolves paths relative to
//! `__file__`; a compiled Rust binary has no `__file__`, so the shipped tree
//! is found via `CARGO_MANIFEST_DIR/..` baked in at compile time, plus a
//! `PIROSTATS_CODE_ROOT` env override for packaged installs where the binary
//! and the read-only assets live under different `/usr/...` prefixes (e.g.
//! `/usr/bin/pirostats` reading from `/usr/lib/pirostats`).
//!
//! Pure cores ([`compute_code_root`], [`compute_xdg_dir`]) take their inputs
//! explicitly so tests exercise the choice without touching the host
//! environment.

use std::env;
use std::path::{Path, PathBuf};

/// Compile-time path of the crate manifest dir (`rust/`) joined with `..`,
/// yielding the repo root in dev. Overridden at runtime by
/// `PIROSTATS_CODE_ROOT` for packaged installs.
const COMPILE_TIME_CODE_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

/// Env var name that overrides the shipped-tree root for packaged installs.
///
/// Packaging and the checkout launcher set this same name before config loads.
pub const PIROSTATS_CODE_ROOT_ENV: &str = "PIROSTATS_CODE_ROOT";

/// Returns the path to the shipped asset tree.
///
/// Dev: `<repo>` (the parent of `rust/`). Packaged: whatever the installer
/// wrote into the `PIROSTATS_CODE_ROOT` env (e.g. `/usr/lib/pirostats`).
#[must_use]
pub fn code_root() -> PathBuf {
    compute_code_root(env::var(PIROSTATS_CODE_ROOT_ENV).ok().as_deref())
}

/// Pure core of [`code_root`] for tests.
///
/// `custom` mirrors the value [`env::var`] would have returned for
/// [`PIROSTATS_CODE_ROOT_ENV`]: `None` when unset, `Some("")` when set to
/// the empty string (treated as unset, matching Python's truthiness on
/// `os.environ.get`).
#[must_use]
pub fn compute_code_root(custom: Option<&str>) -> PathBuf {
    match custom.filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from(COMPILE_TIME_CODE_ROOT),
    }
}

/// Returns the user's home directory.
///
/// Reads `$HOME` directly; falls back to `/` when unset. Python's
/// `Path.home()` consults the passwd database as a fallback, but every
/// realistic runtime (`systemd --user`, an interactive shell, the applet's
/// process) sets `HOME`, and the fall-back path is the same. Documented as
/// a deviation: no `getpwuid` round-trip.
#[must_use]
pub fn home_dir() -> PathBuf {
    compute_home_dir(env::var("HOME").ok().as_deref())
}

/// Pure core of [`home_dir`] for tests.
#[must_use]
pub fn compute_home_dir(home_env: Option<&str>) -> PathBuf {
    match home_env.filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => PathBuf::from("/"),
    }
}

/// Returns the user's writable PiroStats config directory.
///
/// `$XDG_CONFIG_HOME/pirostats` when the env is set and non-empty;
/// otherwise `~/.config/pirostats`. A packaged install ships read-only
/// defaults under [`code_root`]; the user drops overrides here (the conky
/// model — see the user-facing `config.toml` header).
#[must_use]
pub fn xdg_dir() -> PathBuf {
    compute_xdg_dir(
        env::var("XDG_CONFIG_HOME").ok().as_deref(),
        env::var("HOME").ok().as_deref(),
    )
}

/// Pure core of [`xdg_dir`] for tests.
#[must_use]
pub fn compute_xdg_dir(xdg_config_home: Option<&str>, home_env: Option<&str>) -> PathBuf {
    let base = match xdg_config_home.filter(|value| !value.is_empty()) {
        Some(value) => PathBuf::from(value),
        None => compute_home_dir(home_env).join(".config"),
    };
    base.join("pirostats")
}

/// Returns the shipped default `config.toml`.
#[must_use]
pub fn shipped_config() -> PathBuf {
    code_root().join("config").join("config.toml")
}

/// Returns the shipped `machines.toml` (a how-to template in the repo).
#[must_use]
pub fn shipped_machines() -> PathBuf {
    code_root().join("config").join("machines.toml")
}

/// Returns the path to a language TOML under the shipped `lang/` tree.
#[must_use]
pub fn shipped_language(language: &str) -> PathBuf {
    code_root().join("lang").join(format!("{language}.toml"))
}

/// Returns the parent directory of `path`, or `.` when `path` has no parent.
///
/// Mirrors Python's `pathlib.Path.parent`, which returns `PosixPath('.')`
/// for a bare filename. Used by [`super::merge::machines_path_for`] to
/// locate the `machines.toml` sibling of an explicit `--config`.
#[must_use]
pub fn parent_or_dot(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn compute_code_root_uses_override_when_set() {
        let resolved = compute_code_root(Some("/usr/lib/pirostats"));

        assert_eq!(resolved, PathBuf::from("/usr/lib/pirostats"));
    }

    #[test]
    fn compute_code_root_treats_empty_override_as_unset() {
        let resolved = compute_code_root(Some(""));

        // Falls back to the compile-time repo root; just verify it's not "".
        assert!(!resolved.as_os_str().is_empty());
    }

    #[test]
    fn compute_code_root_uses_compile_time_default_when_unset() {
        let resolved = compute_code_root(None);

        assert!(
            resolved.ends_with(".."),
            "default code root is CARGO_MANIFEST_DIR/.., got {}",
            resolved.display(),
        );
    }

    #[test]
    fn compute_home_dir_uses_home_env() {
        assert_eq!(
            compute_home_dir(Some("/home/test")),
            PathBuf::from("/home/test"),
        );
    }

    #[test]
    fn compute_home_dir_falls_back_to_root_when_unset() {
        assert_eq!(compute_home_dir(None), PathBuf::from("/"));
        assert_eq!(compute_home_dir(Some("")), PathBuf::from("/"));
    }

    #[test]
    fn compute_xdg_dir_honors_xdg_config_home() {
        let resolved = compute_xdg_dir(Some("/custom/xdg"), Some("/home/test"));

        assert_eq!(resolved, PathBuf::from("/custom/xdg/pirostats"));
    }

    #[test]
    fn compute_xdg_dir_falls_back_to_home_config() {
        let resolved = compute_xdg_dir(None, Some("/home/test"));

        assert_eq!(resolved, PathBuf::from("/home/test/.config/pirostats"));
    }

    #[test]
    fn compute_xdg_dir_treats_empty_xdg_as_unset() {
        let resolved = compute_xdg_dir(Some(""), Some("/home/test"));

        assert_eq!(resolved, PathBuf::from("/home/test/.config/pirostats"));
    }

    #[test]
    fn shipped_assets_live_under_code_root() {
        // Verify the compile-time code root actually contains the shipped
        // files — guards against a broken dev checkout.
        let root = code_root();
        let config = root.join("config").join("config.toml");

        assert!(
            config.is_file(),
            "shipped config.toml must exist under code_root ({})",
            config.display(),
        );
    }

    #[test]
    fn parent_or_dot_matches_python_pathlib_parent() {
        assert_eq!(parent_or_dot(Path::new("/a/b/c.toml")), Path::new("/a/b"));
        assert_eq!(parent_or_dot(Path::new("c.toml")), Path::new("."));
        assert_eq!(parent_or_dot(Path::new("./c.toml")), Path::new("."));
    }
}
