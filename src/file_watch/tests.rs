#![allow(clippy::expect_used)]

use super::*;
use std::time::Duration;

fn fixture(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("plasma-top-watch-{name}-{}", std::process::id()))
}

#[tokio::test]
async fn observes_atomic_rename_and_debounces_logical_source() {
    let root = fixture("rename");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fixture");
    let target = root.join("config.toml");
    let mut watcher =
        FileWatcher::new(vec![WatchTarget::new(WatchSource::Config, &target)]).expect("watch");
    std::fs::write(root.join("config.tmp"), "one").expect("write temp");
    std::fs::rename(root.join("config.tmp"), &target).expect("rename target");
    std::fs::write(&target, "two").expect("write target");

    let changed = tokio::time::timeout(Duration::from_secs(1), watcher.changed())
        .await
        .expect("watch deadline")
        .expect("watch event");

    assert_eq!(changed, BTreeSet::from([WatchSource::Config]));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn follows_recreated_parent_directory() {
    let root = fixture("recreate");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fixture");
    let parent = root.join("nested");
    let target = parent.join("status");
    let mut watcher = FileWatcher::new(vec![WatchTarget::new(WatchSource::Server, &target)])
        .expect("watch ancestor");
    std::fs::create_dir(&parent).expect("create watched parent");
    let first = tokio::time::timeout(Duration::from_secs(1), watcher.changed())
        .await
        .expect("first deadline")
        .expect("first event");
    assert!(first.contains(&WatchSource::Server));
    std::fs::write(&target, "ok").expect("write nested target");
    let second = tokio::time::timeout(Duration::from_secs(1), watcher.changed())
        .await
        .expect("second deadline")
        .expect("second event");
    assert!(second.contains(&WatchSource::Server));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn initial_failure_names_exact_target_and_cause() {
    let target = PathBuf::from("/proc/1/mem/no/such/file");
    let error = FileWatcher::new(vec![WatchTarget::new(WatchSource::Config, &target)])
        .err()
        .expect("non-directory ancestor");
    let detail = error.to_string();
    assert!(detail.contains(&target.display().to_string()));
    assert!(detail.to_ascii_lowercase().contains("directory"));
}

#[tokio::test]
async fn recovery_masks_rearm_and_full_rescan() {
    let root = fixture("recovery-masks");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fixture");
    let target = root.join("status");
    let mut watcher = FileWatcher::new(vec![WatchTarget::new(WatchSource::Server, &target)])
        .expect("initial watch");
    for mask in [
        AddWatchFlags::IN_Q_OVERFLOW,
        AddWatchFlags::IN_IGNORED,
        AddWatchFlags::IN_DELETE_SELF,
        AddWatchFlags::IN_MOVE_SELF,
        AddWatchFlags::IN_UNMOUNT,
    ] {
        assert_eq!(
            watcher.simulate_recovery(mask).expect("recovery rearm"),
            BTreeSet::from([WatchSource::Server])
        );
        assert!(!watcher.directories.is_empty());
    }
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn directory_target_watches_children_and_survives_delete_recreate() {
    let root = fixture("directory-recreate");
    let _ = std::fs::remove_dir_all(&root);
    let target = root.join("state/presented");
    std::fs::create_dir_all(&target).expect("create target");
    let mut watcher = FileWatcher::new(vec![WatchTarget::new(WatchSource::Presentation, &target)])
        .expect("watch target and parent");
    let watched = watcher.directories.values().collect::<BTreeSet<_>>();
    assert!(watched.contains(&target));
    assert!(watched.contains(&target.parent().expect("target parent").to_path_buf()));

    std::fs::remove_dir(&target).expect("delete target");
    let deleted = tokio::time::timeout(Duration::from_secs(1), watcher.changed())
        .await
        .expect("delete deadline")
        .expect("delete event");
    assert_eq!(deleted, BTreeSet::from([WatchSource::Presentation]));

    std::fs::create_dir(&target).expect("recreate target");
    let recreated = tokio::time::timeout(Duration::from_secs(1), watcher.changed())
        .await
        .expect("recreate deadline")
        .expect("recreate event");
    assert_eq!(recreated, BTreeSet::from([WatchSource::Presentation]));
    std::fs::write(target.join("1"), []).expect("write child");
    let child = tokio::time::timeout(Duration::from_secs(1), watcher.changed())
        .await
        .expect("child deadline")
        .expect("child event");
    assert_eq!(child, BTreeSet::from([WatchSource::Presentation]));
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn target_replacement_returns_post_arm_rescan_sources() {
    let root = fixture("post-arm");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("create fixture");
    let first = root.join("first");
    let second = root.join("second");
    let mut watcher = FileWatcher::new(vec![WatchTarget::new(WatchSource::Config, first)])
        .expect("initial watch");

    let sources = watcher
        .set_targets(vec![WatchTarget::new(WatchSource::Style, second)])
        .expect("replace targets")
        .expect("changed targets rescan");

    assert_eq!(sources, BTreeSet::from([WatchSource::Style]));
    let _ = std::fs::remove_dir_all(root);
}
