#![allow(clippy::expect_used)]

use super::*;

fn fixture(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root =
        std::env::temp_dir().join(format!("plasma-top-config-ui-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create fixture directory");
    (root.join("config.toml"), root)
}

#[test]
fn init_copies_once_and_preserves_existing_file() {
    let (path, root) = fixture("init");
    let shipped = root.join("shipped.toml");
    fs::write(&shipped, b"[display]\npoll_interval = 1.5\n").expect("write shipped config");
    init(&path, &shipped).expect("initialize");
    assert_eq!(
        fs::read(&path).expect("read initialized config"),
        fs::read(&shipped).expect("read shipped config")
    );
    fs::write(&path, b"custom = true\n").expect("change user config");
    init(&path, &shipped).expect("existing config is preserved");
    assert_eq!(
        fs::read(&path).expect("read user config"),
        b"custom = true\n"
    );
    fs::remove_dir_all(root).expect("remove fixture directory");
}

#[test]
fn apply_preserves_comments_and_unrelated_settings() {
    use std::os::unix::fs::PermissionsExt;

    let (path, root) = fixture("apply");
    fs::write(
        &path,
        "# keep this comment\n[display]\npoll_interval = 1.5 # keep this interval note\nhistory_interval = 1.5\nlanguage = \"en\"\n\n[pages]\norder = [\"processes\", \"graphs\", \"future_page\"]\ngraph_history_length = 60\n\n[notifications]\ndisk_usage = true\n\n[sensors]\nfan1_speed = \"chip|fan1_input\"\n",
    )
    .expect("write user config");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("set private mode");
    apply(&path, "2.5", "3", "10100", "01000000000").expect("save settings");
    let updated = fs::read_to_string(&path).expect("read updated config");
    assert!(updated.contains("# keep this comment"));
    assert!(updated.contains("# keep this interval note"));
    assert!(updated.contains("fan1_speed = \"chip|fan1_input\""));
    assert!(updated.contains("graph_history_length = 60"));
    assert!(updated.contains("order = [\"graphs\", \"future_page\", \"cpu_cores\"]"));
    assert_eq!(
        show(&path).expect("show settings"),
        "2.5\t3\t10100\t01000000000\n"
    );
    assert_eq!(
        fs::metadata(&path).expect("read mode").permissions().mode() & 0o777,
        0o600
    );
    let loaded = crate::config::load_config(Some(&path), Some(false)).expect("daemon loads edit");
    assert_eq!(loaded.display.poll_interval.as_secs_f64(), 2.5);
    assert_eq!(loaded.pages.order, ["graphs", "future_page", "cpu_cores"]);
    assert!(loaded.notifications.disk_smart);
    assert!(!loaded.notifications.disk_usage);
    fs::remove_dir_all(root).expect("remove fixture directory");
}

#[test]
fn invalid_values_leave_file_unchanged() {
    let (path, root) = fixture("invalid");
    fs::write(&path, b"[display]\npoll_interval = 1.5\n").expect("write config");
    let original = fs::read(&path).expect("read original");
    assert!(apply(&path, "0.01", "1.5", "10000", "11111111111").is_err());
    assert!(apply(&path, "1.5", "1.5", "10x00", "11111111111").is_err());
    assert_eq!(fs::read(&path).expect("read config"), original);
    fs::remove_dir_all(root).expect("remove fixture directory");
}
