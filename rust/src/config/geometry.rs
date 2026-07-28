//! Panel geometry, Plasma appletsrc parsing, and the vertical auto-fit.
//!
//! Mirrors `src/config.py` lines 380–401 (machine detect) and 471–716
//! (geometry + auto-fit). Every disk-touching function has an `_at` / `_text`
//! / `_with_*` test seam that takes the input directly so tests don't mutate
//! process env (edition-2024 makes `env::set_var` / `env::remove_var`
//! `unsafe`).
//!
//! [`detect_machine`] is the odd one out — it has nothing to do with panel
//! geometry, but it shares the same DMI-touching pattern and is colocated
//! here because its pure core [`detect_machine_with_dmi`]
//! takes the board/product strings directly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use toml::Table;

use super::Config;
use super::assets::{home_dir, parent_or_dot};
use super::{BAR_SAFETY_PX, COLUMN_DIGIT_RATIO, CSS_ADVANCE_RATIO};

/// Default location of the live Plasma appletsrc (matches Python
/// `PLASMA_APPLETSRC`).
#[must_use]
pub fn plasma_appletsrc_path() -> PathBuf {
    home_dir()
        .join(".config")
        .join("plasma-org.kde.plasma.desktop-appletsrc")
}

/// Persistent geom cache (matches Python `GEOM_CACHE`).
///
/// The runtime dir is tmpfs, wiped at logout; this cache survives reboots so
/// the first paint after a cold start is already width-fitted.
#[must_use]
pub fn geom_cache_path() -> PathBuf {
    home_dir().join(".cache").join("plasma-top").join("geom")
}

/// Panel facts used at load time.
///
/// Orientation is always resolvable (defaults vertical); the measured
/// `usable_px` and `glyph_adv` are present only when the plasmoid has
/// published a geometry file. `tooltip_adv` is orientation-independent and
/// sizes the graphs-page PNGs to the tooltip's real text width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelGeometry {
    /// Panel edge: vertical (left/right) vs horizontal (top/bottom).
    pub vertical: bool,
    /// Usable px of the panel's text area (vertical panel only).
    pub usable_px: Option<f64>,
    /// Real on-screen advance of one main-font monospace glyph.
    pub glyph_adv: Option<f64>,
    /// Real on-screen advance of one tooltip-font monospace glyph.
    pub tooltip_adv: Option<f64>,
}

impl PanelGeometry {
    /// Constructs a geometry with only orientation set, no measurements.
    #[must_use]
    pub fn only_vertical(vertical: bool) -> Self {
        Self {
            vertical,
            usable_px: None,
            glyph_adv: None,
            tooltip_adv: None,
        }
    }
}

impl Default for PanelGeometry {
    fn default() -> Self {
        // Python default is `PanelGeometry()` with `vertical=True`.
        Self::only_vertical(true)
    }
}

// ── Plasma appletsrc parsing ────────────────────────────────────────────────

/// Parses KDE's appletsrc into `{full-bracketed-header: {key: value}}`.
///
/// Mirrors Python's `_parse_kde_ini`. Headers keep their exact nested form
/// (e.g. `[Containments][2][Applets][25]`) so callers match them with the
/// manual `applet_root_containment` parser; a hand parse avoids
/// `configparser`'s greedy-header surprises on the `[a][b]` section names.
#[must_use]
pub fn parse_kde_ini(text: &str) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut sections: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let trimmed = line;
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let header = trimmed.to_owned();
            sections.entry(header.clone()).or_default();
            current = Some(header);
        } else if let Some(header) = &current {
            if let Some(entry) = sections.get_mut(header) {
                if let Some((key, value)) = trimmed.split_once('=') {
                    entry.insert(key.trim().to_owned(), value.trim().to_owned());
                }
            }
        }
    }
    sections
}

/// Returns the containment number when `header` matches the plasma-top applet
/// root pattern: `[Containments][<num>]` followed by one or more
/// `[Applets][<num>]` and nothing else.
///
/// Manual port of Python's `_APPLET_ROOT_RE` regex
/// (`^(\[Containments\]\[(\d+)\](?:\[Applets\]\[\d+\])+)$`). Returns the
/// captured containment number (group 2 in the Python regex); `None` when
/// the header does not match.
#[must_use]
fn applet_root_containment(header: &str) -> Option<&str> {
    let after_open = header.strip_prefix("[Containments][")?;
    let (num, after_num_close) = after_open.split_once(']')?;
    if num.is_empty() || !num.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let mut rest = after_num_close;
    let mut matched_any_applet = false;
    while let Some(after_applet_open) = rest.strip_prefix("[Applets][") {
        let (applet_num, after_applet_close) = after_applet_open.split_once(']')?;
        if applet_num.is_empty() || !applet_num.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        rest = after_applet_close;
        matched_any_applet = true;
    }

    if matched_any_applet && rest.is_empty() {
        Some(num)
    } else {
        None
    }
}

