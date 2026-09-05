#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default
)]

use super::*;
use crate::domain::registry::{misplaced_items, unknown_item_names};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::error::Error as StdError;
use toml::toml;

// ── apply_canonical_width ───────────────────────────────────────────────

#[test]
fn apply_canonical_width_sets_resolved_width() {
    let mut cfg = Config::default();

    apply_canonical_width(&mut cfg, TOOLTIP_WIDTH_FLOOR + 6);

    assert_eq!(cfg.display.tooltip_width, TOOLTIP_WIDTH_FLOOR + 6);
}

#[test]
fn apply_canonical_width_does_not_ratchet() {
    let mut cfg = Config::default();
    apply_canonical_width(&mut cfg, TOOLTIP_WIDTH_FLOOR + 12);
    apply_canonical_width(&mut cfg, TOOLTIP_WIDTH_FLOOR + 4);

    // Follows down, not stuck on the previous max.
    assert_eq!(cfg.display.tooltip_width, TOOLTIP_WIDTH_FLOOR + 4);
}

#[test]
fn apply_canonical_width_floors_at_builtin_minimum() {
    let mut cfg = Config::default();
    apply_canonical_width(&mut cfg, TOOLTIP_WIDTH_FLOOR - 10);

    assert_eq!(cfg.display.tooltip_width, TOOLTIP_WIDTH_FLOOR);
}

#[test]
fn apply_canonical_width_ignores_nonpositive() {
    let mut cfg = Config::default();
    cfg.display.tooltip_width = 42;
    apply_canonical_width(&mut cfg, 0);

    assert_eq!(cfg.display.tooltip_width, 42);
}

// ── typed defaults & serde round-trip ───────────────────────────────────

#[test]
fn display_defaults_match_python() {
    let d = DisplayConfig::default();
    assert_eq!(d.poll_interval.as_secs_f64(), 1.5);
    assert_eq!(d.history_interval.as_secs_f64(), 1.5);
    assert_eq!(d.language, "en");
    assert_eq!(d.top_process_name_max_len, 20);
    assert_eq!(d.panel_font_size, 13);
    assert_eq!(d.tooltip_width, TOOLTIP_WIDTH_FLOOR);
    assert_eq!(d.panel_min_width, 5);
    assert!(!d.overlay);
}

#[test]
fn display_serde_uses_struct_default_for_missing_fields() {
    // Round-trip a partial table through serde; with `#[serde(default)]`
    // at the container level, missing fields fall back to the struct's
    // own Default impl (NOT the field type's), matching Python's
    // `_from_dict`.
    let value = Value::Table(toml! {
        poll_interval = 3.0
    });

    let d: DisplayConfig = DisplayConfig::deserialize(value).unwrap();
    assert_eq!(
        d.poll_interval.as_secs_f64(),
        3.0,
        "provided value overrides"
    );
    assert_eq!(
        d.history_interval.as_secs_f64(),
        1.5,
        "missing field uses struct default"
    );
    assert_eq!(d.language, "en");
    assert_eq!(d.tooltip_width, TOOLTIP_WIDTH_FLOOR);
}

#[test]
fn thresholds_defaults_match_python() {
    let t = ThresholdConfig::default();
    assert_eq!(t.cpu_usage, vec![50, 70]);
    assert_eq!(t.mem_usage, vec![40, 60]);
    assert_eq!(t.hd_temp, vec![50, 55]);
    assert_eq!(t.battery_sys, vec![20, 80]);
    assert_eq!(t.gpu_nvidia_dec_usage, 1);
    assert_eq!(t.load_avg_1, vec![0.7, 1.0]);
    assert_eq!(t.load_avg_15, vec![0.5, 0.8]);
}

#[test]
fn notify_thresholds_defaults_match_python() {
    let n = NotifyThresholds::default();
    assert_eq!(n.disk_usage, 80);
    assert_eq!(n.cpu_temp, 80);
    assert_eq!(n.temp_sustain_seconds, 60);
    assert_eq!(n.temp_hysteresis, 5);
    assert_eq!(n.load_avg_15, 0.9);
    assert_eq!(n.load_avg_minutes, 10);
}

