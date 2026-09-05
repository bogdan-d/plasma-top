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
    ("gpu_amd_temp", Capability::GpuAmdTemperature),
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
/// `src/registry.py`: the graphs page requests GPU and network
/// history even when no such item lives on a surface, so its capabilities are
/// requested unconditionally. The hardware gate in the collector narrows this
/// to the GPU/interface actually present.
pub const GRAPHS_PAGE_CAPABILITIES: &[Capability] = &[
    Capability::GpuNvidia,
    Capability::GpuAmdUsage,
    Capability::GpuAmdCodec,
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
mod tests;
