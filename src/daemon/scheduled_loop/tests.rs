#![allow(clippy::expect_used)]

use std::time::UNIX_EPOCH;

use super::*;

#[test]
fn unchanged_external_paths_preserve_stamps_and_emit_same_wake_file_jobs() {
    let root = std::env::temp_dir().join(format!("plasma-top-reload-stamp-{}", std::process::id()));
    let updates = root.join("updates");
    let server = root.join("server");
    fs::create_dir_all(&root).expect("external fixture root");
    fs::write(&updates, "7\n").expect("same-wake external mutation");
    fs::write(&server, "1\n").expect("same-wake server mutation");
    let updates = updates.to_string_lossy().into_owned();
    let server = server.to_string_lossy().into_owned();
    let mut cfg = Config::default();
    cfg.system_updates.file.clone_from(&updates);
    cfg.server_check.file.clone_from(&server);
    let mut updates_stamp = Some(UNIX_EPOCH);
    let mut server_stamp = Some(UNIX_EPOCH);

    reset_external_stamp_if_path_changed(&updates, &updates, &mut updates_stamp);
    reset_external_stamp_if_path_changed(&server, &server, &mut server_stamp);
    let jobs = changed_external_file_jobs(&cfg, &mut updates_stamp, &mut server_stamp);

    assert_eq!(
        jobs,
        [Some(JobKind::UpdatesFile), Some(JobKind::ServerFile)]
    );
    assert_ne!(updates_stamp, Some(UNIX_EPOCH));
    assert_ne!(server_stamp, Some(UNIX_EPOCH));
    let _ = fs::remove_dir_all(root);
}
