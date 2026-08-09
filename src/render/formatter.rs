//! Main panel/tooltip formatter and item dispatch.

mod battery;
mod connectivity;
mod metrics;
mod storage;
mod system;
mod width;

#[cfg(test)]
mod tests;

use crate::config::Config;
use crate::domain::{DisplaySnapshot, HardwareInventory, Metric};

use super::cells::{SSID_MAX, TOP_PROCESS_MIN_WIDTH, normalize_separators, separator_size};
use super::model::{
    Cell, Entry, Row, Separator, SeparatorSize, group_rows_into_blocks, render_row_inline,
};
use super::mono::{global_width_of, render_blocks_monospace};
use super::registry::{item_gate, resolve_item};

/// Formats panel and tooltip HTML from resolved config, hardware, and readings.
pub struct PanelFormatter<'a> {
    cfg: &'a Config,
    hw: &'a HardwareInventory,
    vertical: bool,
    now_unix: Option<u64>,
}

impl<'a> PanelFormatter<'a> {
    /// Creates a formatter using the current wall clock for panel battery alternation.
    #[must_use]
    pub fn new(cfg: &'a Config, hw: &'a HardwareInventory) -> Self {
        Self {
            cfg,
            hw,
            vertical: cfg.vertical,
            now_unix: None,
        }
    }

    /// Creates a formatter pinned to a fixed unix time for deterministic tests.
    #[must_use]
    pub fn with_now_unix(cfg: &'a Config, hw: &'a HardwareInventory, now_unix: u64) -> Self {
        Self {
            cfg,
            hw,
            vertical: cfg.vertical,
            now_unix: Some(now_unix),
        }
    }