#[test]
fn notifications_defaults_match_python() {
    let n = NotificationConfig::default();
    assert!(n.disk_usage);
    assert!(!n.cpu_temp);
    assert!(!n.gpu_nvidia_temp);
    assert!(!n.server_check);
    assert!(!n.load_avg);
}

#[test]
fn disks_defaults_match_python() {
    let d = DiskConfig::default();
    assert_eq!(d.mounts, Mounts::Auto);
    assert_eq!(
        d.auto_roots,
        vec![
            "/mnt".to_owned(),
            "/media".to_owned(),
            "/run/media".to_owned()
        ],
    );
    assert!(d.smart);
    assert_eq!(d.smart_interval.as_secs_f64(), 3600.0);
    assert_eq!(d.smart_interval_hdd.as_secs_f64(), 21600.0);
}

#[test]
fn surface_glyphs_defaults_true() {
    assert!(Surface::default().glyphs);
}

#[test]
fn surface_has_and_item_set_empty() {
    let s = Surface::default();
    assert!(!s.has("anything"));
    assert!(s.item_set().is_empty());
}

// ── Mounts enum (list[str] | str) ───────────────────────────────────────

#[test]
fn mounts_deserializes_auto_string() {
    let v = Value::String(String::from("auto"));
    let m: Mounts = Mounts::deserialize(v).unwrap();
    assert_eq!(m, Mounts::Auto);
}

#[test]
fn mounts_deserializes_explicit_list() {
    let v = Value::Array(vec![
        Value::String(String::from("/")),
        Value::String(String::from("/mnt/data")),
    ]);
    let m: Mounts = Mounts::deserialize(v).unwrap();
    assert_eq!(
        m,
        Mounts::Explicit(vec![String::from("/"), String::from("/mnt/data")]),
    );
}

#[test]
fn mounts_deserializes_single_string_as_one_element_list() {
    let v = Value::String(String::from("/"));
    let m: Mounts = Mounts::deserialize(v).unwrap();
    assert_eq!(m, Mounts::Explicit(vec![String::from("/")]));
}

// ── typed_section ───────────────────────────────────────────────────────

#[test]
fn typed_section_falls_back_to_default_when_missing() {
    let empty = Table::new();
    let d: DisplayConfig = typed_section(&empty, "display").unwrap();
    assert_eq!(d, DisplayConfig::default());
}

#[test]
fn typed_section_overrides_known_keys_only() {
    let raw = toml! {
        [display]
        poll_interval = 9.0
        language = "it"
        unknown_key = "ignored"
    };

    let d: DisplayConfig = typed_section(&raw, "display").unwrap();
    assert_eq!(d.poll_interval.as_secs_f64(), 9.0);
    assert_eq!(d.language, "it");
    assert_eq!(
        d.history_interval.as_secs_f64(),
        1.5,
        "missing field falls back to default"
    );
}

#[test]
fn typed_section_returns_err_on_wrong_type() {
    let raw = toml! {
        display = "not a table"
    };

    let result: Result<DisplayConfig, ConfigError> = typed_section(&raw, "display");
    assert!(
        result.is_err(),
        "scalar where a table is expected must fail"
    );
}

// ── drop_unknown_items / drop_misplaced_items ───────────────────────────

fn str_set(items: &[&str]) -> BTreeSet<String> {
    items.iter().copied().map(str::to_owned).collect()
}

#[test]
fn drop_unknown_items_removes_typos_only() {
    let mut cfg = Config::default();
    cfg.tooltip.sections.push(Section {
        key: String::from("live"),
        title: String::new(),
        items: vec![
            String::from("cpu_usage"),
            String::from("cpu_usage:bogus_form"),
            String::from("totally_bogus"),
        ],
    });

    drop_unknown_items(&mut cfg);

    assert_eq!(
        cfg.tooltip.sections[0].items,
        vec![String::from("cpu_usage")],
    );
}

#[test]
fn drop_unknown_items_spares_separators() {
    let mut cfg = Config::default();
    cfg.tooltip.sections.push(Section {
        key: String::from("live"),
        title: String::new(),
        items: vec![
            String::from("cpu_usage"),
            String::from("separator_small"),
            String::from("separator_big"),
            String::from("nope"),
        ],
    });

    drop_unknown_items(&mut cfg);

    assert_eq!(
        cfg.tooltip.sections[0].items,
        vec![
            String::from("cpu_usage"),
            String::from("separator_small"),
            String::from("separator_big"),
        ],
    );
}

