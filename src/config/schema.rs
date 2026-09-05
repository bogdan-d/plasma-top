//! Typed configuration schema and defaults.
//!
//! Types are re-exported from [`super`], preserving the public `config::*` paths.

use serde::Deserialize;
use toml::{Table, Value};

use super::{COLUMN_DIGIT_RATIO, Cadence, TOOLTIP_WIDTH_FLOOR};

/// Global display knobs: the daemon's two cadences, plus the inspection
/// aid. Mirrors `DisplayConfig` in `src/config.py`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    /// Rewrite of both panel and tooltip HTML, in seconds.
    pub poll_interval: Cadence,
    /// Sampling cadence of the shared history buffer (read by every
    /// spark/braille form and the graphs page).
    pub history_interval: Cadence,
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
            poll_interval: Cadence::from_millis(1500),
            history_interval: Cadence::from_millis(1500),
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
    /// AMDGPU color thresholds.
    pub gpu_amd_temp: [i32; 2],
    /// gpu_nvidia_usage thresholds.
    pub gpu_nvidia_usage: Vec<i32>,
    /// AMDGPU color thresholds.
    pub gpu_amd_usage: [i32; 2],
    /// gpu_nvidia_mem_usage thresholds.
    pub gpu_nvidia_mem_usage: Vec<i32>,
    /// AMDGPU color thresholds.
    pub gpu_amd_mem_usage: [i32; 2],
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
    /// Combined AMDGPU codec activity threshold.
    pub gpu_amd_codec_usage: i32,
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
            gpu_amd_temp: [50, 70],
            gpu_nvidia_usage: vec![50, 70],
            gpu_amd_usage: [50, 70],
            gpu_nvidia_mem_usage: vec![50, 70],
            gpu_amd_mem_usage: [50, 70],
            gpu_intel_usage: vec![50, 70],
            hd_temp: vec![50, 55],
            battery_sys: vec![20, 80],
            battery_mouse: vec![20, 80],
            battery_kbd: vec![20, 80],
            wifi_signal: vec![30, 60],
            gpu_nvidia_dec_usage: 1,
            gpu_amd_codec_usage: 1,
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
    /// AMDGPU edge-temperature notification threshold.
    pub gpu_amd_temp: i32,
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
            gpu_amd_temp: 80,
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
    /// AMDGPU edge-temperature notification enabled.
    pub gpu_amd_temp: bool,
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
            gpu_amd_temp: false,
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
                        serde::de::Error::custom("expected string or array of strings for `mounts`")
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
    /// SMART freshness budget for SSD/NVMe drives (seconds).
    pub smart_interval: Cadence,
    /// SMART freshness budget for rotational drives (seconds).
    pub smart_interval_hdd: Cadence,
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
            smart_interval: Cadence::from_millis(3_600_000),
            smart_interval_hdd: Cadence::from_millis(21_600_000),
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

/// The fully resolved PlasmaTop configuration.
///
/// Mirrors the `Config` dataclass in `src/config.py`. Built by [`super::load_config`] after the machine/orientation merge has produced the final raw TOML table. The `machine` field records which machine block (if any) matched; `vertical` records the resolved panel orientation.
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