/// Panel orientation from the live appletsrc text.
///
/// Mirrors Python's `_detect_vertical_from_appletsrc` body (minus the
/// `Path.read_text` call): our applet's containment edge (location 5/6 =
/// left/right → vertical, 3/4 = top/bottom → horizontal), defaulting to
/// vertical when no applet is found or the location is outside the known
/// set. Pure version of [`detect_vertical_from_appletsrc`] for tests.
#[must_use]
pub fn detect_vertical_from_appletsrc_text(text: &str) -> bool {
    let sections = parse_kde_ini(text);
    for (header, kv) in &sections {
        if let Some(containment) = applet_root_containment(header) {
            let is_target_applet =
                kv.get("plugin").map(String::as_str) == Some("com.github.bogdan-d.plasma-top");
            if is_target_applet {
                let containment_header = format!("[Containments][{containment}]");
                let location = sections
                    .get(&containment_header)
                    .and_then(|table| table.get("location"))
                    .and_then(|s| s.parse::<i64>().ok());
                return match location {
                    Some(3) | Some(4) => false,
                    Some(5) | Some(6) => true,
                    _ => true,
                };
            }
        }
    }
    true
}

/// Panel orientation from the live appletsrc file.
///
/// Mirrors Python's `_detect_vertical_from_appletsrc`: reads the file at
/// [`plasma_appletsrc_path`] and delegates to
/// [`detect_vertical_from_appletsrc_text`]. Falls back to vertical on any
/// read failure (file absent, permissions, non-UTF-8).
#[must_use]
pub fn detect_vertical_from_appletsrc_at(path: &Path) -> bool {
    match std::fs::read_to_string(path) {
        Ok(text) => detect_vertical_from_appletsrc_text(&text),
        Err(_) => true,
    }
}

/// Panel orientation from the default appletsrc location.
#[must_use]
pub fn detect_vertical_from_appletsrc() -> bool {
    detect_vertical_from_appletsrc_at(&plasma_appletsrc_path())
}

// ── Geom file parsing ───────────────────────────────────────────────────────

/// Parses a geom line.
///
/// Mirrors Python's `_parse_geom`: `<usable_px> <glyph_adv_px> <vertical 0|1>
/// [tooltip_adv_px]`. Returns `None` when malformed or degenerate so a
/// half-written or startup-zero file falls back to the config's own values
/// rather than producing a nonsensical fit. The tooltip advance is optional
/// (absent on geoms from the pre-tooltip-metrics plasmoid); a non-positive
/// tooltip advance is dropped to `None`.
#[must_use]
pub fn parse_geom(text: &str) -> Option<PanelGeometry> {
    let parts: Vec<&str> = text.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    let usable: f64 = parts[0].parse().ok()?;
    let adv: f64 = parts[1].parse().ok()?;
    let tip: Option<f64> = if parts.len() > 3 {
        let parsed: f64 = parts[3].parse().ok()?;
        Some(parsed)
    } else {
        None
    };

    if usable <= 0.0 || adv <= 0.0 {
        return None;
    }
    let tooltip_adv = tip.filter(|value| *value > 0.0);
    let vertical = parts[2] == "1";
    Some(PanelGeometry {
        vertical,
        usable_px: Some(usable),
        glyph_adv: Some(adv),
        tooltip_adv,
    })
}

/// Reads the live geom file or the persisted cache when live is absent.
///
/// Mirrors Python's `_read_geom_file`. At session start tmpfs is wiped, so
/// only the cache carries a fit until the plasmoid republishes — this is
/// what lets the boot paint be width-fitted from the previous session. The
/// cache write-back is the daemon's job ([`cache_live_geom_at`]), not this
/// read path, so config stays side-effect-free.
#[must_use]
pub fn read_geom_file_at(live: &Path, cache: &Path) -> Option<PanelGeometry> {
    if let Ok(text) = std::fs::read_to_string(live) {
        if let Some(geo) = parse_geom(&text) {
            return Some(geo);
        }
    }
    std::fs::read_to_string(cache)
        .ok()
        .and_then(|text| parse_geom(&text))
}

/// Reads the live geom file from its default locations.
#[must_use]
pub fn read_geom_file() -> Option<PanelGeometry> {
    read_geom_file_at(&crate::runtime::geom_file(), &geom_cache_path())
}

