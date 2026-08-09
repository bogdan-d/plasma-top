#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

#[test]
fn compute_code_root_uses_override_when_set() {
    let resolved = compute_code_root(Some("/usr/lib/plasma-top"));

    assert_eq!(resolved, PathBuf::from("/usr/lib/plasma-top"));
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

    assert_eq!(resolved, PathBuf::from(env!("CARGO_MANIFEST_DIR")));
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

    assert_eq!(resolved, PathBuf::from("/custom/xdg/plasma-top"));
}

#[test]
fn compute_xdg_dir_falls_back_to_home_config() {
    let resolved = compute_xdg_dir(None, Some("/home/test"));

    assert_eq!(resolved, PathBuf::from("/home/test/.config/plasma-top"));
}

#[test]
fn compute_xdg_dir_treats_empty_xdg_as_unset() {
    let resolved = compute_xdg_dir(Some(""), Some("/home/test"));

    assert_eq!(resolved, PathBuf::from("/home/test/.config/plasma-top"));
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
