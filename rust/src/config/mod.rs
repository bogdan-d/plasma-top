//! Configuration loading, typed view, and validation.
//!
//! Mirrors `src/config.py` (885 lines). The four files in this module split
//! the responsibilities Python keeps in one:
//!
//! - this file (`mod.rs`): the typed [`Config`] tree and sub-config structs,
//!   [`load_config`], [`apply_canonical_width`], the unknown/misplaced item
//!   guardrails, and the constants every other module reads.
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

use std::collections::BTreeSet;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use toml::{Table, Value};

use crate::domain::registry::{misplaced_items, unknown_item_names};

pub use merge::{deep_merge_tables, parse_surface, resolve_items};
pub use merge::{default_config_path, machine_source_paths, machines_path_for, resolve_style};
pub use merge::{load_machines, load_toml_at, user_machines_path};

// Re-export the typed tree and helpers at the module root so callers can
// write `config::Config`, `config::DisplayConfig`, etc., without reaching
// into a `types` submodule.
pub use self::typed::{
    BarConfig, BatteryConfig, BrailleConfig, ColumnConfig, DiskConfig, DisplayConfig, Mounts,
    NotificationConfig, NotifyThresholds, PagesConfig, Section, SensorOverrides, ServerCheckConfig,
    SparkConfig, Surface, SystemUpdatesConfig, ThresholdConfig,
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

// ── Typed Config tree ───────────────────────────────────────────────────────
//
// Kept in a private `typed` submodule so the re-exports above stay tidy
// while keeping the long Default impls out of the load/save logic. Every
// leaf struct derives `Deserialize` and uses `#[serde(default)]` at the
// container level — serde then fills any missing field from the struct's
// own `Default` impl, mirroring Python's `_from_dict` (which ignores
// unknown keys and falls back to dataclass defaults).

mod typed {
    use serde::Deserialize;
    use toml::Value;

    use super::{COLUMN_DIGIT_RATIO, TOOLTIP_WIDTH_FLOOR};

    /// Global display knobs: the daemon's two cadences, plus the inspection
    /// aid. Mirrors `DisplayConfig` in `src/config.py`.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct DisplayConfig {
        /// Rewrite of both panel and tooltip HTML, in seconds.
        pub poll_interval: f64,
        /// Sampling cadence of the shared history buffer (read by every
        /// spark/braille form and the graphs page).
        pub history_interval: f64,
        /// Label language; loads `lang/<language>.toml`.
        pub language: String,
        /// Compact `top_process` item truncate length (0 = off).
        pub top_process_name_max_len: i32,
        /// Alignment divisor for values at the bar's edge; auto-derived in
        /// the vertical Plasma panel.
        pub panel_font_size: i32,
        /// The RESOLVED tooltip width every page + the graphs PNG render to;
        /// set at runtime by [`super::apply_canonical_width`].
        pub tooltip_width: i32,
        /// Minimum vertical-panel width in monospace columns; auto-derived.
        pub panel_min_width: i32,
        /// Inspection overlay: per-cell diagnostic backgrounds.
        pub overlay: bool,
    }

    impl Default for DisplayConfig {
        fn default() -> Self {
            Self {
                poll_interval: 1.5,
                history_interval: 1.5,
                language: String::from("en"),
                top_process_name_max_len: 20,
                panel_font_size: 13,
                tooltip_width: TOOLTIP_WIDTH_FLOOR,
                panel_min_width: 5,
                overlay: false,
            }
        }
    }

    /// Tooltip deep-dive pages: which ones the wheel cycles through, and
    /// the only knob `graphs` exposes.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct PagesConfig {
        /// Deep-dive pages in wheel order; this lists what follows page 0.
        pub order: Vec<String>,
        /// Samples the graphs page's history charts keep.
        pub graph_history_length: i32,
        /// Graphs page PNG width in px; AUTO-derived from the resolved
        /// tooltip width.
        pub graph_width: i32,
    }

    impl Default for PagesConfig {
        fn default() -> Self {
            Self {
                order: vec![
                    String::from("processes"),
                    String::from("cpu_cores"),
                    String::from("connections"),
                    String::from("fastfetch"),
                ],
                graph_history_length: 60,
                graph_width: 315,
            }
        }
    }

    /// Visual knobs for the `:bar` form. Width auto-fits in the vertical
    /// Plasma panel; `height` is the manual knob.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct BarConfig {
        /// Bar width in monospace columns (auto-fit overrides in vertical).
        pub width: i32,
        /// Strip thickness in px (font-size of the bar glyphs).
        pub height: i32,
    }

    impl Default for BarConfig {
        fn default() -> Self {
            Self {
                width: 22,
                height: 0,
            }
        }
    }

    /// Visual knobs for the `:spark` form (block sparkline).
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct SparkConfig {
        /// cpu_usage:spark length in chars.
        pub cpu_spark_length: i32,
        /// mem_usage:spark length in chars.
        pub mem_spark_length: i32,
    }

    impl Default for SparkConfig {
        fn default() -> Self {
            Self {
                cpu_spark_length: 5,
                mem_spark_length: 5,
            }
        }
    }

    /// Visual knobs for the `:braille` form.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct BrailleConfig {
        /// cpu_usage:braille length in chars (2 samples/char).
        pub cpu_braille_length: i32,
        /// mem_usage:braille length in chars.
        pub mem_braille_length: i32,
    }

    impl Default for BrailleConfig {
        fn default() -> Self {
            Self {
                cpu_braille_length: 5,
                mem_braille_length: 5,
            }
        }
    }

    /// Visual knobs for the `:bar` form when it renders as a vertical column
    /// in the horizontal panel.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct ColumnConfig {
        /// Column thickness in glyphs.
        pub width: i32,
        /// Block glyph font-size in px; auto-fit in the horizontal panel.
        pub height: i32,
    }

    impl Default for ColumnConfig {
        fn default() -> Self {
            // The default `height` mirrors the Python default 0, which means
            // "inherit"; the horizontal auto-fit overrides it. The literal
            // `0.0` is unused — `COLUMN_DIGIT_RATIO` is referenced to keep
            // the const in scope and silence dead-code warnings if a future
            // edit removes its only other use.
            let _ = COLUMN_DIGIT_RATIO;
            Self {
                width: 1,
                height: 0,
            }
        }
    }

    /// 3-band color thresholds: `[mid, high]`. Below `mid` → low color,
    /// between mid and high → mid, from high → high.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct ThresholdConfig {
        /// cpu_usage thresholds.
        pub cpu_usage: Vec<i32>,
        /// cpu_spark thresholds.
        pub cpu_spark: Vec<i32>,
        /// mem_spark thresholds.
        pub mem_spark: Vec<i32>,
        /// mem_usage thresholds.
        pub mem_usage: Vec<i32>,
        /// Per-process CPU bands for the processes page.
        pub top_process_cpu: Vec<i32>,
        /// Per-process memory bands for the processes page.
        pub top_process_mem: Vec<i32>,
        /// swap_usage thresholds.
        pub swap_usage: Vec<i32>,
        /// disk_usage thresholds.
        pub disk_usage: Vec<i32>,
        /// cpu_temp thresholds.
        pub cpu_temp: Vec<i32>,
        /// gpu_nvidia_temp thresholds.
        pub gpu_nvidia_temp: Vec<i32>,
        /// gpu_nvidia_usage thresholds.
        pub gpu_nvidia_usage: Vec<i32>,
        /// gpu_nvidia_mem_usage thresholds.
        pub gpu_nvidia_mem_usage: Vec<i32>,
        /// gpu_intel_usage thresholds.
        pub gpu_intel_usage: Vec<i32>,
        /// hd_temp thresholds.
        pub hd_temp: Vec<i32>,
        /// battery_sys thresholds (inverted: low = alarm).
        pub battery_sys: Vec<i32>,
        /// battery_mouse thresholds.
        pub battery_mouse: Vec<i32>,
        /// battery_kbd thresholds.
        pub battery_kbd: Vec<i32>,
        /// wifi_signal thresholds (inverted: low = alarm).
        pub wifi_signal: Vec<i32>,
        /// gpu_nvidia_dec_usage single-value binary threshold.
        pub gpu_nvidia_dec_usage: i32,
        /// gpu_intel_dec_usage single-value binary threshold.
        pub gpu_intel_dec_usage: i32,
        /// load_avg_1 thresholds as a fraction of cores.
        pub load_avg_1: Vec<f64>,
        /// load_avg_5 thresholds as a fraction of cores.
        pub load_avg_5: Vec<f64>,
        /// load_avg_15 thresholds as a fraction of cores.
        pub load_avg_15: Vec<f64>,
    }

    impl Default for ThresholdConfig {
        fn default() -> Self {
            Self {
                cpu_usage: vec![50, 70],
                cpu_spark: vec![50, 70],
                mem_spark: vec![40, 60],
                mem_usage: vec![40, 60],
                top_process_cpu: vec![50, 70],
                top_process_mem: vec![15, 30],
                swap_usage: vec![50, 70],
                disk_usage: vec![50, 80],
                cpu_temp: vec![50, 70],
                gpu_nvidia_temp: vec![50, 70],
                gpu_nvidia_usage: vec![50, 70],
                gpu_nvidia_mem_usage: vec![50, 70],
                gpu_intel_usage: vec![50, 70],
                hd_temp: vec![50, 55],
                battery_sys: vec![20, 80],
                battery_mouse: vec![20, 80],
                battery_kbd: vec![20, 80],
                wifi_signal: vec![30, 60],
                gpu_nvidia_dec_usage: 1,
                gpu_intel_dec_usage: 1,
                load_avg_1: vec![0.7, 1.0],
                load_avg_5: vec![0.6, 0.9],
                load_avg_15: vec![0.5, 0.8],
            }
        }
    }

    /// Thresholds that trigger a desktop notification (independent of color
    /// thresholds).
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct NotifyThresholds {
        /// disk_usage notify threshold.
        pub disk_usage: i32,
        /// cpu_temp notify threshold.
        pub cpu_temp: i32,
        /// gpu_nvidia_temp notify threshold.
        pub gpu_nvidia_temp: i32,
        /// hd_temp notify threshold.
        pub hd_temp: i32,
        /// battery_sys notify threshold.
        pub battery_sys: i32,
        /// battery_mouse notify threshold.
        pub battery_mouse: i32,
        /// battery_kbd notify threshold.
        pub battery_kbd: i32,
        /// Seconds a temperature must hold over its threshold to notify.
        pub temp_sustain_seconds: i32,
        /// Degrees a temperature must fall below its threshold to re-arm.
        pub temp_hysteresis: i32,
        /// load_avg_15 notify threshold as a fraction of cores.
        pub load_avg_15: f64,
        /// Minutes `load_avg_15` must hold over its threshold to notify.
        pub load_avg_minutes: i32,
    }

    impl Default for NotifyThresholds {
        fn default() -> Self {
            Self {
                disk_usage: 80,
                cpu_temp: 80,
                gpu_nvidia_temp: 80,
                hd_temp: 60,
                battery_sys: 10,
                battery_mouse: 20,
                battery_kbd: 20,
                temp_sustain_seconds: 60,
                temp_hysteresis: 5,
                load_avg_15: 0.9,
                load_avg_minutes: 10,
            }
        }
    }

    /// Notification enable flags.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct NotificationConfig {
        /// disk_usage notification enabled.
        pub disk_usage: bool,
        /// disk_smart notification enabled.
        pub disk_smart: bool,
        /// cpu_temp notification enabled.
        pub cpu_temp: bool,
        /// gpu_nvidia_temp notification enabled.
        pub gpu_nvidia_temp: bool,
        /// hd_temp notification enabled.
        pub hd_temp: bool,
        /// battery_sys notification enabled.
        pub battery_sys: bool,
        /// battery_mouse notification enabled.
        pub battery_mouse: bool,
        /// battery_kbd notification enabled.
        pub battery_kbd: bool,
        /// server_check notification enabled.
        pub server_check: bool,
        /// load_avg notification enabled.
        pub load_avg: bool,
    }

    impl Default for NotificationConfig {
        fn default() -> Self {
            Self {
                disk_usage: true,
                disk_smart: true,
                cpu_temp: false,
                gpu_nvidia_temp: false,
                hd_temp: true,
                battery_sys: true,
                battery_mouse: true,
                battery_kbd: true,
                server_check: false,
                load_avg: false,
            }
        }
    }

    /// Manual hwmon sensor spec in `'chip|file'` format (same as the bash
    /// config).
    #[derive(Debug, Clone, PartialEq, Deserialize, Default)]
    #[serde(default)]
    pub struct SensorOverrides {
        /// cpu_temp hwmon spec.
        pub cpu_temp: Option<String>,
        /// fan1_speed hwmon spec.
        pub fan1_speed: Option<String>,
        /// fan2_speed hwmon spec.
        pub fan2_speed: Option<String>,
        /// fan3_speed hwmon spec.
        pub fan3_speed: Option<String>,
        /// fan4_speed hwmon spec.
        pub fan4_speed: Option<String>,
        /// hd1_temp hwmon spec.
        pub hd1_temp: Option<String>,
        /// hd2_temp hwmon spec.
        pub hd2_temp: Option<String>,
        /// hd3_temp hwmon spec.
        pub hd3_temp: Option<String>,
        /// hd4_temp hwmon spec.
        pub hd4_temp: Option<String>,
    }

    /// `list[str] | str = "auto"` mount selection for [`DiskConfig`].
    ///
    /// `"auto"` discovers real mounts under `auto_roots` (plus `/` always)
    /// via `psutil.disk_partitions()`, so external drives appear and
    /// disappear on their own. An explicit list of mountpoints gives
    /// manual control. Any string other than `"auto"` is treated as a
    /// one-element list, mirroring Python's tolerant typing.
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub enum Mounts {
        /// Discover mounts automatically.
        #[default]
        Auto,
        /// An explicit list of mountpoints.
        Explicit(Vec<String>),
    }

    impl<'de> Deserialize<'de> for Mounts {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let value = Value::deserialize(deserializer)?;
            match value {
                Value::String(s) if s == "auto" => Ok(Self::Auto),
                Value::String(s) => Ok(Self::Explicit(vec![s])),
                Value::Array(items) => {
                    let mut mounts = Vec::with_capacity(items.len());
                    for item in items {
                        let text = item.as_str().ok_or_else(|| {
                            serde::de::Error::custom(
                                "expected string or array of strings for `mounts`",
                            )
                        })?;
                        mounts.push(text.to_owned());
                    }
                    Ok(Self::Explicit(mounts))
                }
                _ => Err(serde::de::Error::custom(
                    "expected string or array of strings for `mounts`",
                )),
            }
        }
    }

    /// Disk discovery and SMART knobs.
    #[derive(Debug, Clone, PartialEq, Deserialize)]
    #[serde(default)]
    pub struct DiskConfig {
        /// `"auto"` or an explicit mountpoint list.
        pub mounts: Mounts,
        /// Roots scanned when `mounts == Auto`.
        pub auto_roots: Vec<String>,
        /// Whether to run SMART self-assessments.
        pub smart: bool,
        /// SMART re-check TTL for SSD/NVMe drives (seconds).
        pub smart_interval: f64,
        /// SMART re-check TTL for rotational drives (seconds).
        pub smart_interval_hdd: f64,
    }

    impl Default for DiskConfig {
        fn default() -> Self {
            Self {
                mounts: Mounts::Auto,
                auto_roots: vec![
                    String::from("/mnt"),
                    String::from("/media"),
                    String::from("/run/media"),
                ],
                smart: true,
                smart_interval: 3600.0,
                smart_interval_hdd: 21600.0,
            }
        }
    }

    /// Peripheral battery hints (Unifying/Bolt/name overrides).
    #[derive(Debug, Clone, PartialEq, Deserialize, Default)]
    #[serde(default)]
    pub struct BatteryConfig {
        /// Mouse Unifying receiver serial.
        pub mouse_unifying: Option<String>,
        /// Keyboard Unifying receiver serial.
        pub kbd_unifying: Option<String>,
        /// Mouse Bolt device index.
        pub mouse_bolt: Option<i32>,
        /// Keyboard Bolt device index.
        pub kbd_bolt: Option<i32>,
        /// Mouse device-name override.
        pub mouse_name: Option<String>,
        /// Keyboard device-name override.
        pub kbd_name: Option<String>,
    }

    /// System-updates checker (file-based, no in-loop subprocess).
    #[derive(Debug, Clone, PartialEq, Deserialize, Default)]
    #[serde(default)]
    pub struct SystemUpdatesConfig {
        /// Path written by an external updates checker; empty = disabled.
        pub file: String,
    }

    /// Server-reachability checker (file-based, no in-loop ping).
    #[derive(Debug, Clone, PartialEq, Deserialize, Default)]
    #[serde(default)]
    pub struct ServerCheckConfig {
        /// Path written by an external ping checker; empty = disabled.
        pub file: String,
    }

    /// A renderable section of a panel/tooltip surface.
    ///
    /// `key` is the section's identifier in TOML; `title` renders only in
    /// the tooltip; `items` is the membership list (presence = enabled,
    /// order = render order). Whether an item actually shows is membership
    /// AND its hardware gate.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Section {
        /// Section key in the source TOML.
        pub key: String,
        /// Title rendered only in the tooltip (empty for panel sections).
        pub title: String,
        /// Item tokens in render order.
        pub items: Vec<String>,
    }

    /// A surface (panel or tooltip): an ordered list of [`Section`]s, plus
    /// the panel-only `glyphs` toggle.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Surface {
        /// Sections in render order.
        pub sections: Vec<Section>,
        /// PANEL only: show the label glyph next to each value.
        pub glyphs: bool,
    }

    impl Default for Surface {
        fn default() -> Self {
            // `glyphs` defaults to True (matches Python); the derive(Default)
            // would set it to false, so we provide a manual impl.
            Self {
                sections: Vec::new(),
                glyphs: true,
            }
        }
    }

    impl Surface {
        /// Returns `true` when `name` is a member of any section (i.e.
        /// enabled by config, before the hardware gate).
        ///
        /// Mirrors `Surface.has` in Python.
        #[must_use]
        pub fn has(&self, name: &str) -> bool {
            self.sections
                .iter()
                .any(|sec| sec.items.iter().any(|item| item == name))
        }

        /// Returns the set of all item tokens across every section.
        ///
        /// Mirrors `Surface.item_set` in Python. Returned as a sorted
        /// `BTreeSet` for stable comparison in tests (Python returns an
        /// unordered `set`, so this is a deliberately stricter view).
        #[must_use]
        pub fn item_set(&self) -> std::collections::BTreeSet<String> {
            self.sections
                .iter()
                .flat_map(|sec| sec.items.iter().cloned())
                .collect()
        }
    }
}