/// Persists a valid live geom file to the cache.
///
/// Mirrors Python's `cache_live_geom`. Best-effort: a read/write failure or
/// a degenerate file is silently ignored — never disturbs a render. Called
/// by the daemon when the plasmoid publishes a fresh geometry.
pub fn cache_live_geom_at(live: &Path, cache: &Path) {
    let text = match std::fs::read_to_string(live) {
        Ok(text) => text,
        Err(_) => return,
    };
    if parse_geom(&text).is_none() {
        return;
    }
    if let Some(parent) = cache.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let _ = std::fs::write(cache, text);
}

/// Persists the live geom file from its default locations.
pub fn cache_live_geom() {
    cache_live_geom_at(&crate::runtime::geom_file(), &geom_cache_path());
}

/// Resolves the panel geometry for [`super::load_config`].
///
/// Mirrors Python's `detect_panel_geometry`: orientation comes from
/// appletsrc (Plasma updates it synchronously when the panel moves); the
/// measured width/advance come from the geom file when its own orientation
/// flag matches the resolved one (a stale geom still describing the old edge
/// was measured on the wrong axis, so we keep the orientation but skip its
/// numbers). The tooltip advance is orientation-independent, so it survives
/// the stale check.
#[must_use]
pub fn detect_panel_geometry_at(
    appletsrc: &Path,
    live_geom: &Path,
    cache_geom: &Path,
) -> PanelGeometry {
    let vertical = detect_vertical_from_appletsrc_at(appletsrc);
    let geo = read_geom_file_at(live_geom, cache_geom);
    let tooltip_adv = geo.and_then(|g| g.tooltip_adv);
    match geo {
        Some(g) if g.vertical == vertical => PanelGeometry {
            vertical,
            usable_px: g.usable_px,
            glyph_adv: g.glyph_adv,
            tooltip_adv,
        },
        _ => PanelGeometry {
            vertical,
            usable_px: None,
            glyph_adv: None,
            tooltip_adv,
        },
    }
}

/// Resolves panel geometry from the default locations.
#[must_use]
pub fn detect_panel_geometry() -> PanelGeometry {
    detect_panel_geometry_at(
        &plasma_appletsrc_path(),
        &crate::runtime::geom_file(),
        &geom_cache_path(),
    )
}

/// Orientation alone (see [`detect_panel_geometry`]).
#[must_use]
pub fn detect_vertical_layout() -> bool {
    detect_panel_geometry().vertical
}

// ── Auto-fit ────────────────────────────────────────────────────────────────

/// Sizes the panel visuals to the real panel, in place.
///
/// Mirrors Python's `_auto_fit_panel`. Both orientations need the glyph
/// advance; the vertical branch also needs the usable width. No-op without
/// the glyph advance (plasmoid hasn't published) — the config's own values
/// stand.
///
/// See the Python docstring for the full rationale; the per-field effects
/// are:
///
/// - **Vertical**: `cols = usable / glyph_adv` (floored); bar width ←
///   `(usable - BAR_SAFETY_PX) / bar_adv` (floored); panel_min_width ← cols;
///   panel spark/braille lengths ← cols; panel_font_size ← the divisor that
///   makes `round(width*height/pfs)` equal `cols` (only when bar height>0).
/// - **Horizontal**: column height ← `main_px * COLUMN_DIGIT_RATIO` rounded,
///   where `main_px = glyph_adv / CSS_ADVANCE_RATIO`.
pub fn auto_fit_panel(cfg: &mut Config, geo: &PanelGeometry) {
    let Some(glyph_adv) = geo.glyph_adv else {
        return;
    };

    if cfg.vertical {
        let Some(usable) = geo.usable_px else {
            return;
        };
        let cols = ((usable / glyph_adv) as i32).max(1);
        let height = cfg.bar_panel.height;
        // CSS-px advance when the bar has its own font-size, else the main
        // pointSize advance (measured live).
        let bar_adv = if height > 0 {
            f64::from(height) * CSS_ADVANCE_RATIO
        } else {
            glyph_adv
        };
        let new_width = (((usable - BAR_SAFETY_PX) / bar_adv) as i32).max(1);
        cfg.bar_panel.width = new_width;
        cfg.display.panel_min_width = cols;
        // Standalone spark/braille span the width at the main font → one glyph
        // per column, so cols glyphs fill it like the bar.
        cfg.spark_panel.cpu_spark_length = cols;
        cfg.spark_panel.mem_spark_length = cols;
        cfg.braille_panel.cpu_braille_length = cols;
        cfg.braille_panel.mem_braille_length = cols;
        if height > 0 {
            let divisor =
                (f64::from(new_width) * f64::from(height) / f64::from(cols)).round() as i32;
            cfg.display.panel_font_size = divisor.max(1);
        }
    } else {
        let main_px = glyph_adv / CSS_ADVANCE_RATIO;
        let column_height = (main_px * COLUMN_DIGIT_RATIO).round() as i32;
        cfg.column_panel.height = column_height.max(1);
    }
}

