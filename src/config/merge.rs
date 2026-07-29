//! Raw TOML merge, machine resolution, and asset path selection.
//!
//! Mirrors the untyped-table layer of `src/config.py` (lines 30–67, 424–456,
//! 738–769): the merge grammar that both machine blocks and orientation
//! overrides use, the surface/section parser, and the per-asset path
//! resolution. The typed view lives in [`super`]; this module stays on
//! [`toml::Table`] / [`toml::Value`] so the merge runs identically to
//! Python's `_deep_merge` regardless of the typed schema on either side.

use std::path::{Path, PathBuf};

use toml::{Table, Value};

use super::assets::{code_root, parent_or_dot, shipped_config, shipped_machines, xdg_dir};
use super::{Section, Surface};

/// Recursively merges `override_` into `base`, returning a new table.
///
/// Mirrors Python's `_deep_merge`: nested tables merge recursively; any
/// non-table value in `override_` (or a type mismatch between base and
/// override) replaces the base entry. Neither input is mutated.
///
/// Used for both the machine block merge and the `[panel_horizontal]` /
/// `[panel_vertical]` orientation override, exactly as in Python.
#[must_use]
pub fn deep_merge_tables(mut base: Table, override_: Table) -> Table {
    for (key, override_value) in override_ {
        match base.remove(&key) {
            Some(Value::Table(mut base_sub)) => match override_value {
                Value::Table(override_sub) => {
                    base_sub = deep_merge_tables(base_sub, override_sub);
                    base.insert(key, Value::Table(base_sub));
                }
                other => {
                    // override replaces a table with a non-table — matches Python.
                    base.insert(key, other);
                }
            },
            _ => {
                // base had no entry or a non-table value: override wins.
                base.insert(key, override_value);
            }
        }
    }
    base
}

/// Returns the final ordered item list for one section.
///
/// Mirrors Python's `_resolve_items`: base `items`, then `items_add`
/// (appended if absent), then `items_remove` applied. `items` itself has
/// already replaced the base list via [`deep_merge_tables`] before this
/// function runs — that's the "items replace" half of the grammar.
#[must_use]
pub fn resolve_items(sec: &Table) -> Vec<String> {
    let mut items = string_array(sec, "items");

    for add in iter_string_array(sec, "items_add") {
        if !items.contains(&add) {
            items.push(add);
        }
    }
    for remove in iter_string_array(sec, "items_remove") {
        items.retain(|item| item != &remove);
    }
    items
}

/// Builds a [`Surface`] from a raw `[panel]` / `[tooltip]` table.
///
/// Mirrors Python's `_parse_surface`:
/// - `order` (+ a machine's `order_add`) lists section keys in render order;
/// - each key maps to a sub-table with `title` and `items` (+ additive knobs);
/// - scalar keys beside the section tables (today only `glyphs`) are the
///   surface's own options, skipped by the `order` walk.
///
/// Unknown `order` entries (no matching sub-table) are skipped silently,
/// matching Python.
#[must_use]
pub fn parse_surface(raw: &Table) -> Surface {
    let mut order = string_array(raw, "order");
    for add in iter_string_array(raw, "order_add") {
        if !order.contains(&add) {
            order.push(add);
        }
    }

    let mut sections = Vec::with_capacity(order.len());
    for key in &order {
        if let Some(sec) = raw.get(key).and_then(Value::as_table) {
            let title = sec
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let items = resolve_items(sec);
            sections.push(Section {
                key: key.clone(),
                title,
                items,
            });
        }
    }

    let glyphs = raw.get("glyphs").and_then(Value::as_bool).unwrap_or(true);
    Surface { sections, glyphs }
}

/// Loads a flat TOML table from `path`, returning an empty table on any
/// missing/unreadable/malformed input.
///
/// Mirrors Python's `_load_toml_at`: used for the glyph theme
/// (`style/icons.toml`) and the labels i18n files (`lang/<language>.toml`),
/// both external files resolved against the asset roots (not the config).
/// A best-effort reader — a malformed theme file falls back to the empty
/// table rather than failing the load (Python swallows `OSError` and
/// `tomllib.TOMLDecodeError`).
#[must_use]
pub fn load_toml_at(path: &Path) -> Table {
    match std::fs::read(path) {
        Ok(bytes) => toml::from_slice::<Table>(&bytes).unwrap_or_default(),
        Err(_) => Table::new(),
    }
}

