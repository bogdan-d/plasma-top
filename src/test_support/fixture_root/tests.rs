use super::*;

#[test]
fn default_points_at_shared_fixtures_not_host_root() {
    let root = FixtureRoot::default();

    // Invariant: the default root is the shared fixture tree under the
    // repo, never the host `/`. This is the load-bearing assertion that
    // prevents fixture tests from accidentally reading real `/proc`/`/sys`.
    assert_eq!(root.root, PathBuf::from("tests/fixtures"));
    assert_ne!(root.root, PathBuf::from("/"));
    assert_eq!(
        root.join("proc/stat"),
        PathBuf::from("tests/fixtures/proc/stat"),
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
