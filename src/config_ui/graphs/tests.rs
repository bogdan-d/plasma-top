#![allow(clippy::expect_used)]

use super::*;

fn fixture(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root =
        std::env::temp_dir().join(format!("plasma-top-graphs-ui-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create fixture directory");
    (root.join("config.toml"), root)
}

#[test]
fn edits_chart_order_and_history_without_losing_other_config() {
    let (path, root) = fixture("edit");
    fs::write(
        &path,
        "# keep header\n[pages]\norder = [\"graphs\", \"processes\"]\ngraph_history_length = 60 # keep note\n\n[tooltip]\norder = [\"cpumem\"]\n",
    )
    .expect("write config");
    let snapshot: GraphSnapshot =
        serde_json::from_str(&show(&path).expect("show Graphs settings")).expect("decode snapshot");
    assert_eq!(
        snapshot.order,
        ["cpu", "memory", "gpu", "network", "temperature"]
    );
    assert_eq!(snapshot.history_length, 60);
    assert_eq!(snapshot.available, GRAPH_CHARTS);

    apply(
        &path,
        &serde_json::to_string(&GraphEdit {
            order: vec!["network".to_owned(), "cpu".to_owned()],
            history_length: 120,
        })
        .expect("encode edit"),
    )
    .expect("save Graphs settings");
    let updated = fs::read_to_string(&path).expect("read saved config");
    assert!(updated.contains("# keep header"));
    assert!(updated.contains("# keep note"));
    assert!(updated.contains("[tooltip]\norder = [\"cpumem\"]"));
    let loaded = crate::config::load_config(Some(&path), Some(false)).expect("load saved config");
    assert_eq!(loaded.pages.graph_order, ["network", "cpu"]);
    assert_eq!(loaded.pages.graph_history_length, 120);
    fs::remove_dir_all(root).expect("remove fixture directory");
}

#[test]
fn rejects_duplicate_or_unknown_charts_and_bad_history_without_writing() {
    let (path, root) = fixture("invalid");
    fs::write(&path, "[pages]\ngraph_history_length = 60\n").expect("write config");
    let original = fs::read(&path).expect("read original");
    for edit in [
        GraphEdit {
            order: vec!["cpu".to_owned(), "cpu".to_owned()],
            history_length: 60,
        },
        GraphEdit {
            order: vec!["future".to_owned()],
            history_length: 60,
        },
        GraphEdit {
            order: vec!["cpu".to_owned()],
            history_length: 0,
        },
    ] {
        assert!(apply(&path, &serde_json::to_string(&edit).expect("encode edit")).is_err());
    }
    assert_eq!(fs::read(&path).expect("read config"), original);
    fs::remove_dir_all(root).expect("remove fixture directory");
}