#[test]
fn drop_misplaced_items_removes_panel_only_from_tooltip() {
    let mut cfg = Config::default();
    cfg.tooltip.sections.push(Section {
        key: String::from("live"),
        title: String::new(),
        items: vec![
            String::from("cpu_usage:spark"),
            String::from("cpu_usage"),
            String::from("mem_usage:bar"),
        ],
    });

    drop_misplaced_items(&mut cfg);

    assert_eq!(
        cfg.tooltip.sections[0].items,
        vec![String::from("cpu_usage")],
    );
}

#[test]
fn drop_misplaced_items_removes_tooltip_only_from_panel() {
    let mut cfg = Config::default();
    cfg.panel.sections.push(Section {
        key: String::from("live"),
        title: String::new(),
        items: vec![
            String::from("uptime"),
            String::from("cpu_usage"),
            String::from("net_speed"),
        ],
    });

    drop_misplaced_items(&mut cfg);

    assert_eq!(
        cfg.panel.sections[0].items,
        vec![String::from("cpu_usage"), String::from("net_speed")],
    );
}

#[test]
fn drop_misplaced_items_leaves_a_section_empty_rather_than_absent() {
    let mut cfg = Config::default();
    cfg.tooltip.sections.push(Section {
        key: String::from("live"),
        title: String::new(),
        items: vec![String::from("cpu_usage:spark")],
    });

    drop_misplaced_items(&mut cfg);

    // The section is preserved but empty: the render collapses it.
    assert_eq!(cfg.tooltip.sections.len(), 1);
    assert!(cfg.tooltip.sections[0].items.is_empty());
}

// ── unknown_item_names / misplaced_items smoke (registry re-export) ─────

#[test]
fn unknown_item_names_flags_only_unknowns() {
    assert_eq!(
        unknown_item_names(["cpu_usage", "disk_usage", "bogus_item"]),
        str_set(&["bogus_item"]),
    );
    assert!(unknown_item_names(["cpu_usage", "hd_temp"]).is_empty());
}

#[test]
fn misplaced_items_filters_panel_only_out_of_tooltip() {
    let (bad_panel, bad_tooltip) = misplaced_items(
        ["cpu_usage", "cpu_usage:spark_value", "top_process"],
        ["cpu_usage", "net_device_ip"],
    );
    assert_eq!(
        bad_panel,
        str_set(&["cpu_usage:spark_value", "top_process"]),
    );
    assert!(bad_tooltip.is_empty());
}

// ── load_config ─────────────────────────────────────────────────────────

