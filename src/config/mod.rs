//! Configuration loading, typed view, and validation.
//!
//! Mirrors `src/config.py` (885 lines). The files in this module split
//! the responsibilities Python keeps in one:
//!
//! - this file (`mod.rs`): [`load_config`], [`apply_canonical_width`], the
//!   unknown/misplaced item guardrails, and shared constants.
//! - `schema`: the typed [`Config`] tree and sub-config defaults.
//! - [`merge`]: raw-table deep merge, surface parsing, machine block merge,
//!   asset path selection.
//! - [`geometry`]: Plasma appletsrc parsing, geom-file reads, the vertical
//!   auto-fit, and machine DMI detection.
//! - [`assets`]: code root / XDG / home path resolution.
//!
//! The typed tree uses [`serde`] derives for the leaf sub-configs
//! (`DisplayConfig`, `ThresholdConfig`, …). Unknown keys are silently
//! ignored (matching Python's `_from_dict`), and missing keys fall back to
//! the per-struct [`Default`] impl.

pub mod assets;
pub mod geometry;
pub mod merge;

mod schema;

use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use toml::{Table, Value};

use crate::domain::Cadence;
use crate::domain::registry::{misplaced_items, unknown_item_names};

pub use merge::{deep_merge_tables, parse_surface, resolve_items};
pub use merge::{default_config_path, machine_source_paths, machines_path_for, resolve_style};
pub use merge::{load_machines, load_toml_at, user_machines_path};

// Re-export the typed tree and helpers at the module root so callers can
// write `config::Config`, `config::DisplayConfig`, etc., without reaching
// into the private `schema` submodule.
pub use self::schema::{
    BarConfig, BatteryConfig, BrailleConfig, ColumnConfig, Config, DiskConfig, DisplayConfig,
    Mounts, NotificationConfig, NotifyThresholds, PagesConfig, Section, SensorOverrides,
    ServerCheckConfig, SparkConfig, Surface, SystemUpdatesConfig, ThresholdConfig,
};
pub use geometry::{
    PanelGeometry, auto_fit_panel, cache_live_geom, cache_live_geom_at, detect_machine,
    detect_machine_with_dmi, detect_panel_geometry, detect_panel_geometry_at,
    detect_vertical_from_appletsrc, detect_vertical_from_appletsrc_at,
    detect_vertical_from_appletsrc_text, detect_vertical_layout, dmi_paths, geom_cache_path,
    parse_geom, parse_kde_ini, plasma_appletsrc_path, read_geom_file, read_geom_file_at,
};

// ── Constants ───────────────────────────────────────────────────────────────

/// Built-in lower bound for the tooltip width (monospace columns).
///
/// Keeps the tooltip from looking cramped on a sparse config, before the
/// main page's canonical width (usually larger) takes over. Not a user
/// knob — a sensible minimum. See [`DisplayConfig::tooltip_width`] and
/// [`apply_canonical_width`].
pub const TOOLTIP_WIDTH_FLOOR: i32 = 30;

/// `cpu_usage:braille`/`mem_usage:braille` pack this many samples per char
/// (see `traces.braille_html`). To occupy the same visual width as the
/// 1-sample/char block spark at the same `*_history_length`, they need
/// `BRAILLE_LENGTH_MULTIPLIER`× the underlying samples. The sensor lane
/// sizes its history deque off this so the buffer is never the bottleneck,
/// regardless of whether braille items are actually enabled.
pub const BRAILLE_LENGTH_MULTIPLIER: i32 = 2;

/// On-screen advance ratio of one CSS-px monospace glyph vs its CSS-px
/// font-size. DPI-independent and constant across sizes (measured via
/// `QFontMetricsF`). Used by [`geometry::auto_fit_panel`] to derive the
/// bar's advance from its `height` knob.
pub const CSS_ADVANCE_RATIO: f64 = 0.6;

/// Pixels shaved off the usable width when sizing the bar (was
/// `_BAR_SAFETY_PX` in Python). Font hinting rounds each glyph's advance
/// per size, so the last of many glyphs can land a pixel past the edge and
/// wrap; `floor + this reserve` keeps the bar just inside for any height.
pub const BAR_SAFETY_PX: f64 = 1.0;

/// Ratio of a digit's on-screen height to a full-block glyph's at the same
/// font-size (was `_COLUMN_DIGIT_RATIO` in Python). Used to size the
/// horizontal panel's column glyph so its grey track matches the digit
/// height of the values beside it.
pub const COLUMN_DIGIT_RATIO: f64 = 0.612;

// ── Error ───────────────────────────────────────────────────────────────────

