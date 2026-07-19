//! Runtime file locations, shared by the daemon, the `page` command, and the
//! applet.
//!
//! Ported from `src/runtime.py`. Everything the daemon publishes for the
//! widget lives under one per-user runtime directory on tmpfs. The layout is
//! load-bearing: the applet drives its reads off an inotify watch on the HTML
//! directory (`FolderListModel`), so that directory must churn only when there
//! is something new to show:
//!
//! ```text
//! <runtime>/            <- watched: the two HTML files, nothing else
//!     panel.html
//!     tooltip.html
//!     state/            <- not watched: page counter, geometry, lock
//!         geom
//!         page
//!         npages
//!         page.lock
//! ```
//!
//! State files change on every wheel notch and every panel resize; keeping
//! them in a sibling means those writes never wake the watcher.
//!
//! Paths are exposed as functions (not constants) so the runtime directory is
//! resolved lazily on each call. That matters in tests, which override
//! `XDG_RUNTIME_DIR` per case, and in any future session that re-evaluates
//! the environment.

use std::path::PathBuf;

use nix::unistd::getuid;

pub mod atomic;
pub mod page;

/// Returns the per-user runtime root.
///
/// `$XDG_RUNTIME_DIR/pirostats` when the env var is set and non-empty;
/// otherwise `/tmp/pirostats-{uid}` so bare `probe` / `render` invocations
/// outside a systemd/PAM session still resolve a writable root. The applet
/// resolves the same directory independently via Qt's `RuntimeLocation`
/// (which *is* `XDG_RUNTIME_DIR` on Linux), so the two sides agree without
/// sharing a constant.
#[must_use]
pub fn runtime_dir() -> PathBuf {
    match std::env::var("XDG_RUNTIME_DIR") {
        Ok(xdg) if !xdg.is_empty() => PathBuf::from(xdg).join("pirostats"),
        _ => PathBuf::from(format!("/tmp/pirostats-{}", getuid())),
    }
}

/// Returns the runtime `state/` subtree holding the page counter, geometry,
/// and the flock that serializes concurrent wheel processes.
#[must_use]
pub fn state_dir() -> PathBuf {
    runtime_dir().join("state")
}

/// Returns the watched panel HTML path read by the applet.
#[must_use]
pub fn panel_file() -> PathBuf {
    runtime_dir().join("panel.html")
}

/// Returns the watched tooltip HTML path read by the applet.
#[must_use]
pub fn tooltip_file() -> PathBuf {
    runtime_dir().join("tooltip.html")
}

/// Returns the applet-published geometry path under `state/`:
/// `<usable_px> <glyph_adv_px> <vertical 0|1> <tooltip_adv_px>`.
#[must_use]
pub fn geom_file() -> PathBuf {
    state_dir().join("geom")
}

/// Returns the tooltip page counter path under `state/`.
#[must_use]
pub fn page_file() -> PathBuf {
    state_dir().join("page")
}

/// Returns the published page-count path under `state/`.
#[must_use]
pub fn npages_file() -> PathBuf {
    state_dir().join("npages")
}

/// Returns the flock path under `state/` that serializes concurrent wheel
/// processes (see [`page::step_page`]).
#[must_use]
pub fn lock_file() -> PathBuf {
    state_dir().join("page.lock")
}

/// Creates the runtime tree (root + `state/`).
///
/// Idempotent: cheap enough (two stats on tmpfs when it exists) to call from
/// any writer rather than relying on startup order. The applet can publish a
/// geometry, or a wheel notch bump the counter, before the daemon has ever
/// run.
///
/// # Errors
///
/// Returns the underlying [`std::io::Error`] if directory creation fails
/// (permission denied, read-only filesystem, …).
pub fn ensure_dirs() -> std::io::Result<()> {
    std::fs::create_dir_all(state_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_dir_lives_under_runtime_dir() {
        let state = state_dir();
        let runtime = runtime_dir();

        assert_eq!(state, runtime.join("state"));
    }

    #[test]
    fn known_files_live_at_documented_paths() {
        let runtime = runtime_dir();
        let state = runtime.join("state");

        assert_eq!(panel_file(), runtime.join("panel.html"));
        assert_eq!(tooltip_file(), runtime.join("tooltip.html"));
        assert_eq!(geom_file(), state.join("geom"));
        assert_eq!(page_file(), state.join("page"));
        assert_eq!(npages_file(), state.join("npages"));
        assert_eq!(lock_file(), state.join("page.lock"));
    }
}