/// Loads the machine blocks that feed [`super::load_config`].
///
/// Mirrors Python's `_load_machines`: machine blocks (one top-level table
/// per machine) merged low→high from [`machine_source_paths`]. The default
/// resolution reads the shipped base + the user's XDG override; an explicit
/// `--config` reads only its own sibling. `{}` if none exist or all are
/// malformed.
#[must_use]
pub fn load_machines(config_path: Option<&Path>) -> Table {
    let mut machines = Table::new();
    for path in machine_source_paths(config_path) {
        if !path.exists() {
            continue;
        }
        if let Ok(bytes) = std::fs::read(&path) {
            if let Ok(loaded) = toml::from_slice::<Table>(&bytes) {
                machines = deep_merge_tables(machines, loaded);
            }
        }
    }
    machines
}

/// The `config.toml` loaded when the CLI gives no `--config`.
///
/// Mirrors Python's `default_config_path`: the user's XDG copy
/// (`~/.config/plasma-top/config.toml`) if present, else the shipped default.
/// The XDG file replaces the shipped one wholesale (conky model) — the user
/// copies the default and edits it, rather than layering on top.
#[must_use]
pub fn default_config_path() -> PathBuf {
    let xdg = xdg_dir().join("config.toml");
    if xdg.exists() { xdg } else { shipped_config() }
}

/// Returns the path to a `style/` asset.
///
/// Mirrors Python's `resolve_style`: the user's XDG override
/// (`~/.config/plasma-top/style/<name>`) if present, else the shipped one
/// under [`code_root`]. Resolved independently of the config path so it
/// stays correct when `config.toml` itself is loaded from XDG.
#[must_use]
pub fn resolve_style(name: &str) -> PathBuf {
    let xdg = xdg_dir().join("style").join(name);
    if xdg.exists() {
        xdg
    } else {
        code_root().join("style").join(name)
    }
}

/// Returns the user's own `machines.toml`.
///
/// Mirrors Python's `user_machines_path`: `~/.config/plasma-top/machines.toml`,
/// absent on a fresh install (nothing personal ships).
#[must_use]
pub fn user_machines_path() -> PathBuf {
    xdg_dir().join("machines.toml")
}

/// Returns `machines.toml` next to `config_path`.
///
/// Mirrors Python's `machines_path_for`: an explicit `--config` keeps its
/// own machines sibling, so a self-contained config set — and the tests —
/// stay isolated from the user's real hardware.
#[must_use]
pub fn machines_path_for(config_path: &Path) -> PathBuf {
    parent_or_dot(config_path).join("machines.toml")
}

/// Returns the `machines.toml` files that feed [`load_machines`], in load
/// order.
///
/// Mirrors Python's `machine_source_paths`: the default resolution reads
/// the shipped base + the user's XDG override; an explicit `--config` reads
/// only its own sibling. This is the list the daemon watches for
/// hot-reload.
#[must_use]
pub fn machine_source_paths(config_path: Option<&Path>) -> Vec<PathBuf> {
    match config_path {
        None => vec![shipped_machines(), user_machines_path()],
        Some(path) => vec![machines_path_for(path)],
    }
}

/// Collects a TOML string-array field into a `Vec<String>`.
///
/// Tolerates any shape — non-array values, non-string elements, and missing
/// keys all yield an empty contribution. Mirrors Python's implicit
/// `list(sec.get(key, []))` on lists of strings.
fn string_array(table: &Table, key: &str) -> Vec<String> {
    iter_string_array(table, key).collect()
}

/// Iterates a TOML string-array field as owned `String`s.
///
/// Helper that lets [`resolve_items`] and [`parse_surface`] share the
/// tolerant iteration policy without collecting twice.
fn iter_string_array(table: &Table, key: &str) -> impl Iterator<Item = String> {
    table
        .get(key)
        .and_then(Value::as_array)
        .map(|array| {
            array
                .iter()
                .filter_map(|value| value.as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter()
}

#[cfg(test)]
mod tests {
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
}