// ── Machine detection ───────────────────────────────────────────────────────

/// The board/product DMI paths the daemon reads for machine detection.
///
/// [`detect_machine`] reads these host paths; tests use
/// [`detect_machine_with_dmi`] to avoid host dependence.
#[must_use]
pub fn dmi_paths() -> (PathBuf, PathBuf) {
    (
        PathBuf::from("/sys/class/dmi/id/board_name"),
        PathBuf::from("/sys/class/dmi/id/product_name"),
    )
}

/// Returns the name of the first machine block whose `[<name>.detect]` rule
/// matches the given DMI board/product strings.
///
/// Mirrors Python's `detect_machine`. The pure core of [`detect_machine`]:
/// takes the board and product strings directly so tests don't need to
/// stage `/sys/class/dmi/id/...`. Returns `None` when no block matches (or
/// when every block is malformed — Python's `isinstance(mdata, dict)`
/// guard). Detection order matches Python's iteration over the machines
/// table; Rust's [`Table`] iterates in insertion order (matching Python's
/// dict, modulo the merged-source ordering).
#[must_use]
pub fn detect_machine_with_dmi(machines: &Table, board: &str, product: &str) -> Option<String> {
    for (name, mdata) in machines.iter() {
        let Some(detect) = mdata.as_table().and_then(|t| t.get("detect")) else {
            continue;
        };
        let Some(detect_table) = detect.as_table() else {
            continue;
        };
        if let Some(board_contains) = detect_table
            .get("board_contains")
            .and_then(toml::Value::as_str)
            .filter(|value| !value.is_empty())
        {
            if board.contains(board_contains) {
                return Some(name.clone());
            }
        }
        if let Some(board_startswith) = detect_table
            .get("board_startswith")
            .and_then(toml::Value::as_str)
            .filter(|value| !value.is_empty())
        {
            if board.starts_with(board_startswith) {
                return Some(name.clone());
            }
        }
        if let Some(product_contains) = detect_table
            .get("product_contains")
            .and_then(toml::Value::as_str)
            .filter(|value| !value.is_empty())
        {
            if product.contains(product_contains) {
                return Some(name.clone());
            }
        }
    }
    None
}

/// Returns the name of the first machine block matching this host's DMI.
///
/// Mirrors Python's `detect_machine`: reads `/sys/class/dmi/id/board_name`
/// and `/sys/class/dmi/id/product_name`. Returns `None` when the DMI files
/// are unreadable (typical on non-physical hosts: VMs, containers) or no
/// block matches.
#[must_use]
pub fn detect_machine(machines: &Table) -> Option<String> {
    let (board_path, product_path) = dmi_paths();
    let board = match std::fs::read_to_string(&board_path) {
        Ok(text) => text.trim().to_owned(),
        Err(_) => return None,
    };
    let product = match std::fs::read_to_string(&product_path) {
        Ok(text) => text.trim().to_owned(),
        Err(_) => return None,
    };
    detect_machine_with_dmi(machines, &board, &product)
}