    /// Formats the panel HTML for the current orientation.
    #[must_use]
    pub fn format_panel(&self, readings: &DisplaySnapshot, css: &str) -> String {
        let entries = self.build_entries(readings, false);
        let style = if css.is_empty() {
            String::new()
        } else {
            format!("<style>{css}</style>")
        };

        if self.vertical {
            let blocks = group_rows_into_blocks(entries);
            let min_width = self.cfg.display.panel_min_width.max(0) as usize;
            let body = render_blocks_monospace(&blocks, min_width);
            format!(r#"{style}<div class="panel panel-v">{body}</div>"#)
        } else {
            let mut parts = Vec::new();
            let mut pending = None;
            for entry in entries {
                match entry {
                    Entry::Separator(separator) => pending = Some(separator.size),
                    Entry::Row(row) => {
                        if !parts.is_empty() {
                            let class = match pending.take() {
                                Some(SeparatorSize::Small) => "separator-rule-small",
                                Some(SeparatorSize::Big) => "separator-rule-big",
                                None => "item-gap",
                            };
                            parts.push(format!(r#"<span class="gap {class}">&nbsp;</span>"#));
                        }
                        parts.push(render_row_inline(&row));
                    }
                }
            }
            format!(
                r#"{style}<div class="panel panel-h">{}</div>"#,
                parts.join("")
            )
        }
    }

    /// Formats the main tooltip HTML.
    #[must_use]
    pub fn format_tooltip(&self, readings: &DisplaySnapshot, css: &str) -> String {
        let entries = self.build_entries(readings, true);
        let blocks = group_rows_into_blocks(entries);
        let min_width = self.cfg.display.tooltip_width.max(0) as usize;
        let body = render_blocks_monospace(&blocks, min_width);
        self.wrap_tooltip(&body, css)
    }

    /// Returns the canonical tooltip width in monospace columns.
    #[must_use]
    pub fn canonical_width(&self, readings: &DisplaySnapshot) -> usize {
        let widened = self.maxed_readings(readings);
        let entries = self.build_entries(&widened, true);
        let blocks = group_rows_into_blocks(entries);
        let mut width = global_width_of(&blocks, 0);
        if self.cfg.pages.order.iter().any(|page| page == "processes") {
            width = width.max(TOP_PROCESS_MIN_WIDTH);
        }
        width
    }

    pub(crate) fn build_entries(&self, readings: &DisplaySnapshot, tooltip: bool) -> Vec<Entry> {
        let surface = if tooltip {
            &self.cfg.tooltip
        } else {
            &self.cfg.panel
        };

        let mut entries = Vec::new();
        let mut any_section_rendered = false;
        for section in &surface.sections {
            let mut section_entries = Vec::new();
            let mut has_rows = false;
            for name in &section.items {
                if let Some(size) = separator_size(name) {
                    section_entries.push(Entry::Separator(Separator { size }));
                    continue;
                }

                let Some(resolved) = resolve_item(name, self.vertical) else {
                    continue;
                };
                if !item_gate(self.cfg, self.hw, &resolved.token, readings) {
                    continue;
                }

                let rows = self.render_resolved(resolved, readings, tooltip);
                if !rows.is_empty() {
                    has_rows = true;
                    section_entries.extend(rows.into_iter().map(Entry::Row));
                }
            }

            if !has_rows {
                continue;
            }
            if tooltip && any_section_rendered {
                entries.push(Entry::Separator(Separator {
                    size: SeparatorSize::Big,
                }));
            }
            if tooltip && !section.title.is_empty() {
                entries.push(Entry::Row(vec![Cell::classified(
                    section.title.clone(),
                    "title",
                )]));
                entries.push(Entry::Row(vec![Cell::classified("", "title-rule")]));
            }
            entries.extend(section_entries);
            any_section_rendered = true;
        }

        normalize_separators(entries)
    }

    fn wrap_tooltip(&self, body: &str, css: &str) -> String {
        let style = if css.is_empty() {
            String::new()
        } else {
            format!("<style>{css}</style>")
        };
        format!(r#"{style}<div class="tooltip">{body}</div>"#)
    }

    fn render_resolved(
        &self,
        resolved: super::registry::ResolvedItem,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let metric = resolved.token.metric();
        match metric {
            Metric::CpuUsage | Metric::MemUsage => {
                self.render_historied(metric, resolved.form_token, readings, tooltip)
            }
            Metric::SwapUsage => self.render_percent_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.swap_usage,
                (
                    &self.cfg.thresholds.swap_usage[0],
                    &self.cfg.thresholds.swap_usage[1],
                ),
                true,
            ),
            Metric::CpuTemp => self.render_temp_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.cpu_temp,
                (
                    &self.cfg.thresholds.cpu_temp[0],
                    &self.cfg.thresholds.cpu_temp[1],
                ),
            ),
            Metric::CpuFreq => self.render_cpu_freq_row(resolved.form_token, readings, tooltip),
            Metric::CpuTurbo => self.render_cpu_turbo_row(resolved.form_token, readings, tooltip),
            Metric::HdTemp => self.render_hd_temp(resolved.form_token, readings, tooltip),
            Metric::DiskUsage => self.render_disk_usage(resolved.form_token, readings, tooltip),
            Metric::DiskSmart => self.render_disk_smart(readings, tooltip),
            Metric::GpuNvidiaTemp => self.render_temp_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.gpu_temp,
                (
                    &self.cfg.thresholds.gpu_nvidia_temp[0],
                    &self.cfg.thresholds.gpu_nvidia_temp[1],
                ),
            ),
            Metric::GpuNvidiaUsage => self.render_percent_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.gpu_usage,
                (
                    &self.cfg.thresholds.gpu_nvidia_usage[0],
                    &self.cfg.thresholds.gpu_nvidia_usage[1],
                ),
                true,
            ),
            Metric::GpuNvidiaMemUsage => self.render_percent_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.gpu_mem,
                (
                    &self.cfg.thresholds.gpu_nvidia_mem_usage[0],
                    &self.cfg.thresholds.gpu_nvidia_mem_usage[1],
                ),
                true,
            ),
            Metric::GpuNvidiaDecoderUsage => self.render_active_percent_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.gpu_dec,
                self.cfg.thresholds.gpu_nvidia_dec_usage,
                true,
            ),
            Metric::GpuNvidiaFanSpeed => {
                self.render_gpu_fan_row(resolved.form_token, readings, tooltip)
            }
            Metric::GpuIntelFreq => self.render_freq_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.gpu_intel_freq,
            ),
            Metric::GpuIntelUsage => self.render_percent_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.gpu_intel_usage,
                (
                    &self.cfg.thresholds.gpu_intel_usage[0],
                    &self.cfg.thresholds.gpu_intel_usage[1],
                ),
                true,
            ),
            Metric::GpuIntelDecoderUsage => self.render_active_percent_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.gpu_intel_dec_usage,
                self.cfg.thresholds.gpu_intel_dec_usage,
                true,
            ),
            Metric::ScreenBrightness => self.render_plain_percent_row(
                metric.as_str(),
                resolved.form_token,
                tooltip,
                readings.screen_brightness,
            ),
            Metric::FanSpeed => self.render_fan_speed(resolved.form_token, readings, tooltip),
            Metric::BatterySystem => self.render_battery_sys(readings, tooltip),
            Metric::BatteryMouse => {
                vec![self.render_battery_peripheral(
                    metric.as_str(),
                    resolved.form_token,
                    readings.battery_mouse.as_ref(),
                    &self.cfg.thresholds.battery_mouse,
                    tooltip,
                )]
            }
            Metric::BatteryKeyboard => {
                vec![self.render_battery_peripheral(
                    metric.as_str(),
                    resolved.form_token,
                    readings.battery_kbd.as_ref(),
                    &self.cfg.thresholds.battery_kbd,
                    tooltip,
                )]
            }
            Metric::NetSpeed => self.render_dual_rate_rows(
                "net_speed_up",
                readings.net_up_bps,
                "net_speed_down",
                readings.net_down_bps,
                tooltip,
            ),
            Metric::DiskIo => self.render_dual_rate_rows(
                "disk_io_read",
                readings.disk_read_bps,
                "disk_io_write",
                readings.disk_write_bps,
                tooltip,
            ),
            Metric::NetDevice => self.render_string_row(
                metric.as_str(),
                resolved.form_token,
                readings.net_device.as_deref(),
                None,
                tooltip,
            ),
            Metric::NetIp => self.render_string_row(
                metric.as_str(),
                resolved.form_token,
                readings.ip_address.as_deref(),
                None,
                tooltip,
            ),
            Metric::NetDeviceIp => self.render_net_device_ip(readings, tooltip),
            Metric::WifiSsid => self.render_string_row(
                metric.as_str(),
                resolved.form_token,
                readings.wifi_ssid.as_deref(),
                Some(SSID_MAX),
                tooltip,
            ),
            Metric::WifiSignal => self.render_wifi_signal(resolved.form_token, readings, tooltip),
            Metric::WifiSsidSignal => self.render_wifi_ssid_signal(readings, tooltip),
            Metric::Uptime => vec![self.render_uptime(resolved.form_token, readings, tooltip)],
            Metric::LoadAverage => {
                vec![self.render_load_average(resolved.form_token, readings, tooltip)]
            }
            Metric::TopProcess => self.render_top_process(resolved.form_token, readings, tooltip),
            Metric::SystemUpdates => {
                vec![self.render_system_updates(resolved.form_token, readings, tooltip)]
            }
            Metric::ServerCheck => {
                vec![self.render_server_check(resolved.form_token, readings, tooltip)]
            }
        }
    }
}

fn collect_row(cells: Vec<Option<Cell>>) -> Vec<Row> {
    let row: Row = cells.into_iter().flatten().collect();
    if row.is_empty() {
        Vec::new()
    } else {
        vec![row]
    }
}