fn write_config(dir: &Path, contents: &str) -> PathBuf {
    let path = dir.join("config.toml");
    std::fs::write(&path, contents).unwrap();
    path
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("plasma-top-config-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn load_config_missing_path_returns_no_machine() {
    let dir = temp_dir("missing");
    let path = dir.join("does-not-exist.toml");

    let cfg = load_config(Some(&path), Some(false)).unwrap();

    assert_eq!(cfg.machine, "");
    assert!(cfg.panel.sections.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_config_section_schema() {
    let dir = temp_dir("schema");
    let toml_text = r#"
[tooltip]
order = ["live", "load"]
[tooltip.live]
title = "Live"
items = ["cpu_usage:spark_value", "mem_usage:spark_value"]
[tooltip.load]
title = "Load"
items = ["uptime", "load_avg"]
"#;
    let path = write_config(&dir, toml_text);

    let cfg = load_config(Some(&path), Some(false)).unwrap();

    let keys: Vec<&str> = cfg
        .tooltip
        .sections
        .iter()
        .map(|s| s.key.as_str())
        .collect();
    assert_eq!(keys, ["live", "load"]);
    assert!(cfg.tooltip.has("uptime"));
    assert_eq!(cfg.tooltip.sections[0].title, "Live");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_config_rejects_invalid_configured_cadences() {
    let cases = [
        "[display]\npoll_interval = 0.099\n",
        "[display]\nhistory_interval = nan\n",
        "[disks]\nsmart_interval = 0.0\n",
        "[disks]\nsmart_interval_hdd = inf\n",
    ];

    for (index, contents) in cases.into_iter().enumerate() {
        let dir = temp_dir(&format!("invalid-cadence-{index}"));
        let path = write_config(&dir, contents);
        let error = load_config(Some(&path), Some(false)).expect_err("invalid cadence");

        assert!(error.to_string().contains("cadence"));
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[test]
fn load_config_drops_unknown_and_misplaced() {
    let dir = temp_dir("drop");
    let toml_text = r#"
[tooltip]
order = ['live']
[tooltip.live]
items = ['cpu_usage', 'cpu_usage:bogus_form', 'totally_bogus']
"#;
    let path = write_config(&dir, toml_text);

    let cfg = load_config(Some(&path), Some(false)).unwrap();

    assert_eq!(
        cfg.tooltip.sections[0].items,
        vec![String::from("cpu_usage")],
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_config_machine_items_add() {
    // The matching machine block (from the sibling machines.toml) merges
    // its items_add over the base section. Force the match via the
    // `_with_dmi` seam — detection itself is covered by the
    // detect_machine tests in geometry.
    let dir = temp_dir("machine-add");
    std::fs::write(
        dir.join("config.toml"),
        r#"
[tooltip]
order = ["live"]
[tooltip.live]
items = ["cpu_usage:spark_value"]
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("machines.toml"),
        r#"
[desktop.detect]
board_contains = "ExampleBoard"
[desktop.tooltip.live]
items_add = ["fan_speed"]
"#,
    )
    .unwrap();
    let path = dir.join("config.toml");

    let cfg = load_config_with_dmi(
        Some(&path),
        Some(false),
        "ACME ExampleBoard v2",
        "Example Product",
    )
    .unwrap();

    assert_eq!(cfg.machine, "desktop");
    assert_eq!(
        cfg.tooltip.sections[0].items,
        vec![
            String::from("cpu_usage:spark_value"),
            String::from("fan_speed"),
        ],
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_config_machine_order_add_new_section() {
    let dir = temp_dir("machine-order");
    std::fs::write(
        dir.join("config.toml"),
        r#"
[panel]
order = ["live"]
[panel.live]
items = ["cpu_usage"]
"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("machines.toml"),
        r#"
[mymachine.detect]
product_contains = "ExampleVM"
[mymachine.panel]
order_add = ["drives"]
[mymachine.panel.drives]
items = ["disk_usage"]
"#,
    )
    .unwrap();
    let path = dir.join("config.toml");

    let cfg =
        load_config_with_dmi(Some(&path), Some(false), "Generic Board", "ExampleVM 7").unwrap();

    assert_eq!(cfg.machine, "mymachine");
    let keys: Vec<&str> = cfg.panel.sections.iter().map(|s| s.key.as_str()).collect();
    assert_eq!(keys, ["live", "drives"]);
    assert!(cfg.panel.has("disk_usage"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_config_orientation_override_horizontal() {
    let dir = temp_dir("orient-h");
    let toml_text = r#"
[panel]
order = ["cpumem"]
[panel.cpumem]
items = ["cpu_usage", "mem_usage"]
[panel_horizontal.cpumem]
items = ["cpu_usage", "cpu_usage:spark", "mem_usage", "mem_usage:spark"]
[panel_vertical.cpumem]
items = ["cpu_usage", "cpu_usage:bar", "mem_usage", "mem_usage:bar"]
"#;
    let path = write_config(&dir, toml_text);

    let cfg = load_config(Some(&path), Some(false)).unwrap();

    assert!(cfg.panel.has("cpu_usage:spark"));
    assert!(cfg.panel.has("mem_usage:spark"));
    assert!(!cfg.panel.has("cpu_usage:bar"));
    assert!(!cfg.vertical);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_config_orientation_override_vertical() {
    let dir = temp_dir("orient-v");
    let toml_text = r#"
[panel]
order = ["cpumem"]
[panel.cpumem]
items = ["cpu_usage", "mem_usage"]
[panel_horizontal.cpumem]
items = ["cpu_usage", "cpu_usage:spark", "mem_usage", "mem_usage:spark"]
[panel_vertical.cpumem]
items = ["cpu_usage", "cpu_usage:bar", "mem_usage", "mem_usage:bar"]
"#;
    let path = write_config(&dir, toml_text);

    let cfg = load_config(Some(&path), Some(true)).unwrap();

    assert!(cfg.panel.has("cpu_usage:bar"));
    assert!(cfg.panel.has("mem_usage:bar"));
    assert!(!cfg.panel.has("cpu_usage:spark"));
    assert!(cfg.vertical);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_config_column_panel_width_loads() {
    let dir = temp_dir("colwidth");
    let path = write_config(&dir, "[column_panel]\nwidth = 3\n");

    let cfg = load_config(Some(&path), Some(false)).unwrap();

    assert_eq!(cfg.column_panel.width, 3);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_config_warns_on_unknown_item() {
    let dir = temp_dir("warn");
    let path = write_config(
        &dir,
        "[panel]\norder = ['main']\n\n[panel.main]\nitems = ['cpu_usage', 'totally_not_an_item']\n",
    );

    // The warning goes to stderr; we can't easily intercept it here, but
    // the load still succeeds and the typo is dropped (verified above).
    let cfg = load_config(Some(&path), Some(false)).unwrap();

    assert_eq!(cfg.panel.sections[0].items, vec![String::from("cpu_usage")],);
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Cross-check against the Python oracle's "default config has no
//     unknown items" guarantee. Skipped when the shipped config is not
//     reachable (CI without the full repo); otherwise asserts every
//     item the default ships resolves to a valid token.
#[test]
fn default_config_has_no_unknown_items() {
    let shipped = assets::shipped_config();
    if !shipped.is_file() {
        return;
    }
    let cfg = load_config(None, Some(false)).unwrap();
    let configured: BTreeSet<String> = cfg
        .panel
        .item_set()
        .iter()
        .cloned()
        .chain(cfg.tooltip.item_set())
        .collect();
    assert!(
        unknown_item_names(configured.iter().map(String::as_str)).is_empty(),
        "every item in the shipped config.toml must resolve to a valid token",
    );
}

#[test]
fn config_error_displays_with_source() {
    let err = ConfigError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "missing"));
    assert!(format!("{err}").contains("config I/O failure"));
    assert!(StdError::source(&err).is_some());

    let parse_err = toml::from_str::<Table>("bad =").unwrap_err();
    let err = ConfigError::Toml(parse_err);
    assert!(format!("{err}").contains("config parse failure"));
}

#[test]
fn amd_defaults_preserve_old_config_and_request_tooltip_items() {
    let dir = temp_dir("amd-old-config");
    let path = dir.join("config.toml");
    std::fs::write(
        &path,
        "[notifications]\ngpu_nvidia_temp = true\n[notify_thresholds]\ngpu_nvidia_temp = 90\n",
    )
    .unwrap();
    let cfg = load_config(Some(&path), Some(false)).unwrap();
    assert!(cfg.notifications.gpu_nvidia_temp);
    assert!(!cfg.notifications.gpu_amd_temp);
    assert_eq!(cfg.notify_thresholds.gpu_nvidia_temp, 90);
    assert_eq!(cfg.notify_thresholds.gpu_amd_temp, 80);
    assert_eq!(cfg.thresholds.gpu_amd_usage, [50, 70]);
    assert_eq!(cfg.thresholds.gpu_amd_mem_usage, [50, 70]);
    assert_eq!(cfg.thresholds.gpu_amd_temp, [50, 70]);
    assert_eq!(cfg.thresholds.gpu_amd_codec_usage, 1);
    assert!(toml::from_str::<ThresholdConfig>("gpu_amd_usage = [50]").is_err());
    let shipped = load_config(
        Some(&Path::new(env!("CARGO_MANIFEST_DIR")).join("config/config.toml")),
        Some(false),
    )
    .unwrap();
    let tooltip: Vec<_> = shipped
        .tooltip
        .sections
        .iter()
        .flat_map(|section| section.items.iter().map(String::as_str))
        .collect();
    assert!(unknown_item_names(tooltip.iter().copied()).is_empty());
    for metric in crate::domain::Metric::all()
        .iter()
        .filter(|metric| metric.as_str().starts_with("gpu_amd_"))
    {
        assert!(tooltip.contains(&metric.as_str()));
        assert!(
            !shipped
                .panel
                .sections
                .iter()
                .any(|section| section.items.iter().any(|item| item == metric.as_str()))
        );
    }
}
