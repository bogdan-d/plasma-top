#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use toml::toml;

// ── deep_merge_tables ───────────────────────────────────────────────────

#[test]
fn deep_merge_override_scalar() {
    let base = toml! { a = 1 b = 2 };
    let override_ = toml! { b = 3 };

    let merged = deep_merge_tables(base, override_);

    assert_eq!(merged.get("a").and_then(Value::as_integer), Some(1));
    assert_eq!(merged.get("b").and_then(Value::as_integer), Some(3));
}

#[test]
fn deep_merge_nested_dicts_merge_recursively() {
    let base = toml! { [panel] cpu_usage = true mem_usage = true };
    let override_ = toml! { [panel] mem_usage = false };

    let merged = deep_merge_tables(base, override_);

    let panel = merged.get("panel").and_then(Value::as_table).unwrap();
    assert_eq!(panel.get("cpu_usage").and_then(Value::as_bool), Some(true));
    assert_eq!(panel.get("mem_usage").and_then(Value::as_bool), Some(false));
}

#[test]
fn deep_merge_does_not_mutate_base() {
    let base = toml! { [a] x = 1 };
    let override_ = toml! { [a] x = 2 };

    let _ = deep_merge_tables(base.clone(), override_);

    // The base clone is unmodified: deep_merge takes ownership of the
    // passed table but never reaches into the original the caller kept.
    assert_eq!(
        base.get("a")
            .and_then(Value::as_table)
            .unwrap()
            .get("x")
            .and_then(Value::as_integer),
        Some(1),
    );
}

#[test]
fn deep_merge_dict_replaces_non_dict() {
    let base = toml! { a = 1 };
    let override_ = toml! { [a] x = 2 };

    let merged = deep_merge_tables(base, override_);

    let a = merged.get("a").unwrap();
    assert!(a.is_table(), "table replaces scalar: got {a:?}");
    assert_eq!(
        a.as_table().unwrap().get("x").and_then(Value::as_integer),
        Some(2),
    );
}

// ── resolve_items / parse_surface ───────────────────────────────────────

#[test]
fn resolve_items_plain() {
    let sec = toml! { items = ["a", "b"] };

    assert_eq!(resolve_items(&sec), vec!["a".to_owned(), "b".to_owned()]);
}

#[test]
fn resolve_items_add_appends_without_dups_preserving_order() {
    let sec = toml! { items = ["a", "b"] items_add = ["b", "c"] };

    assert_eq!(
        resolve_items(&sec),
        vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
    );
}

#[test]
fn resolve_items_remove() {
    let sec = toml! { items = ["a", "b", "c"] items_remove = ["b"] };

    assert_eq!(resolve_items(&sec), vec!["a".to_owned(), "c".to_owned()]);
}

#[test]
fn parse_surface_order_drives_sections() {
    let raw = toml! {
        order = ["live", "io"]
        [live]
        title = "Live"
        items = ["cpu_usage"]
        [io]
        title = "I/O"
        items = ["net_speed"]
        [ghost]
        items = ["nope"]
    };

    let surface = parse_surface(&raw);

    let keys: Vec<&str> = surface.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["live", "io"]);
    assert_eq!(surface.sections[0].title, "Live");
    assert!(surface.has("net_speed"));
    assert!(!surface.has("nope"));
    assert_eq!(
        surface.item_set(),
        ["cpu_usage".to_owned(), "net_speed".to_owned()]
            .into_iter()
            .collect(),
    );
}

#[test]
fn parse_surface_order_add_appends_section() {
    let raw = toml! {
        order = ["live"]
        order_add = ["extra"]
        [live]
        items = ["cpu_usage"]
        [extra]
        items = ["uptime"]
    };

    let surface = parse_surface(&raw);

    let keys: Vec<&str> = surface.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["live", "extra"]);
}

#[test]
fn parse_surface_glyphs_default_true_overridden_false() {
    let raw_default = toml! { order = [] };
    let raw_off = toml! { order = [] glyphs = false };

    assert!(parse_surface(&raw_default).glyphs);
    assert!(!parse_surface(&raw_off).glyphs);
}

// ── load_toml_at ────────────────────────────────────────────────────────

#[test]
fn load_toml_at_returns_empty_table_for_missing_path() {
    let table = load_toml_at(Path::new("/does/not/exist.toml"));

    assert!(table.is_empty());
}

#[test]
fn load_toml_at_returns_empty_table_for_malformed_content() {
    let tmp = std::env::temp_dir().join(format!(
        "plasma-top-merge-malformed-{}.toml",
        std::process::id()
    ));
    std::fs::write(&tmp, "this is = not = valid\n").unwrap();
    let table = load_toml_at(&tmp);
    let _ = std::fs::remove_file(&tmp);

    assert!(table.is_empty());
}

#[test]
fn load_toml_at_parses_valid_toml() {
    let tmp = std::env::temp_dir().join(format!(
        "plasma-top-merge-valid-{}.toml",
        std::process::id()
    ));
    std::fs::write(&tmp, "cpu_usage = \"glyph\"\n").unwrap();
    let table = load_toml_at(&tmp);
    let _ = std::fs::remove_file(&tmp);

    assert_eq!(
        table.get("cpu_usage").and_then(Value::as_str),
        Some("glyph"),
    );
}

// ── machine_source_paths / machines_path_for ────────────────────────────

#[test]
fn machine_source_paths_default_resolution() {
    let paths = machine_source_paths(None);

    assert_eq!(paths.len(), 2);
    assert!(paths[0].ends_with("config/machines.toml"));
    assert!(paths[1].ends_with(".config/plasma-top/machines.toml"));
}

#[test]
fn machine_source_paths_explicit_config_keeps_sibling_only() {
    let paths = machine_source_paths(Some(Path::new("/tmp/my/config.toml")));

    assert_eq!(paths, vec![PathBuf::from("/tmp/my/machines.toml")]);
}

#[test]
fn machines_path_for_returns_parent_sibling() {
    let path = machines_path_for(Path::new("/tmp/dir/config.toml"));

    assert_eq!(path, PathBuf::from("/tmp/dir/machines.toml"));
}

#[test]
fn machines_path_for_bare_filename_yields_dot_parent() {
    let path = machines_path_for(Path::new("config.toml"));

    assert_eq!(path, PathBuf::from("./machines.toml"));
}