/// Default parent for an explicit config path (test convenience).
#[must_use]
pub fn config_parent_dir(config_path: &Path) -> &Path {
    parent_or_dot(config_path)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::field_reassign_with_default
    )]

    use super::*;
    use crate::config::merge::deep_merge_tables;
    use toml::toml;

    // ── detect_machine_with_dmi ─────────────────────────────────────────────

    #[test]
    fn detect_machine_with_dmi_board_contains_match() {
        let machines = toml! {
            [laptop.detect]
            board_contains = "Example"
            [desktop.detect]
            board_contains = "Z790"
        };

        assert_eq!(
            detect_machine_with_dmi(&machines, "ABC-1234 Example Board", "Example Laptop 15"),
            Some("laptop".to_owned()),
        );
    }

    #[test]
    fn detect_machine_with_dmi_no_match_returns_none() {
        let machines = toml! {
            [laptop.detect]
            board_contains = "Example"
        };

        assert_eq!(
            detect_machine_with_dmi(&machines, "Some Board", "Some Product"),
            None,
        );
    }

    #[test]
    fn detect_machine_with_dmi_product_contains_match() {
        let machines = toml! {
            [vm.detect]
            product_contains = "ExampleVM"
        };

        assert_eq!(
            detect_machine_with_dmi(&machines, "Generic Board", "ExampleVM 7"),
            Some("vm".to_owned()),
        );
    }

    #[test]
    fn detect_machine_with_dmi_board_startswith_match() {
        let machines = toml! {
            [box.detect]
            board_startswith = "PRIME"
        };

        assert_eq!(
            detect_machine_with_dmi(&machines, "PRIME Z790", "Whatever"),
            Some("box".to_owned()),
        );
    }

    #[test]
    fn detect_machine_with_dmi_ignores_non_dict_entries() {
        // A top-level scalar (not a table) is skipped, not a match.
        let machines = toml! {
            not_a_machine = "oops"
        };

        assert_eq!(
            detect_machine_with_dmi(&machines, "Some Board", "Some Product"),
            None,
        );
    }

    #[test]
    fn detect_machine_with_dmi_ignores_empty_detect_keys() {
        let machines = toml! {
            [empty.detect]
            board_contains = ""
        };

        assert_eq!(
            detect_machine_with_dmi(&machines, "Anything", "Anything"),
            None,
            "empty board_contains must not match every board",
        );
    }

    #[test]
    fn detect_machine_with_dmi_returns_none_on_empty_machines() {
        let machines = Table::new();

        assert_eq!(
            detect_machine_with_dmi(&machines, "Anything", "Anything"),
            None,
        );
    }

    // ── parse_kde_ini + applet_root_containment ─────────────────────────────

    #[test]
    fn parse_kde_ini_splits_headers_and_keyvals() {
        let text = "[Containments][2]\nlocation=5\nplugin=panel\n[Containments][2][Applets][7]\nplugin=foo\n";

        let sections = parse_kde_ini(text);

        assert_eq!(
            sections.get("[Containments][2]").unwrap().get("location"),
            Some(&"5".to_owned()),
        );
        assert_eq!(
            sections
                .get("[Containments][2][Applets][7]")
                .unwrap()
                .get("plugin"),
            Some(&"foo".to_owned()),
        );
    }

    #[test]
    fn applet_root_matches_target_applet_levels() {
        assert_eq!(
            applet_root_containment("[Containments][2][Applets][25]"),
            Some("2")
        );
        assert_eq!(
            applet_root_containment("[Containments][10][Applets][3][Applets][99]"),
            Some("10"),
        );
    }

    #[test]
    fn applet_root_rejects_non_applet_levels() {
        assert_eq!(applet_root_containment("[Containments][2]"), None);
        assert_eq!(applet_root_containment("[Containments][2][General]"), None,);
        assert_eq!(
            applet_root_containment("[Containments][abc][Applets][1]"),
            None,
            "non-digit containment must not match",
        );
        assert_eq!(
            applet_root_containment("[Containments][2][Applets][xyz]"),
            None,
        );
        assert_eq!(applet_root_containment(""), None);
        assert_eq!(applet_root_containment("[Other][2][Applets][1]"), None);
    }

    // ── detect_vertical_from_appletsrc_text ─────────────────────────────────

    #[test]
    fn detect_vertical_from_appletsrc_text_defaults_vertical_with_no_applet() {
        assert!(detect_vertical_from_appletsrc_text(""));
        assert!(detect_vertical_from_appletsrc_text(
            "[Containments][2]\nlocation=4\nplugin=panel\n",
        ));
    }

    #[test]
    fn detect_vertical_from_appletsrc_text_reads_panel_edge_horizontal() {
        let text = "[Containments][2]\nlocation=4\n\
                    [Containments][2][Applets][7]\nplugin=com.github.bogdan-d.plasma-top\n";

        assert!(!detect_vertical_from_appletsrc_text(text));
    }

    #[test]
    fn detect_vertical_from_appletsrc_text_reads_panel_edge_vertical() {
        let text = "[Containments][2]\nlocation=5\n\
                    [Containments][2][Applets][7]\nplugin=com.github.bogdan-d.plasma-top\n";

        assert!(detect_vertical_from_appletsrc_text(text));
    }

    #[test]
    fn detect_vertical_from_appletsrc_text_unknown_location_defaults_vertical() {
        let text = "[Containments][2]\nlocation=99\n\
                    [Containments][2][Applets][7]\nplugin=com.github.bogdan-d.plasma-top\n";

        assert!(detect_vertical_from_appletsrc_text(text));
    }

    // ── parse_geom ──────────────────────────────────────────────────────────

    #[test]
    fn parse_geom_parses_three_fields_with_tooltip_adv() {
        let geo = parse_geom("42 6.59375 1 7.5\n").unwrap();

        assert!(geo.vertical);
        assert_eq!(geo.usable_px, Some(42.0));
        assert_eq!(geo.glyph_adv, Some(6.59375));
        assert_eq!(geo.tooltip_adv, Some(7.5));
    }

    #[test]
    fn parse_geom_supports_three_fields_without_tooltip_adv() {
        let geo = parse_geom("42 6.59375 0\n").unwrap();

        assert!(!geo.vertical);
        assert_eq!(geo.usable_px, Some(42.0));
        assert_eq!(geo.glyph_adv, Some(6.59375));
        assert_eq!(geo.tooltip_adv, None);
    }

    #[test]
    fn parse_geom_returns_none_for_short_or_malformed() {
        assert!(parse_geom("42 6.5\n").is_none());
        assert!(parse_geom("garbage\n").is_none());
        assert!(parse_geom("0 0 1\n").is_none(), "degenerate zeroed file");
        assert!(parse_geom("-1 6 1\n").is_none());
        assert!(parse_geom("42 6 1 0\n").is_some_and(|g| g.tooltip_adv.is_none()));
    }

    // ── read_geom_file_at + cache_live_geom_at ──────────────────────────────

    fn write_tmp(name: &str, contents: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("plasma-top-geom-{name}-{}", std::process::id()));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn read_geom_falls_back_to_cache_when_live_absent() {
        let cache = write_tmp("cache-1", "42 6.59375 1\n");
        let live =
            std::env::temp_dir().join(format!("plasma-top-geom-absent-1-{}", std::process::id()));

        let geo = read_geom_file_at(&live, &cache).unwrap();

        assert_eq!(geo.usable_px, Some(42.0));
        assert_eq!(geo.glyph_adv, Some(6.59375));
        let _ = std::fs::remove_file(&cache);
    }

    #[test]
    fn read_geom_prefers_live_over_cache() {
        let live = write_tmp("live-2", "100 5 1\n");
        let cache = write_tmp("cache-2", "42 6.59375 1\n");

        let geo = read_geom_file_at(&live, &cache).unwrap();

        assert_eq!(geo.usable_px, Some(100.0));
        let _ = std::fs::remove_file(&live);
        let _ = std::fs::remove_file(&cache);
    }

    #[test]
    fn read_geom_none_when_live_absent_and_no_cache() {
        let live =
            std::env::temp_dir().join(format!("plasma-top-geom-absent-2-{}", std::process::id()));
        let cache =
            std::env::temp_dir().join(format!("plasma-top-geom-absent-3-{}", std::process::id()));

        assert!(read_geom_file_at(&live, &cache).is_none());
    }

    #[test]
    fn cache_live_geom_persists_valid_live() {
        let live = write_tmp("live-3", "100 5 1\n");
        let cache_parent =
            std::env::temp_dir().join(format!("plasma-top-geom-cache-sub-{}", std::process::id()));
        let cache = cache_parent.join("geom_cache");

        cache_live_geom_at(&live, &cache);

        assert_eq!(std::fs::read_to_string(&cache).unwrap(), "100 5 1\n");
        let _ = std::fs::remove_file(&live);
        let _ = std::fs::remove_dir_all(&cache_parent);
    }

    #[test]
    fn cache_live_geom_ignores_degenerate_and_absent() {
        let cache = std::env::temp_dir().join(format!(
            "plasma-top-geom-cache-degenerate-{}",
            std::process::id()
        ));
        let live = write_tmp("live-4", "0 0 1\n");

        cache_live_geom_at(&live, &cache);
        assert!(
            !cache.exists(),
            "degenerate live geom must not be persisted"
        );

        let absent =
            std::env::temp_dir().join(format!("plasma-top-geom-absent-4-{}", std::process::id()));
        cache_live_geom_at(&absent, &cache);
        assert!(!cache.exists(), "absent live geom must not create a cache");

        let _ = std::fs::remove_file(&live);
        let _ = std::fs::remove_file(&cache);
    }

    // ── detect_panel_geometry_at ────────────────────────────────────────────

    fn appletsrc(location: u32) -> String {
        format!(
            "[Containments][2]\nlocation={location}\n\
             [Containments][2][Applets][25]\nplugin=com.github.bogdan-d.plasma-top\n"
        )
    }

    #[test]
    fn detect_panel_geometry_reads_geom_file() {
        let appletsrc_path = write_tmp("appletsrc-v", &appletsrc(5));
        let geom_path = write_tmp("geom-v", "42 6.59375 1\n");
        let cache_path = std::env::temp_dir().join(format!(
            "plasma-top-geom-cache-absent-{}",
            std::process::id()
        ));

        let geo = detect_panel_geometry_at(&appletsrc_path, &geom_path, &cache_path);

        assert!(geo.vertical);
        assert_eq!(geo.usable_px, Some(42.0));
        assert_eq!(geo.glyph_adv, Some(6.59375));
        let _ = std::fs::remove_file(&appletsrc_path);
        let _ = std::fs::remove_file(&geom_path);
    }

    #[test]
    fn detect_panel_geometry_falls_back_to_appletsrc_orientation() {
        let appletsrc_path = write_tmp("appletsrc-nogeo", &appletsrc(5));
        let geom_path =
            std::env::temp_dir().join(format!("plasma-top-geom-nogeo-{}", std::process::id()));
        let cache_path =
            std::env::temp_dir().join(format!("plasma-top-geom-nocache-{}", std::process::id()));

        let geo = detect_panel_geometry_at(&appletsrc_path, &geom_path, &cache_path);

        assert!(geo.vertical);
        assert!(geo.usable_px.is_none());
        assert!(geo.glyph_adv.is_none());
        let _ = std::fs::remove_file(&appletsrc_path);
    }

    #[test]
    fn detect_panel_geometry_ignores_degenerate_geom_file() {
        let appletsrc_path = write_tmp("appletsrc-zero", &appletsrc(5));
        let geom_zero = write_tmp("geom-zero", "0 0 1\n");
        let geom_garbage = write_tmp("geom-garbage", "garbage\n");
        let cache_absent = std::env::temp_dir().join(format!(
            "plasma-top-geom-cache-absent2-{}",
            std::process::id()
        ));

        for bad in [geom_zero.clone(), geom_garbage.clone()] {
            let geo = detect_panel_geometry_at(&appletsrc_path, &bad, &cache_absent);
            assert!(
                geo.usable_px.is_none(),
                "degenerate geom `{}` should not produce a fit",
                bad.display(),
            );
        }
        let _ = std::fs::remove_file(&appletsrc_path);
        let _ = std::fs::remove_file(&geom_zero);
        let _ = std::fs::remove_file(&geom_garbage);
    }

    #[test]
    fn detect_panel_geometry_stale_geom_orientation_uses_appletsrc() {
        // Vertical panel (location=5), but the geom file still reports the
        // old horizontal edge (vertical=0). The orientation stays the
        // appletsrc's; the measurements for the wrong axis are ignored.
        let appletsrc_path = write_tmp("appletsrc-stale", &appletsrc(5));
        let geom_path = write_tmp("geom-stale", "42 6.59375 0\n");
        let cache_path = std::env::temp_dir().join(format!(
            "plasma-top-geom-cache-stale-{}",
            std::process::id()
        ));

        let geo = detect_panel_geometry_at(&appletsrc_path, &geom_path, &cache_path);

        assert!(geo.vertical);
        assert!(geo.usable_px.is_none());
        assert!(geo.glyph_adv.is_none());
        let _ = std::fs::remove_file(&appletsrc_path);
        let _ = std::fs::remove_file(&geom_path);
    }

    #[test]
    fn detect_panel_geometry_keeps_tooltip_adv_across_stale_edge() {
        // The tooltip advance is orientation-independent; even when the panel
        // geom's edge is stale, the tooltip advance survives the stale drop.
        let appletsrc_path = write_tmp("appletsrc-tip", &appletsrc(5));
        let geom_path = write_tmp("geom-tip", "42 6.59375 0 8.0\n");
        let cache_path =
            std::env::temp_dir().join(format!("plasma-top-geom-cache-tip-{}", std::process::id()));

        let geo = detect_panel_geometry_at(&appletsrc_path, &geom_path, &cache_path);

        assert!(geo.vertical);
        assert!(geo.usable_px.is_none());
        assert_eq!(geo.tooltip_adv, Some(8.0));
        let _ = std::fs::remove_file(&appletsrc_path);
        let _ = std::fs::remove_file(&geom_path);
    }

    #[test]
    fn detect_panel_geometry_defaults_when_unreadable() {
        let appletsrc_absent = std::env::temp_dir().join(format!(
            "plasma-top-appletsrc-absent-{}",
            std::process::id()
        ));
        let geom_absent =
            std::env::temp_dir().join(format!("plasma-top-geom-absent-5-{}", std::process::id()));
        let cache_absent =
            std::env::temp_dir().join(format!("plasma-top-cache-absent-5-{}", std::process::id()));

        let geo = detect_panel_geometry_at(&appletsrc_absent, &geom_absent, &cache_absent);

        assert_eq!(geo, PanelGeometry::default());
    }

    // ── auto_fit_panel ──────────────────────────────────────────────────────

    fn base_config(vertical: bool) -> Config {
        Config {
            vertical,
            ..Config::default()
        }
    }

    #[test]
    fn auto_fit_panel_derives_knobs_from_geometry() {
        let mut cfg = base_config(true);
        cfg.bar_panel.height = 3;

        auto_fit_panel(
            &mut cfg,
            &PanelGeometry {
                vertical: true,
                usable_px: Some(42.0),
                glyph_adv: Some(6.59375),
                tooltip_adv: None,
            },
        );

        // cols = floor(42/6.59375) = 6
        // width = floor((42-1)/(3*0.6)) = 22
        // pfs = round(22*3/6) = 11
        assert_eq!(cfg.display.panel_min_width, 6);
        assert_eq!(cfg.bar_panel.width, 22);
        assert_eq!(cfg.display.panel_font_size, 11);
        // Bar's footprint lands on cols → shared right edge, no wrap.
        assert_eq!(
            (cfg.bar_panel.width * cfg.bar_panel.height / cfg.display.panel_font_size),
            6,
        );
        assert_eq!(cfg.spark_panel.cpu_spark_length, 6);
        assert_eq!(cfg.spark_panel.mem_spark_length, 6);
        assert_eq!(cfg.braille_panel.cpu_braille_length, 6);
        assert_eq!(cfg.braille_panel.mem_braille_length, 6);
    }

    #[test]
    fn auto_fit_bar_height_zero_uses_main_advance() {
        let mut cfg = base_config(true);
        cfg.bar_panel.height = 0;
        let pfs_before = cfg.display.panel_font_size;

        auto_fit_panel(
            &mut cfg,
            &PanelGeometry {
                vertical: true,
                usable_px: Some(42.0),
                glyph_adv: Some(6.59375),
                tooltip_adv: None,
            },
        );

        assert_eq!(cfg.display.panel_min_width, 6);
        assert_eq!(cfg.bar_panel.width, 6, "floor((42-1)/6.59375) = 6");
        assert_eq!(
            cfg.display.panel_font_size, pfs_before,
            "untouched when height=0"
        );
    }

    #[test]
    fn auto_fit_horizontal_sizes_column_height() {
        let mut cfg = base_config(false);
        let before = (cfg.display.panel_min_width, cfg.bar_panel.width);

        auto_fit_panel(
            &mut cfg,
            &PanelGeometry {
                vertical: false,
                usable_px: Some(138.0),
                glyph_adv: Some(6.59375),
                tooltip_adv: None,
            },
        );

        // main_px = 6.59375/0.6 ≈ 10.99; column height = round(10.99*0.612) = 7.
        assert_eq!(cfg.column_panel.height, 7);
        assert_eq!(
            (cfg.display.panel_min_width, cfg.bar_panel.width),
            before,
            "vertical panel knobs untouched in horizontal",
        );
    }

    #[test]
    fn auto_fit_noop_when_geometry_unpublished() {
        for vertical in [true, false] {
            let mut cfg = base_config(vertical);
            let snap = (
                cfg.display.panel_font_size,
                cfg.display.panel_min_width,
                cfg.bar_panel.width,
                cfg.column_panel.height,
            );

            auto_fit_panel(&mut cfg, &PanelGeometry::only_vertical(vertical));

            assert_eq!(
                (
                    cfg.display.panel_font_size,
                    cfg.display.panel_min_width,
                    cfg.bar_panel.width,
                    cfg.column_panel.height,
                ),
                snap,
                "no glyph_adv: nothing should change for vertical={vertical}",
            );
        }

        // Vertical with glyph_adv but no usable_px: also no-op.
        let mut cfg = base_config(true);
        let before = (cfg.display.panel_min_width, cfg.bar_panel.width);
        auto_fit_panel(
            &mut cfg,
            &PanelGeometry {
                vertical: true,
                usable_px: None,
                glyph_adv: Some(6.59375),
                tooltip_adv: None,
            },
        );
        assert_eq!((cfg.display.panel_min_width, cfg.bar_panel.width), before);
    }

    // ── helpers reused by the auto_fit tests ────────────────────────────────

    // (No shared helpers today; the auto_fit tests construct their Config
    // inline. Kept as a placeholder so future shared fixtures have an
    // obvious home.)

    // ── deep_merge round-trip (smoke) ───────────────────────────────────────

    #[test]
    fn deep_merge_smoke_keeps_table_via_geometry_module_reexport() {
        // Just exercises the reexport path through `super::merge` to keep
        // the test mod independent of `merge::tests`.
        let base = toml! { [panel] items = ["a"] };
        let override_ = toml! { [panel] items_add = ["b"] };

        let merged = deep_merge_tables(base, override_);
        let panel = merged.get("panel").expect("merged panel must exist");
        assert!(panel.is_table());
    }
}