/// The fully resolved PiroStats configuration.
///
/// Mirrors the `Config` dataclass in `src/config.py`. Built by
/// [`load_config`] after the machine/orientation merge has produced the
/// final raw TOML table. The `machine` field records which machine block
/// (if any) matched; `vertical` records the resolved panel orientation.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Global display knobs.
    pub display: DisplayConfig,
    /// Bar form knobs on the panel.
    pub bar_panel: BarConfig,
    /// Column form knobs on the panel (the `:bar` form, horizontal).
    pub column_panel: ColumnConfig,
    /// Bar form knobs on the tooltip.
    pub bar_tooltip: BarConfig,
    /// Spark form knobs on the panel.
    pub spark_panel: SparkConfig,
    /// Spark form knobs on the tooltip.
    pub spark_tooltip: SparkConfig,
    /// Braille form knobs on the panel.
    pub braille_panel: BrailleConfig,
    /// Braille form knobs on the tooltip.
    pub braille_tooltip: BrailleConfig,
    /// Resolved panel surface.
    pub panel: Surface,
    /// Resolved tooltip surface.
    pub tooltip: Surface,
    /// Tooltip deep-dive pages.
    pub pages: PagesConfig,
    /// Color thresholds.
    pub thresholds: ThresholdConfig,
    /// Notification thresholds.
    pub notify_thresholds: NotifyThresholds,
    /// Notification enable flags.
    pub notifications: NotificationConfig,
    /// Glyph theme table (loaded from `style/icons.toml`).
    pub icons: Table,
    /// Label i18n table (loaded from `lang/<language>.toml`).
    pub labels: Table,
    /// Manual hwmon sensor specs.
    pub sensors: SensorOverrides,
    /// Disk discovery and SMART knobs.
    pub disks: DiskConfig,
    /// Peripheral battery hints.
    pub battery: BatteryConfig,
    /// System-updates checker file.
    pub system_updates: SystemUpdatesConfig,
    /// Server-reachability checker file.
    pub server_check: ServerCheckConfig,
    /// Matched machine block name (`""` if none matched).
    pub machine: String,
    /// Resolved panel orientation.
    pub vertical: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            display: DisplayConfig::default(),
            bar_panel: BarConfig::default(),
            column_panel: ColumnConfig::default(),
            bar_tooltip: BarConfig::default(),
            spark_panel: SparkConfig::default(),
            spark_tooltip: SparkConfig::default(),
            braille_panel: BrailleConfig::default(),
            braille_tooltip: BrailleConfig::default(),
            panel: Surface::default(),
            tooltip: Surface::default(),
            pages: PagesConfig::default(),
            thresholds: ThresholdConfig::default(),
            notify_thresholds: NotifyThresholds::default(),
            notifications: NotificationConfig::default(),
            icons: Table::new(),
            labels: Table::new(),
            sensors: SensorOverrides::default(),
            disks: DiskConfig::default(),
            battery: BatteryConfig::default(),
            system_updates: SystemUpdatesConfig::default(),
            server_check: ServerCheckConfig::default(),
            machine: String::new(),
            vertical: false,
        }
    }
}

// ── load_config ─────────────────────────────────────────────────────────────

/// Loads and fully resolves a PiroStats config.
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
mod tests {
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
        assert_eq!(d.poll_interval, 1.5);
        assert_eq!(d.history_interval, 1.5);
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
        assert_eq!(d.poll_interval, 3.0, "provided value overrides");
        assert_eq!(d.history_interval, 1.5, "missing field uses struct default");
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
        assert_eq!(d.smart_interval, 3600.0);
        assert_eq!(d.smart_interval_hdd, 21600.0);
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
        assert_eq!(d.poll_interval, 9.0);
        assert_eq!(d.language, "it");
        assert_eq!(
            d.history_interval, 1.5,
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
        let dir =
            std::env::temp_dir().join(format!("pirostats-config-{tag}-{}", std::process::id()));
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
}
