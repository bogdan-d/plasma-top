//! Token, separator, and capability boundary — the Rust mirror of the token
//! layer in `src/registry.py`.
//!
//! Render dispatch belongs to `crate::render::registry`; this module owns the
//! [`parse`] boundary that formatter/config/sensors consume, the
//! [`unknown_item_names`] / [`misplaced_items`] validators, and the
//! [`needed_capabilities`] derivation that drives the collector's call set.

use std::collections::BTreeSet;
use std::str::FromStr;

use crate::domain::form::{Form, Surface, SurfaceSet};
use crate::domain::item::ItemToken;
use crate::domain::metric::{Capability, Metric};

/// TOML entries that introduce a visual separator instead of an item.
///
/// Mirrors the keys of `SEPARATOR_ITEMS` in `src/render_model.py`. They are
/// valid section entries but never resolve to a metric, so [`parse`] returns
/// `None` for them (and [`unknown_item_names`] / [`misplaced_items`] skip
/// them).
pub const SEPARATOR_ITEMS: &[&str] = &["separator_small", "separator_big"];

/// Notification flag name → capability pulled by an enabled notification.
///
/// Mirrors `_NOTIFY_CAPS` in `src/registry.py`: a notification keeps the
/// matching sensor alive even when no item renders it. The flag names are the
/// `NotificationConfig` field keys; the capabilities are the matching
/// [`Metric::capabilities`] tokens.
pub const NOTIFY_CAPABILITY_MAP: &[(&str, Capability)] = &[
    ("cpu_temp", Capability::CpuTemperature),
    ("gpu_nvidia_temp", Capability::GpuNvidia),
    ("disk_usage", Capability::DiskUsage),
    ("disk_smart", Capability::DiskSmart),
    ("hd_temp", Capability::DiskTemperature),
    ("battery_sys", Capability::BatterySystem),
    ("battery_mouse", Capability::BatteryMouse),
    ("battery_kbd", Capability::BatteryKeyboard),
    ("load_avg", Capability::LoadAverage),
    ("server_check", Capability::ServerCheck),
];

/// Capabilities added when the `graphs` page is enabled.
///
/// Mirrors the special case in `registry.needed_capabilities` in
/// `src/registry.py`: the graphs page charts NVIDIA/Intel GPU and network
/// history even when no such item lives on a surface, so its capabilities are
/// requested unconditionally. The hardware gate in the collector narrows this
/// to the GPU/interface actually present.
pub const GRAPHS_PAGE_CAPABILITIES: &[Capability] = &[
    Capability::GpuNvidia,
    Capability::GpuIntelUsage,
    Capability::GpuIntelDecoder,
    Capability::NetworkSpeed,
];

/// Returns `true` when `token` is a reserved separator entry.
fn is_separator(token: &str) -> bool {
    SEPARATOR_ITEMS.contains(&token)
}

/// Resolves a `"metric[:form]"` token to its metric and optional generic form.
///
/// Mirrors `registry.parse` in `src/registry.py`. Returns `None` for unknown
/// metrics, unsupported forms, form-on-intrinsic-metric, and separators — the
/// same set of cases the Python layer treats as "not a valid item".
///
/// Intrinsic metrics (e.g. `net_speed`, `top_process`) yield `(metric, None)`
/// because they carry their own shape and never take a generic form. Bare
/// generic metrics yield `(metric, Some(Form::Value))` because VALUE is the
/// implicit default.
#[must_use]
pub fn parse(token: &str) -> Option<(Metric, Option<Form>)> {
    let item = ItemToken::from_str(token).ok()?;
    Some((item.metric(), item.form()))
}

/// Returns the tokens in `names` that fail to resolve to a valid item.
///
/// Separators are valid section entries and never flagged. Mirrors
/// `registry.unknown_item_names` in `src/registry.py`: a typo in the toml
/// shows up here instead of vanishing silently. A form on an intrinsic metric
/// (`net_speed:value`) or an unsupported form (`cpu_temp:bar`) also counts as
/// unknown because [`parse`] rejects them.
pub fn unknown_item_names<'a>(names: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
    names
        .into_iter()
        .filter(|name| !is_separator(name) && parse(name).is_none())
        .map(str::to_owned)
        .collect()
}