/// Errors returned by [`load_config`] and the typed-tree deserialization
/// helpers.
///
/// Kept as the config-owned source error and promoted through
/// [`crate::Error::Config`](crate::error::Error::Config) at the application
/// boundary.
#[derive(Debug)]
pub enum ConfigError {
    /// A config file could not be read.
    Io(std::io::Error),
    /// A config file is not valid TOML, or a sub-table failed to
    /// deserialize into its typed view.
    Toml(toml::de::Error),
}

impl Display for ConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "config I/O failure: {error}"),
            Self::Toml(error) => write!(formatter, "config parse failure: {error}"),
        }
    }
}

impl std::error::Error for ConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Toml(error) => Some(error),
        }
    }
}

impl From<std::io::Error> for ConfigError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<toml::de::Error> for ConfigError {
    fn from(error: toml::de::Error) -> Self {
        Self::Toml(error)
    }
}

// ── load_config ─────────────────────────────────────────────────────────────

/// Loads and fully resolves a PlasmaTop config.
///
/// Mirrors `src/config.py::load_config` (lines 772–848). The resolution
/// pipeline, applied in order, matches Python exactly:
///
/// 1. Machine files feed off the ORIGINAL `path` arg: `None` → the default
///    resolution (shipped base + the user's XDG machines); an explicit
///    `--config` → only its own sibling, keeping it self-contained.
/// 2. `path = None` resolves to [`merge::default_config_path`].
/// 3. Missing path → empty `Config` (machine detection still runs).
/// 4. The machine block whose `[<name>.detect]` matches this host is
///    deep-merged over the raw TOML.
/// 5. Glyphs (`style/icons.toml`) and labels (`lang/<language>.toml`) are
///    loaded as flat tables.
/// 6. Orientation override (`[panel_horizontal]` / `[panel_vertical]`) is
///    deep-merged onto `[panel]`.
/// 7. Each typed sub-config is deserialized from its raw sub-table.
/// 8. [`geometry::auto_fit_panel`] sizes the panel visuals from the live
///    Plasma geometry.
/// 9. [`drop_unknown_items`] / [`drop_misplaced_items`] enforce the item
///    registry.
///
/// `vertical` overrides orientation auto-detection when given (used by
/// `render --layout` and by tests).
///
/// # Errors
///
/// Returns [`ConfigError::Io`] when an explicit path is unreadable and
/// [`ConfigError::Toml`] when a config file is malformed or fails to
/// deserialize into its typed view. Missing files (the default resolution
/// path) fall back to an empty `Config`, matching Python — they are not
/// errors.
/// Thin wrapper around [`load_config_with_dmi`]: reads this host's DMI
/// board/product strings and delegates. When the DMI files are unreadable
/// (typical on VMs/containers without a fake sysfs),
/// [`load_config_with_dmi`] is called with empty strings so no machine
/// block matches — matching Python's `OSError → None` fall-through in
/// `detect_machine`.
///
/// # Errors
///
/// Returns [`ConfigError::Io`] when an explicit path is unreadable and
/// [`ConfigError::Toml`] when a config file is malformed or fails to
/// deserialize into its typed view. Missing files (the default resolution
/// path) fall back to an empty `Config`, matching Python — they are not
/// errors.
pub fn load_config(path: Option<&Path>, vertical: Option<bool>) -> Result<Config, ConfigError> {
    let machines = load_machines(path);
    let machine = detect_machine(&machines);
    load_config_with_machine(path, vertical, &machines, machine.as_deref())
}

/// Test-friendly entry point: takes the DMI board/product strings
/// explicitly so the machine match can be exercised without staging
/// `/sys/class/dmi/id/...` files.
///
/// Replaces Python's `monkeypatch.setattr("config.detect_machine", …)`
/// pattern. Used by the `machine_items_add` / `machine_order_add_new_section`
/// parity tests in this module's test suite.
///
/// # Errors
///
/// Same failure modes as [`load_config`].
pub fn load_config_with_dmi(
    path: Option<&Path>,
    vertical: Option<bool>,
    board: &str,
    product: &str,
) -> Result<Config, ConfigError> {
    let machines = load_machines(path);
    let machine = detect_machine_with_dmi(&machines, board, product);
    load_config_with_machine(path, vertical, &machines, machine.as_deref())
}

