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
    let text =
        "[Containments][2]\nlocation=5\nplugin=panel\n[Containments][2][Applets][7]\nplugin=foo\n";

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

// ── geometry cache ──────────────────────────────────────────────────────

fn write_tmp(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("plasma-top-geom-{name}-{}", std::process::id()));
    std::fs::write(&path, contents).unwrap();
    path
}

#[test]
fn geometry_cache_round_trips_and_ignores_damage() {
    let parent =
        std::env::temp_dir().join(format!("plasma-top-geom-cache-sub-{}", std::process::id()));
    let cache = parent.join("geom");
    let measured = PanelGeometry::from(AppletGeometry::new(100.0, 5.5, true, 7.25).unwrap());

    write_geom_cache(&cache, &measured);

    assert_eq!(read_geom_cache(&cache), Some(measured));
    std::fs::write(&cache, "0 0 1\n").unwrap();
    assert_eq!(read_geom_cache(&cache), None, "degenerate cache");
    let _ = std::fs::remove_dir_all(&parent);
    assert_eq!(read_geom_cache(&cache), None, "absent cache");
}

// ── detect_panel_geometry_at ────────────────────────────────────────────

fn appletsrc(location: u32) -> String {
    format!(
        "[Containments][2]\nlocation={location}\n\
         [Containments][2][Applets][25]\nplugin=com.github.bogdan-d.plasma-top\n"
    )
}

#[test]
fn detect_panel_geometry_uses_matching_measurement() {
    let appletsrc_path = write_tmp("appletsrc-v", &appletsrc(5));

    let geo = detect_panel_geometry_at(&appletsrc_path, parse_geom("42 6.59375 1\n"));

    assert!(geo.vertical);
    assert_eq!(geo.usable_px, Some(42.0));
    assert_eq!(geo.glyph_adv, Some(6.59375));
    let unmeasured = detect_panel_geometry_at(&appletsrc_path, None);
    assert!(unmeasured.vertical);
    assert!(unmeasured.usable_px.is_none());
    let _ = std::fs::remove_file(&appletsrc_path);
}

#[test]
fn detect_panel_geometry_drops_stale_edge_but_keeps_tooltip_advance() {
    // Vertical panel (location=5) while the measurement still reports the old horizontal edge: the orientation stays appletsrc's and the wrong-axis numbers are ignored, but the orientation-independent tooltip advance survives.
    let appletsrc_path = write_tmp("appletsrc-stale", &appletsrc(5));

    let geo = detect_panel_geometry_at(&appletsrc_path, parse_geom("42 6.59375 0 8.0\n"));

    assert!(geo.vertical);
    assert!(geo.usable_px.is_none());
    assert!(geo.glyph_adv.is_none());
    assert_eq!(geo.tooltip_adv, Some(8.0));
    let _ = std::fs::remove_file(&appletsrc_path);
}

#[test]
fn detect_panel_geometry_defaults_when_unreadable() {
    let appletsrc_absent = std::env::temp_dir().join(format!(
        "plasma-top-appletsrc-absent-{}",
        std::process::id()
    ));

    let geo = detect_panel_geometry_at(&appletsrc_absent, None);

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