/// Returns the tokens placed on a surface their effective surfaces don't admit.
///
/// Mirrors `registry.misplaced_items` in `src/registry.py`. Returns a pair of
/// sets: the misplaced panel entries and the misplaced tooltip entries. Panel
/// membership means EITHER panel orientation admits the token (matching
/// `Surface.PANEL = PANEL_H | PANEL_V` in Python). Unknown tokens are ignored
/// here — use [`unknown_item_names`] to flag those separately.
pub fn misplaced_items<'a>(
    panel_names: impl IntoIterator<Item = &'a str>,
    tooltip_names: impl IntoIterator<Item = &'a str>,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let bad_panel = misplaced_on(panel_names, SurfaceSet::PANEL);
    let bad_tooltip = misplaced_on(tooltip_names, SurfaceSet::TOOLTIP);
    (bad_panel, bad_tooltip)
}

/// Computes the misplaced entries against a single target surface set.
fn misplaced_on<'a>(
    names: impl IntoIterator<Item = &'a str>,
    target: SurfaceSet,
) -> BTreeSet<String> {
    names
        .into_iter()
        .filter_map(|name| {
            if is_separator(name) {
                return None;
            }
            let item = ItemToken::from_str(name).ok()?;
            let effective = item.effective_surfaces();
            if effective.intersection(target).is_empty() {
                Some(name.to_owned())
            } else {
                None
            }
        })
        .collect()
}

/// Returns the notification-flag → capability map.
///
/// Convenience accessor for the static [`NOTIFY_CAPABILITY_MAP`] table; the
/// return is `&'static` so callers can iterate it without owning a copy.
#[must_use]
pub fn notification_capability_map() -> &'static [(&'static str, Capability)] {
    NOTIFY_CAPABILITY_MAP
}

/// Returns the capability set requested by the `graphs` page.
///
/// Convenience accessor for the static [`GRAPHS_PAGE_CAPABILITIES`] table.
#[must_use]
pub fn graphs_page_capabilities() -> &'static [Capability] {
    GRAPHS_PAGE_CAPABILITIES
}

/// Computes the sensor capabilities to read this poll.
///
/// Mirrors `registry.needed_capabilities` in `src/registry.py`: the union of
/// (a) each item's metric capabilities, (b) capabilities pulled by enabled
/// notification flags via [`notification_capability_map`], and (c) the
/// [`graphs_page_capabilities`] set when `pages_order` contains `"graphs"`.
/// Callers are responsible for parsing token strings into [`ItemToken`]s and
/// filtering unknowns (Python folds that into a single step here we keep the
/// boundary explicit so the function does not depend on CONFIG types).
pub fn needed_capabilities<'a>(
    items: impl Iterator<Item = ItemToken>,
    notify_flags: impl Iterator<Item = &'a str>,
    mut pages_order: impl Iterator<Item = &'a str>,
) -> BTreeSet<Capability> {
    let mut caps = BTreeSet::new();
    for item in items {
        caps.extend(item.metric().capabilities().iter().copied());
    }
    let notify_map = notification_capability_map();
    for flag in notify_flags {
        for (key, cap) in notify_map {
            if *key == flag {
                caps.insert(*cap);
            }
        }
    }
    if pages_order.any(|page| page == "graphs") {
        caps.extend(graphs_page_capabilities().iter().copied());
    }
    caps
}

/// Returns the human-readable placement string for an effective surface set.
///
/// Mirrors the `where` closure inside `run_list_items` in `src/daemon.py`:
/// `"panel + tooltip"` when on either panel orientation and the tooltip,
/// `"panel only"` / `"tooltip only"` for single-surface items, and `"-"` for
/// the (currently unreachable) empty case. Used by [`list_items`] and exposed
/// for the DAEMON-CLI `list-items` command.
#[must_use]
pub fn placement_for(effective: SurfaceSet) -> &'static str {
    let on_panel = !effective.intersection(SurfaceSet::PANEL).is_empty();
    let on_tooltip = effective.contains(Surface::Tooltip);
    match (on_panel, on_tooltip) {
        (true, true) => "panel + tooltip",
        (true, false) => "panel only",
        (false, true) => "tooltip only",
        (false, false) => "-",
    }
}