/// The full resolution pipeline once the machine block (if any) is known.
///
/// Split out of [`load_config`] / [`load_config_with_dmi`] so the two entry
/// points share one body. The pipeline, applied in order, mirrors
/// `src/config.py::load_config` (lines 772–848):
///
/// 1. `path = None` resolves to [`merge::default_config_path`]; a missing
///    resolved path returns an empty `Config` (still recording the matched
///    machine name, if any).
/// 2. The matched machine block is deep-merged over the raw TOML.
/// 3. Glyphs (`style/icons.toml`) and labels (`lang/<language>.toml`) are
///    loaded as flat tables.
/// 4. Orientation override (`[panel_horizontal]` / `[panel_vertical]`) is
///    deep-merged onto `[panel]` by the resolved orientation.
/// 5. Each typed sub-config is deserialized from its raw sub-table.
/// 6. [`geometry::auto_fit_panel`] sizes the panel visuals from the live
///    Plasma geometry.
/// 7. [`drop_unknown_items`] / [`drop_misplaced_items`] enforce the item
///    registry.
#[expect(
    clippy::collapsible_if,
    reason = "machine selection, lookup, and table conversion are distinct config stages"
)]
fn load_config_with_machine(
    path: Option<&Path>,
    vertical: Option<bool>,
    machines: &Table,
    machine: Option<&str>,
) -> Result<Config, ConfigError> {
    let resolved_path: PathBuf = match path {
        Some(p) => p.to_path_buf(),
        None => default_config_path(),
    };

    if !resolved_path.exists() {
        return Ok(Config {
            machine: machine.unwrap_or_default().to_owned(),
            vertical: vertical.unwrap_or(true),
            ..Config::default()
        });
    }

    let bytes = std::fs::read(&resolved_path)?;
    let mut raw: Table = toml::from_slice(&bytes)?;

    if let Some(name) = machine {
        if let Some(machine_data) = machines.get(name) {
            if let Some(machine_table) = machine_data.as_table() {
                raw = deep_merge_tables(raw, machine_table.clone());
            }
        }
    }

    let language = raw
        .get("display")
        .and_then(Value::as_table)
        .and_then(|d| d.get("language"))
        .and_then(Value::as_str)
        .unwrap_or("en")
        .to_owned();
    let icons_path = resolve_style("icons.toml");
    let icons = load_toml_at(&icons_path);
    let labels_path = assets::code_root()
        .join("lang")
        .join(format!("{language}.toml"));
    let labels = load_toml_at(&labels_path);

    let geo = if let Some(vertical_forced) = vertical {
        // Skip the live appletsrc read when the caller forces orientation:
        // the geom file may briefly disagree right after a panel move, and
        // tests pass `vertical = Some(_)` precisely to avoid that.
        // We still consult the geom file for the auto-fit measurements.
        let mut g = detect_panel_geometry_at(
            &plasma_appletsrc_path(),
            &crate::runtime::geom_file(),
            &geom_cache_path(),
        );
        g.vertical = vertical_forced;
        g
    } else {
        detect_panel_geometry()
    };
    let is_vertical = vertical.unwrap_or(geo.vertical);

    let raw_panel_value = raw
        .get("panel")
        .cloned()
        .unwrap_or_else(|| Value::Table(Table::new()));
    let mut raw_panel = raw_panel_value.as_table().cloned().unwrap_or_default();
    let override_key = if is_vertical {
        "panel_vertical"
    } else {
        "panel_horizontal"
    };
    if let Some(Value::Table(override_table)) = raw.get(override_key).cloned() {
        raw_panel = deep_merge_tables(raw_panel, override_table);
    }

    let raw_tooltip = raw
        .get("tooltip")
        .and_then(Value::as_table)
        .cloned()
        .unwrap_or_default();
    let panel = parse_surface(&raw_panel);
    let tooltip = parse_surface(&raw_tooltip);

    let mut cfg = Config {
        display: typed_section(&raw, "display")?,
        vertical: is_vertical,
        bar_panel: typed_section(&raw, "bar_panel")?,
        column_panel: typed_section(&raw, "column_panel")?,
        bar_tooltip: typed_section(&raw, "bar_tooltip")?,
        spark_panel: typed_section(&raw, "spark_panel")?,
        spark_tooltip: typed_section(&raw, "spark_tooltip")?,
        braille_panel: typed_section(&raw, "braille_panel")?,
        braille_tooltip: typed_section(&raw, "braille_tooltip")?,
        panel,
        tooltip,
        pages: typed_section(&raw, "pages")?,
        thresholds: typed_section(&raw, "thresholds")?,
        notify_thresholds: typed_section(&raw, "notify_thresholds")?,
        notifications: typed_section(&raw, "notifications")?,
        icons,
        labels,
        sensors: typed_section(&raw, "sensors")?,
        disks: typed_section(&raw, "disks")?,
        battery: typed_section(&raw, "battery")?,
        system_updates: typed_section(&raw, "system_updates")?,
        server_check: typed_section(&raw, "server_check")?,
        machine: machine.unwrap_or_default().to_owned(),
    };

    auto_fit_panel(&mut cfg, &geo);
    drop_unknown_items(&mut cfg);
    drop_misplaced_items(&mut cfg);
    Ok(cfg)
}

