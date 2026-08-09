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
