//! Integration test: load the shipped `config/config.toml` end-to-end and
//! assert typed fields match the Python oracle.
//!
//! Forces `vertical = Some(false)` so the orientation override is
//! deterministic and does not depend on the host's Plasma state. The
//! shipped `config.toml` lives under the repo's `config/` directory; this
//! test resolves it via [`pirostats::config::load_config`] with `path =
//! None`, exercising the same default-resolution path the daemon uses.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use pirostats::config::{DiskConfig, Mounts, load_config};
use pirostats::domain::registry::unknown_item_names;

#[test]
fn loads_shipped_config_with_forced_horizontal() {
    // `path = None` resolves to the shipped `config/config.toml` (the dev
    // checkout). Force horizontal so the orientation override is
    // deterministic and independent of any Plasma state on the test host.
    let cfg = match load_config(None, Some(false)) {
        Ok(cfg) => cfg,
        Err(error) => panic!("loading the shipped config failed: {error}"),
    };

    // The shipped `machines.toml` has no `[<name>.detect]` rule that could
    // match a CI host; if it ever did, this test would catch the regression.
    assert!(
        cfg.machine.is_empty(),
        "no machine block should match the test host, got `{}`",
        cfg.machine,
    );
    assert!(
        !cfg.vertical,
        "forced horizontal override must land on Config.vertical",
    );

    // ── pages.order (graphs first, then processes) ────────────────────────
    assert_eq!(
        cfg.pages.order,
        vec![String::from("graphs"), String::from("processes")],
    );
    assert_eq!(cfg.pages.graph_history_length, 60);

    // ── thresholds vectors ────────────────────────────────────────────────
    assert_eq!(cfg.thresholds.cpu_usage, vec![50, 70]);
    assert_eq!(cfg.thresholds.mem_usage, vec![40, 60]);
    assert_eq!(cfg.thresholds.hd_temp, vec![50, 55]);
    assert_eq!(cfg.thresholds.battery_sys, vec![20, 80]);
    assert_eq!(cfg.thresholds.load_avg_1, vec![0.7, 1.0]);
    assert_eq!(cfg.thresholds.gpu_nvidia_dec_usage, 1);

    // ── disks: mounts = Auto, auto_roots = the shipped list ───────────────
    assert_eq!(cfg.disks.mounts, Mounts::Auto);
    assert_eq!(
        cfg.disks.auto_roots,
        vec![
            String::from("/mnt"),
            String::from("/media"),
            String::from("/run/media"),
        ],
    );
    assert_eq!(cfg.disks.smart_interval, 3600.0);
    assert_eq!(cfg.disks.smart_interval_hdd, 21600.0);
    let _: &DiskConfig = &cfg.disks; // type-check the field shape

    // ── horizontal override: glyphs off, battery_sys removed ──────────────
    assert!(
        !cfg.panel.glyphs,
        "horizontal override must disable glyphs (space is tightest)",
    );
    assert!(
        !cfg.panel.has("battery_sys"),
        "horizontal override must remove battery_sys",
    );

    // ── shipped glyphs/labels loaded as flat tables ──────────────────────
    assert!(
        cfg.icons.contains_key("cpu_usage"),
        "icons.toml must populate the cpu_usage glyph",
    );
    assert_eq!(
        cfg.labels.get("delimiter"),
        Some(&toml::Value::String(String::from(":"))),
    );
    assert!(
        cfg.labels.contains_key("cpu_usage"),
        "lang/en.toml must populate the cpu_usage label",
    );
}

#[test]
fn shipped_config_has_no_unknown_or_misplaced_items() {
    let cfg = match load_config(None, Some(false)) {
        Ok(cfg) => cfg,
        Err(error) => panic!("loading the shipped config failed: {error}"),
    };

    let configured: BTreeSet<String> = cfg
        .panel
        .item_set()
        .into_iter()
        .chain(cfg.tooltip.item_set())
        .collect();

    let unknowns = unknown_item_names(configured.iter().map(String::as_str));
    assert!(
        unknowns.is_empty(),
        "shipped config lists unknown items: {unknowns:?}",
    );
}

#[test]
fn shipped_machines_template_has_no_detect_rules() {
    // The shipped `machines.toml` is a how-to template with no top-level
    // machine block. Even when a test host happens to expose a DMI, the
    // load should report `machine == ""`.
    let cfg = match load_config(None, Some(false)) {
        Ok(cfg) => cfg,
        Err(error) => panic!("loading the shipped config failed: {error}"),
    };
    assert_eq!(cfg.machine, "");
}