/// Deserializes a typed sub-config from `raw[key]`, defaulting to an empty
/// table when the key is absent.
///
/// Mirrors Python's `_build_section`: each typed view tolerates unknown
/// keys (serde ignores them by default) and fills missing fields from the
/// struct's `Default` impl via `#[serde(default)]` at the container level.
fn typed_section<'de, T>(raw: &'de Table, key: &str) -> Result<T, ConfigError>
where
    T: serde::Deserialize<'de>,
{
    let value = raw
        .get(key)
        .cloned()
        .unwrap_or_else(|| Value::Table(Table::new()));
    T::deserialize(value).map_err(ConfigError::from)
}

/// Resolves the tooltip width every page renders to, then re-derives the
/// graphs PNG width from it.
///
/// Mirrors `src/config.py::apply_canonical_width` (lines 719–735).
/// `canonical` comes from the formatter (which needs readings, so this
/// can't run inside [`load_config`]); the daemon/render call it once a
/// readings snapshot exists. `0` = skip (nothing to measure), leaving the
/// default.
///
/// `tooltip_width` is written fresh from the floor each call (not read
/// back), so a shrinking canonical — a disk unmounted, the interface
/// shortened — lowers the width again instead of the field ratcheting up
/// against its own previous max.
///
/// To avoid hitting the live Plasma appletsrc from a hot reload path, the
/// tooltip-advance lookup uses [`geometry::geom_cache_path`] as the
/// fallback when the live geom file is absent (matches Python's
/// `_read_geom_file` behavior via [`geometry::read_geom_file`]).
pub fn apply_canonical_width(cfg: &mut Config, canonical: i32) {
    if canonical <= 0 {
        return;
    }
    let tooltip_width = TOOLTIP_WIDTH_FLOOR.max(canonical);
    cfg.display.tooltip_width = tooltip_width;
    if let Some(tip) = read_geom_file().and_then(|geo| geo.tooltip_adv) {
        cfg.pages.graph_width = (f64::from(tooltip_width) * tip).round() as i32;
    }
}

// ── Item guardrails ─────────────────────────────────────────────────────────

/// Drops items listed in sections but not recognized (a typo in the TOML).
///
/// Mirrors `src/config.py::_drop_unknown_items`. Separators are valid
/// section entries rather than items, so [`unknown_item_names`] spares
/// them. The dropped tokens are reported once on stderr — kept identical
/// to Python so log scrapers and the daemon's hot-reload diagnostics stay
/// stable.
pub fn drop_unknown_items(cfg: &mut Config) {
    let panel_bad: BTreeSet<String> =
        unknown_item_names(cfg.panel.item_set().iter().map(String::as_str));
    let tooltip_bad: BTreeSet<String> =
        unknown_item_names(cfg.tooltip.item_set().iter().map(String::as_str));
    drop_items(&mut cfg.panel, &panel_bad, "unknown items in the panel");
    drop_items(
        &mut cfg.tooltip,
        &tooltip_bad,
        "unknown items in the tooltip",
    );
}

/// Drops items placed on a surface that doesn't admit them.
///
/// Mirrors `src/config.py::_drop_misplaced_items`. Runs before the
/// canonical width is derived, so a dropped item never widens the tooltip.
pub fn drop_misplaced_items(cfg: &mut Config) {
    let (bad_panel, bad_tooltip) = misplaced_items(
        cfg.panel.item_set().iter().map(String::as_str),
        cfg.tooltip.item_set().iter().map(String::as_str),
    );
    drop_items(
        &mut cfg.panel,
        &bad_panel,
        "tooltip-only items placed in the panel",
    );
    drop_items(
        &mut cfg.tooltip,
        &bad_tooltip,
        "panel-only items placed in the tooltip",
    );
}

/// Shared tail of the two guardrails: remove `bad` from every section of
/// `surface` and report once on stderr.
///
/// An emptied section is left in place — the render collapses empty ones
/// on its own (mirrors Python's `_drop_items`).
fn drop_items(surface: &mut Surface, bad: &BTreeSet<String>, what: &str) {
    if bad.is_empty() {
        return;
    }
    for section in &mut surface.sections {
        section.items.retain(|item| !bad.contains(item));
    }
    let joined = bad
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    eprintln!("[config] {what}, dropped: {joined}");
}

#[cfg(test)]
mod tests;
