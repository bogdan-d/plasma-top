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

// ── read_geom_file_at + cache_live_geom_at ──────────────────────────────

fn write_tmp(name: &str, contents: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("plasma-top-geom-{name}-{}", std::process::id()));
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
