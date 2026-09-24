#![allow(clippy::expect_used)]

use super::*;

fn fixture(tag: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-tooltip-ui-{tag}-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("create fixture directory");
    (root.join("config.toml"), root)
}

#[test]
fn edits_section_and_item_order_without_losing_other_config() {
    let (path, root) = fixture("edit");
    fs::write(
        &path,
        "# keep header\n[tooltip]\norder = [\"cpumem\", \"drives\", \"missing\"] # keep order note\n\n[tooltip.cpumem]\ntitle = \"CPU & MEM\"\nitems = [\"cpu_usage\", \"cpu_freq\", \"future_token\"]\n\n[tooltip.drives]\ntitle = \"DRIVES\"\nitems = [\"disk_usage\"]\n\n[tooltip.extra]\ntitle = \"EXTRA\"\nitems = [\"uptime\"]\n\n[panel]\norder = [\"thermal\"]\n",
    )
    .expect("write config");
    let snapshot: TooltipSnapshot =
        serde_json::from_str(&show(&path).expect("show tooltip")).expect("decode snapshot");
    assert_eq!(snapshot.sections.len(), 3);
    assert_eq!(snapshot.sections[0].key, "cpumem");
    assert!(snapshot.sections[0].enabled);
    assert_eq!(snapshot.sections[2].key, "extra");
    assert!(!snapshot.sections[2].enabled);
    assert!(snapshot.available.contains(&"swap_usage".to_owned()));
    assert!(!snapshot.available.contains(&"cpu_usage:bar".to_owned()));

    let mut sections = snapshot.sections;
    sections[0].items = vec![
        "swap_usage".to_owned(),
        "cpu_usage".to_owned(),
        "future_token".to_owned(),
    ];
    sections[1].enabled = false;
    sections[2].enabled = true;
    sections.rotate_right(1);
    apply(
        &path,
        &serde_json::to_string(&TooltipEdit { sections }).expect("encode edit"),
    )
    .expect("save tooltip");

    let updated = fs::read_to_string(&path).expect("read saved config");
    assert!(updated.contains("# keep header"));
    assert!(updated.contains("# keep order note"));
    assert!(updated.contains("[panel]\norder = [\"thermal\"]"));
    assert!(updated.contains("title = \"CPU & MEM\""));
    assert!(updated.contains("future_token"));
    let raw: toml::Value = toml::from_str(&updated).expect("parse saved config");
    assert_eq!(
        raw["tooltip"]["order"]
            .as_array()
            .expect("section order")
            .iter()
            .filter_map(toml::Value::as_str)
            .collect::<Vec<_>>(),
        ["extra", "cpumem", "missing"]
    );
    let loaded = crate::config::load_config(Some(&path), Some(false)).expect("daemon loads edit");
    assert_eq!(
        loaded
            .tooltip
            .sections
            .iter()
            .map(|section| section.key.as_str())
            .collect::<Vec<_>>(),
        ["extra", "cpumem"]
    );
    assert_eq!(
        loaded.tooltip.sections[1].items,
        ["swap_usage", "cpu_usage"]
    );
    fs::remove_dir_all(root).expect("remove fixture directory");
}

#[test]
fn rejects_unknown_sections_and_panel_only_items_without_writing() {
    let (path, root) = fixture("invalid");
    fs::write(
        &path,
        "[tooltip]\norder = [\"cpu\"]\n[tooltip.cpu]\nitems = [\"cpu_usage\"]\n",
    )
    .expect("write config");
    let original = fs::read(&path).expect("read original");
    let mut edit: TooltipSnapshot =
        serde_json::from_str(&show(&path).expect("show tooltip")).expect("decode snapshot");
    edit.sections[0].items.push("cpu_usage:bar".to_owned());
    assert!(apply(&path, &serde_json::to_string(&edit).expect("encode edit")).is_err());
    edit.sections[0].items = vec!["cpu_usage".to_owned()];
    edit.sections[0].key = "other".to_owned();
    assert!(apply(&path, &serde_json::to_string(&edit).expect("encode edit")).is_err());
    assert_eq!(fs::read(&path).expect("read config"), original);
    fs::remove_dir_all(root).expect("remove fixture directory");
}