/// Enumerates every valid `(token, placement)` pair in deterministic order.
///
/// Mirrors `run_list_items` in `src/daemon.py`: iterate each metric's valid
/// forms (intrinsic shape or declared generic forms), build the token string,
/// compute placement from [`ItemToken::effective_surfaces`], then sort by
/// `(placement, token)`. The result matches the Python `plasma-top list-items`
/// output row-for-row.
#[must_use]
pub fn list_items() -> Vec<(String, &'static str)> {
    let mut rows: Vec<(String, &'static str)> = Vec::new();
    for metric in Metric::all() {
        let spec = metric.spec();
        if spec.intrinsic_shape.is_some() {
            let token = metric.to_string();
            let placement = placement_for(metric.surfaces());
            rows.push((token, placement));
            continue;
        }
        for form in spec.generic_forms {
            let token_str = match form {
                Form::Value => metric.to_string(),
                _ => format!("{metric}:{form}"),
            };
            let item = match ItemToken::from_str(&token_str) {
                Ok(item) => item,
                Err(_) => continue,
            };
            let placement = placement_for(item.effective_surfaces());
            rows.push((token_str, placement));
        }
    }
    rows.sort_by(|(a_token, a_place), (b_token, b_place)| {
        a_place.cmp(b_place).then_with(|| a_token.cmp(b_token))
    });
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::form::Surface;
    use crate::domain::item::ItemRendering;

    /// Parses a token, panicking on failure (test-only invariant).
    fn must_parse(token: &str) -> ItemToken {
        let Ok(item) = ItemToken::from_str(token) else {
            panic!("test token `{token}` should parse")
        };
        item
    }

    /// Builds the capability set for a token list + notification flag list,
    /// matching the shape of Python's `needed_capabilities(cfg)` helper.
    fn caps(panel_tooltip_tokens: &[&str], notify_on: &[&str]) -> BTreeSet<Capability> {
        let items = panel_tooltip_tokens
            .iter()
            .filter_map(|token| ItemToken::from_str(token).ok());
        needed_capabilities(items, notify_on.iter().copied(), std::iter::empty())
    }

    /// Builds a `BTreeSet<String>` from string literals (test-only convenience).
    fn str_set(tokens: &[&str]) -> BTreeSet<String> {
        tokens.iter().copied().map(str::to_owned).collect()
    }

    // ── needed_capabilities (collection gating) ──────────────────────────────

    #[test]
    fn cpu_usage_needs_no_dedicated_sensor() {
        // cpu_usage/mem_usage live on always-collected readings → no capability
        assert!(caps(&["cpu_usage", "mem_usage"], &[]).is_empty());
    }

    #[test]
    fn item_pulls_its_capability() {
        assert_eq!(caps(&["disk_usage"], &[]), {
            let mut s = BTreeSet::new();
            s.insert(Capability::DiskUsage);
            s
        });
        assert_eq!(caps(&["fan_speed"], &[]), {
            let mut s = BTreeSet::new();
            s.insert(Capability::FanSpeed);
            s
        });
    }

    #[test]
    fn metric_can_need_multiple_capabilities() {
        // cpu_freq shows turbo intrinsically → also needs cpu_turbo
        let mut expected = BTreeSet::new();
        expected.insert(Capability::CpuFrequency);
        expected.insert(Capability::CpuTurbo);
        assert_eq!(caps(&["cpu_freq"], &[]), expected);
    }

    #[test]
    fn form_does_not_change_the_capability() {
        // the form doesn't affect the data: cpu_usage:spark_value weighs the same as cpu_usage
        assert!(caps(&["cpu_usage:spark_value"], &[]).is_empty());
        assert_eq!(caps(&["hd_temp:pair"], &[]), {
            let mut s = BTreeSet::new();
            s.insert(Capability::DiskTemperature);
            s
        });
    }

    #[test]
    fn notification_keeps_sensor_alive_without_the_item() {
        // no item, but the cpu_temp notification is on → the sensor is still read
        assert_eq!(caps(&[], &["cpu_temp"]), {
            let mut s = BTreeSet::new();
            s.insert(Capability::CpuTemperature);
            s
        });
        assert!(caps(&[], &["disk_usage"]).contains(&Capability::DiskUsage));
    }

    #[test]
    fn unknown_token_contributes_nothing() {
        // unknown tokens fail to parse before reaching needed_capabilities,
        // so feeding them through `caps` (which filters parse failures) yields ∅
        assert!(caps(&["totally_bogus"], &[]).is_empty());
    }

    #[test]
    fn gpu_nvidia_metrics_share_one_capability() {
        let caps_set = caps(&["gpu_nvidia_temp", "gpu_nvidia_usage"], &[]);
        assert_eq!(caps_set, {
            let mut s = BTreeSet::new();
            s.insert(Capability::GpuNvidia);
            s
        });
    }

    #[test]
    fn graphs_page_adds_gpu_and_network_capabilities() {
        let items = std::iter::empty::<ItemToken>();
        let result = needed_capabilities(items, std::iter::empty(), ["graphs"].into_iter());
        let expected: BTreeSet<Capability> = GRAPHS_PAGE_CAPABILITIES.iter().copied().collect();
        assert_eq!(result, expected);
    }

    #[test]
    fn graphs_page_caps_absent_without_graphs_in_order() {
        let items = std::iter::empty::<ItemToken>();
        let result = needed_capabilities(items, std::iter::empty(), ["processes"].into_iter());
        assert!(result.is_empty());
    }

    // ── canonical tokens ─────────────────────────────────────────────────────

    #[test]
    fn unknown_item_names_flags_bad_metric_and_bad_form() {
        assert_eq!(
            unknown_item_names(["cpu_usage", "nope"]),
            str_set(&["nope"])
        );
        assert_eq!(
            unknown_item_names(["cpu_usage:bar", "cpu_usage:nope"]),
            str_set(&["cpu_usage:nope"])
        );
        // separators are valid
        assert!(unknown_item_names(["separator_small"]).is_empty());
        // form on an intrinsic metric counts as unknown
        assert_eq!(
            unknown_item_names(["net_speed:value"]),
            str_set(&["net_speed:value"])
        );
    }

    #[test]
    fn parse_round_trips_known_tokens() {
        assert_eq!(
            parse("cpu_usage"),
            Some((Metric::CpuUsage, Some(Form::Value)))
        );
        assert_eq!(
            parse("cpu_usage:spark_value"),
            Some((Metric::CpuUsage, Some(Form::SparkValue)))
        );
        assert_eq!(parse("net_speed"), Some((Metric::NetSpeed, None)));
    }

    #[test]
    fn parse_rejects_invalid_and_separator_tokens() {
        assert!(parse("nope").is_none());
        assert!(parse("cpu_usage:nope").is_none());
        assert!(parse("cpu_temp:bar").is_none()); // unsupported form
        assert!(parse("net_speed:value").is_none()); // form on intrinsic
        assert!(parse("separator_small").is_none()); // separators are not items
        assert!(parse("separator_big").is_none());
    }

    // ── DERIVED placement (form ∩ metric) ────────────────────────────────────

    /// Mirrors `_where` in `tests/test_items.py`: the token's effective surfaces.
    fn where_surfaces(token: &str) -> SurfaceSet {
        must_parse(token).effective_surfaces()
    }

    #[test]
    fn value_metrics_live_on_both_surfaces() {
        for token in ["cpu_usage", "cpu_temp", "battery_sys"] {
            let surfaces = where_surfaces(token);
            assert!(
                !surfaces.intersection(SurfaceSet::PANEL).is_empty(),
                "{token} should be on a panel"
            );
            assert!(
                surfaces.contains(Surface::Tooltip),
                "{token} should be on the tooltip"
            );
        }
    }

    #[test]
    fn bare_visuals_are_panel_only() {
        for token in [
            "cpu_usage:bar",
            "cpu_usage:spark",
            "cpu_usage:braille",
            "mem_usage:bar",
            "mem_usage:spark",
            "mem_usage:braille",
        ] {
            let surfaces = where_surfaces(token);
            assert!(
                !surfaces.intersection(SurfaceSet::PANEL).is_empty(),
                "{token} should be on a panel"
            );
            assert!(
                !surfaces.contains(Surface::Tooltip),
                "{token} should not be on the tooltip"
            );
        }
    }

    #[test]
    fn wide_forms_and_string_metrics_are_tooltip_only() {
        for token in [
            "cpu_usage:spark_value",
            "cpu_usage:bar_spark",
            "hd_temp:pair",
            "disk_smart:pair",
            "net_device_ip",
            "top_process",
            "uptime",
            "load_avg",
        ] {
            let surfaces = where_surfaces(token);
            assert!(
                surfaces.contains(Surface::Tooltip),
                "{token} should be on the tooltip"
            );
            assert!(
                surfaces.intersection(SurfaceSet::PANEL).is_empty(),
                "{token} should not be on a panel"
            );
        }
    }

    #[test]
    fn misplaced_items_flags_tooltip_only_in_panel() {
        let (bad_panel, bad_tooltip) = misplaced_items(
            ["cpu_usage", "cpu_usage:spark_value", "top_process"],
            ["cpu_usage", "net_device_ip"],
        );
        assert_eq!(
            bad_panel,
            str_set(&["cpu_usage:spark_value", "top_process"])
        );
        assert!(bad_tooltip.is_empty());
    }

    #[test]
    fn misplaced_items_flags_panel_only_in_tooltip() {
        let (_, bad_tooltip) = misplaced_items(
            std::iter::empty(),
            [
                "cpu_usage",
                "cpu_usage:bar",
                "mem_usage:spark",
                "mem_usage:braille",
            ],
        );
        assert_eq!(
            bad_tooltip,
            str_set(&["cpu_usage:bar", "mem_usage:spark", "mem_usage:braille"])
        );
    }

    #[test]
    fn misplaced_items_ignores_unknown_names() {
        let (bad_panel, _) = misplaced_items(["totally_bogus"], std::iter::empty());
        assert!(bad_panel.is_empty());
    }

    #[test]
    fn misplaced_items_skips_separators() {
        let (bad_panel, bad_tooltip) =
            misplaced_items(["separator_small", "separator_big"], ["separator_small"]);
        assert!(bad_panel.is_empty());
        assert!(bad_tooltip.is_empty());
    }

    // ── exhaustive token corpus (plasma-top list-items parity) ────────────────

    /// Snapshot of the Python `plasma-top list-items` output captured at the
    /// integration base (commit 9a088c2). Each tuple is `(token, placement)`.
    /// Ordering is `(placement, token)`, matching the Python `sorted` key.
    const EXPECTED_LIST_ITEMS: &[(&str, &str)] = &[
        // panel + tooltip (26 entries, alphabetical)
        ("battery_kbd", "panel + tooltip"),
        ("battery_mouse", "panel + tooltip"),
        ("battery_sys", "panel + tooltip"),
        ("cpu_freq", "panel + tooltip"),
        ("cpu_temp", "panel + tooltip"),
        ("cpu_turbo", "panel + tooltip"),
        ("cpu_usage", "panel + tooltip"),
        ("disk_io", "panel + tooltip"),
        ("disk_usage", "panel + tooltip"),
        ("fan_speed", "panel + tooltip"),
        ("gpu_intel_dec_usage", "panel + tooltip"),
        ("gpu_intel_freq", "panel + tooltip"),
        ("gpu_intel_usage", "panel + tooltip"),
        ("gpu_nvidia_dec_usage", "panel + tooltip"),
        ("gpu_nvidia_fan_speed", "panel + tooltip"),
        ("gpu_nvidia_mem_usage", "panel + tooltip"),
        ("gpu_nvidia_temp", "panel + tooltip"),
        ("gpu_nvidia_usage", "panel + tooltip"),
        ("hd_temp", "panel + tooltip"),
        ("mem_usage", "panel + tooltip"),
        ("net_speed", "panel + tooltip"),
        ("screen_brightness", "panel + tooltip"),
        ("server_check", "panel + tooltip"),
        ("swap_usage", "panel + tooltip"),
        ("system_updates", "panel + tooltip"),
        ("wifi_signal", "panel + tooltip"),
        // panel only (6 entries, alphabetical)
        ("cpu_usage:bar", "panel only"),
        ("cpu_usage:braille", "panel only"),
        ("cpu_usage:spark", "panel only"),
        ("mem_usage:bar", "panel only"),
        ("mem_usage:braille", "panel only"),
        ("mem_usage:spark", "panel only"),
        // tooltip only (19 entries, alphabetical)
        ("cpu_usage:bar_braille", "tooltip only"),
        ("cpu_usage:bar_spark", "tooltip only"),
        ("cpu_usage:braille_value", "tooltip only"),
        ("cpu_usage:spark_value", "tooltip only"),
        ("disk_smart:pair", "tooltip only"),
        ("fan_speed:pair", "tooltip only"),
        ("hd_temp:pair", "tooltip only"),
        ("load_avg", "tooltip only"),
        ("mem_usage:bar_braille", "tooltip only"),
        ("mem_usage:bar_spark", "tooltip only"),
        ("mem_usage:braille_value", "tooltip only"),
        ("mem_usage:spark_value", "tooltip only"),
        ("net_device", "tooltip only"),
        ("net_device_ip", "tooltip only"),
        ("net_ip", "tooltip only"),
        ("top_process", "tooltip only"),
        ("uptime", "tooltip only"),
        ("wifi_ssid", "tooltip only"),
        ("wifi_ssid_signal", "tooltip only"),
    ];

    #[test]
    fn list_items_matches_python_oracle_byte_for_byte() {
        let actual: Vec<(String, &'static str)> = list_items();
        let expected: Vec<(String, &'static str)> = EXPECTED_LIST_ITEMS
            .iter()
            .map(|(token, placement)| ((*token).to_owned(), *placement))
            .collect();
        assert_eq!(
            actual.len(),
            expected.len(),
            "list_items length drifted from the Python oracle"
        );
        for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_eq!(
                actual, expected,
                "list_items[{index}] drifted from the Python oracle: got {actual:?}, expected {expected:?}"
            );
        }
    }

    // ── matrix: every (metric, form) × every surface ─────────────────────────

    /// Enumerates every valid `(token, effective_surfaces)` pair by walking
    /// `Metric::all()` and each metric's supported forms. Used to drive the
    /// exhaustive placement / misplaced_items matrix.
    fn every_token_surfaces() -> Vec<(String, SurfaceSet)> {
        let mut out = Vec::new();
        for metric in Metric::all() {
            let spec = metric.spec();
            if spec.intrinsic_shape.is_some() {
                let token = metric.to_string();
                out.push((token, metric.surfaces()));
                continue;
            }
            for form in spec.generic_forms {
                let token = match form {
                    Form::Value => metric.to_string(),
                    _ => format!("{metric}:{form}"),
                };
                let item = match ItemToken::from_str(&token) {
                    Ok(item) => item,
                    Err(_) => continue,
                };
                out.push((token, item.effective_surfaces()));
            }
        }
        out
    }

    #[test]
    fn misplaced_matrix_matches_effective_surfaces_for_every_token() {
        for (token, surfaces) in every_token_surfaces() {
            let on_panel = !surfaces.intersection(SurfaceSet::PANEL).is_empty();
            let on_tooltip = surfaces.contains(Surface::Tooltip);

            // Panel pass: a token is misplaced on the panel iff its effective
            // surfaces don't admit either panel orientation.
            let (panel_bad, _) = misplaced_items([token.as_str()], std::iter::empty());
            assert_eq!(
                panel_bad.contains(&token),
                !on_panel,
                "panel placement mismatch for {token}: effective={surfaces:?}, on_panel={on_panel}"
            );

            // Tooltip pass: a token is misplaced on the tooltip iff its
            // effective surfaces don't admit the tooltip.
            let (_, tooltip_bad) = misplaced_items(std::iter::empty(), [token.as_str()]);
            assert_eq!(
                tooltip_bad.contains(&token),
                !on_tooltip,
                "tooltip placement mismatch for {token}: effective={surfaces:?}, on_tooltip={on_tooltip}"
            );
        }
    }

    #[test]
    fn item_token_round_trips_for_every_corpus_entry() {
        for (token, _) in every_token_surfaces() {
            let parsed = must_parse(&token);
            let redisplayed = parsed.to_string();
            assert_eq!(
                redisplayed, token,
                "ItemToken Display did not round-trip: parsed {token:?}, got {redisplayed:?}"
            );
        }
    }

    #[test]
    fn intrinsic_tokens_render_without_a_form() {
        for token in ["net_speed", "disk_io", "top_process"] {
            let item = must_parse(token);
            assert!(
                matches!(item.rendering(), ItemRendering::Intrinsic(_)),
                "{token} should render intrinsically"
            );
            assert!(item.form().is_none());
        }
    }

    #[test]
    fn separator_set_matches_render_model_keys() {
        assert_eq!(SEPARATOR_ITEMS, &["separator_small", "separator_big"]);
    }

    #[test]
    fn notification_capability_map_covers_python_notify_caps() {
        let map = notification_capability_map();
        let keys: Vec<&str> = map.iter().map(|(key, _)| *key).collect();
        assert_eq!(
            keys,
            [
                "cpu_temp",
                "gpu_nvidia_temp",
                "disk_usage",
                "disk_smart",
                "hd_temp",
                "battery_sys",
                "battery_mouse",
                "battery_kbd",
                "load_avg",
                "server_check",
            ]
        );
        // spot-check three values that exercise the spelling-mismatch cases
        // (flag name ≠ capability token)
        let by_key: std::collections::BTreeMap<&str, Capability> = map.iter().copied().collect();
        assert_eq!(by_key.get("cpu_temp"), Some(&Capability::CpuTemperature));
        assert_eq!(by_key.get("gpu_nvidia_temp"), Some(&Capability::GpuNvidia));
        assert_eq!(
            by_key.get("battery_kbd"),
            Some(&Capability::BatteryKeyboard)
        );
    }
}
