//! Main panel/tooltip formatter and item dispatch.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::Config;
use crate::domain::{
    BatteryPeripheralReading, BatteryState, BatterySystemReading, HardwareSnapshot, LoadAverage,
    Metric, ReadingsSnapshot,
};

use super::cells::{
    BATTERY_ALTERNATE_SECONDS, NETDEV_MAX, SSID_MAX, TEMP_SCALE, TOP_PROCESS_MIN_WIDTH, disk_label,
    fmt_disk_space, fmt_freq, hd_label, label_cell, middle_ellipsis, net_fmt, normalize_separators,
    regular_label_cell, separator_size, table_text,
};
use super::model::{
    Cell, Entry, Ident, PERCENT_PANEL_WIDTH, Row, Separator, SeparatorSize, auxiliary_cell,
    css_class_active, css_class_battery, css_class_from_thresholds, format_percent,
    group_rows_into_blocks, render_row_inline, render_three_col_row, render_two_pair_row,
    value_cell,
};
use super::mono::{global_width_of, render_blocks_monospace};
use super::registry::{item_gate, resolve_item, trace_metric};
use super::traces::{
    bar_braille_row, bar_row, bar_spark_row, braille_html, braille_row, column_row, spark_html,
    spark_row,
};

/// Formats panel and tooltip HTML from resolved config, hardware, and readings.
pub struct PanelFormatter<'a> {
    cfg: &'a Config,
    hw: &'a HardwareSnapshot,
    vertical: bool,
    now_unix: Option<u64>,
}

impl<'a> PanelFormatter<'a> {
    /// Creates a formatter using the current wall clock for panel battery alternation.
    #[must_use]
    pub fn new(cfg: &'a Config, hw: &'a HardwareSnapshot) -> Self {
        Self {
            cfg,
            hw,
            vertical: cfg.vertical,
            now_unix: None,
        }
    }

    /// Creates a formatter pinned to a fixed unix time for deterministic tests.
    #[must_use]
    pub fn with_now_unix(cfg: &'a Config, hw: &'a HardwareSnapshot, now_unix: u64) -> Self {
        Self {
            cfg,
            hw,
            vertical: cfg.vertical,
            now_unix: Some(now_unix),
        }
    }

    /// Formats the panel HTML for the current orientation.
    #[must_use]
    pub fn format_panel(&self, readings: &ReadingsSnapshot, css: &str) -> String {
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
    pub fn format_tooltip(&self, readings: &ReadingsSnapshot, css: &str) -> String {
        let entries = self.build_entries(readings, true);
        let blocks = group_rows_into_blocks(entries);
        let min_width = self.cfg.display.tooltip_width.max(0) as usize;
        let body = render_blocks_monospace(&blocks, min_width);
        self.wrap_tooltip(&body, css)
    }

    /// Returns the canonical tooltip width in monospace columns.
    #[must_use]
    pub fn canonical_width(&self, readings: &ReadingsSnapshot) -> usize {
        let widened = self.maxed_readings(readings);
        let entries = self.build_entries(&widened, true);
        let blocks = group_rows_into_blocks(entries);
        let mut width = global_width_of(&blocks, 0);
        if self.cfg.pages.order.iter().any(|page| page == "processes") {
            width = width.max(TOP_PROCESS_MIN_WIDTH);
        }
        width
    }

    pub(crate) fn build_entries(&self, readings: &ReadingsSnapshot, tooltip: bool) -> Vec<Entry> {
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
        readings: &ReadingsSnapshot,
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

    fn render_historied(
        &self,
        metric: Metric,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let Some(trace_metric) = trace_metric(metric) else {
            return Vec::new();
        };
        let (value, history, thresholds, metric_name) = match metric {
            Metric::CpuUsage => (
                readings.cpu_usage,
                Some(readings.cpu_history.as_slice()),
                (
                    &self.cfg.thresholds.cpu_usage[0],
                    &self.cfg.thresholds.cpu_usage[1],
                ),
                metric.as_str(),
            ),
            Metric::MemUsage => (
                readings.mem_usage,
                Some(readings.mem_history.as_slice()),
                (
                    &self.cfg.thresholds.mem_usage[0],
                    &self.cfg.thresholds.mem_usage[1],
                ),
                metric.as_str(),
            ),
            _ => unreachable!("non-historied metric routed to render_historied"),
        };

        match resolved_form(metric, form_token) {
            HistoriedForm::Value => {
                let mut cells = Vec::new();
                cells.push(regular_label_cell(
                    self.cfg,
                    metric_name,
                    form_token,
                    tooltip,
                    None,
                    None,
                ));
                if metric == Metric::MemUsage {
                    cells.push(self.mem_space_cell(metric_name, form_token, readings, tooltip));
                }
                cells.push(Some(self.percent_value_cell(
                    metric_name,
                    form_token,
                    tooltip,
                    value,
                    (*thresholds.0, *thresholds.1),
                    true,
                )));
                collect_row(cells)
            }
            HistoriedForm::Bar => {
                if self.vertical {
                    bar_row(
                        self.cfg,
                        value,
                        (*thresholds.0, *thresholds.1),
                        tooltip,
                        &Ident::new(metric_name, form_token),
                    )
                } else {
                    column_row(
                        self.cfg,
                        value,
                        (*thresholds.0, *thresholds.1),
                        &Ident::new(metric_name, form_token),
                    )
                }
            }
            HistoriedForm::Spark => spark_row(
                self.cfg,
                history,
                trace_metric,
                tooltip,
                &Ident::new(metric_name, form_token),
            ),
            HistoriedForm::Braille => braille_row(
                self.cfg,
                history,
                trace_metric,
                tooltip,
                &Ident::new(metric_name, form_token),
            ),
            HistoriedForm::SparkValue => collect_row(vec![
                regular_label_cell(self.cfg, metric_name, form_token, tooltip, None, None),
                Some(self.spark_aux_cell(metric_name, form_token, history, trace_metric, tooltip)),
                Some(self.percent_value_cell(
                    metric_name,
                    form_token,
                    tooltip,
                    value,
                    (*thresholds.0, *thresholds.1),
                    true,
                )),
            ]),
            HistoriedForm::BrailleValue => collect_row(vec![
                regular_label_cell(self.cfg, metric_name, form_token, tooltip, None, None),
                Some(self.braille_aux_cell(
                    metric_name,
                    form_token,
                    history,
                    trace_metric,
                    tooltip,
                    None,
                )),
                Some(self.percent_value_cell(
                    metric_name,
                    form_token,
                    tooltip,
                    value,
                    (*thresholds.0, *thresholds.1),
                    true,
                )),
            ]),
            HistoriedForm::BarSpark => bar_spark_row(
                self.cfg,
                trace_metric,
                value,
                (*thresholds.0, *thresholds.1),
                history,
                tooltip,
            ),
            HistoriedForm::BarBraille => bar_braille_row(
                self.cfg,
                trace_metric,
                value,
                (*thresholds.0, *thresholds.1),
                history,
                tooltip,
            ),
        }
    }

    fn render_percent_row(
        &self,
        metric: &str,
        form_token: Option<&str>,
        tooltip: bool,
        value: Option<i32>,
        thresholds: (&i32, &i32),
        regular_label: bool,
    ) -> Vec<Row> {
        let label = if regular_label {
            regular_label_cell(self.cfg, metric, form_token, tooltip, None, None)
        } else {
            Some(label_cell(
                self.cfg, metric, form_token, tooltip, None, None,
            ))
        };
        collect_row(vec![
            label,
            Some(self.percent_value_cell(
                metric,
                form_token,
                tooltip,
                value,
                (*thresholds.0, *thresholds.1),
                true,
            )),
        ])
    }

    fn render_plain_percent_row(
        &self,
        metric: &str,
        form_token: Option<&str>,
        tooltip: bool,
        value: Option<i32>,
    ) -> Vec<Row> {
        collect_row(vec![
            Some(label_cell(
                self.cfg, metric, form_token, tooltip, None, None,
            )),
            Some(self.plain_percent_value_cell(metric, form_token, tooltip, value)),
        ])
    }

    fn render_active_percent_row(
        &self,
        metric: &str,
        form_token: Option<&str>,
        tooltip: bool,
        value: Option<i32>,
        threshold: i32,
        regular_label: bool,
    ) -> Vec<Row> {
        let label = if regular_label {
            regular_label_cell(self.cfg, metric, form_token, tooltip, None, None)
        } else {
            Some(label_cell(
                self.cfg, metric, form_token, tooltip, None, None,
            ))
        };
        collect_row(vec![
            label,
            Some(self.active_percent_value_cell(metric, form_token, tooltip, value, threshold)),
        ])
    }

    fn render_temp_row(
        &self,
        metric: &str,
        form_token: Option<&str>,
        tooltip: bool,
        value: Option<i32>,
        thresholds: (&i32, &i32),
    ) -> Vec<Row> {
        collect_row(vec![
            regular_label_cell(self.cfg, metric, form_token, tooltip, None, None),
            Some(self.temp_value_cell(
                metric,
                form_token,
                tooltip,
                value,
                (*thresholds.0, *thresholds.1),
            )),
        ])
    }

    fn render_freq_row(
        &self,
        metric: &str,
        form_token: Option<&str>,
        tooltip: bool,
        value: Option<i32>,
    ) -> Vec<Row> {
        collect_row(vec![
            regular_label_cell(self.cfg, metric, form_token, tooltip, None, None),
            Some(value_cell(
                fmt_freq(value.map(f64::from), tooltip),
                None,
                Some(&Ident::new(metric, form_token)),
                0,
            )),
        ])
    }

    fn render_cpu_freq_row(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let ident = Ident::new(Metric::CpuFreq.as_str(), form_token);
        let (text, class) = match readings.cpu_turbo {
            Some(true) => ("Turbo", Some("active")),
            Some(false) => ("Slow", Some("deactive")),
            None => ("", None),
        };
        let mut aux = auxiliary_cell(text, class, Some(&ident), 0, 0, None);
        if !aux.text.is_empty() {
            aux.pad_left = 1;
        }
        collect_row(vec![
            regular_label_cell(
                self.cfg,
                Metric::CpuFreq.as_str(),
                form_token,
                tooltip,
                None,
                None,
            ),
            Some(aux),
            Some(value_cell(
                fmt_freq(readings.cpu_freq_mhz, tooltip),
                None,
                Some(&ident),
                0,
            )),
        ])
    }

    fn render_cpu_turbo_row(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let (text, class) = match readings.cpu_turbo {
            Some(true) => ("on", Some("active")),
            Some(false) => ("off", Some("crit")),
            None => (super::model::EMPTY_VALUE, None),
        };
        collect_row(vec![
            regular_label_cell(
                self.cfg,
                Metric::CpuTurbo.as_str(),
                form_token,
                tooltip,
                None,
                None,
            ),
            Some(value_cell(
                text,
                class,
                Some(&Ident::new(Metric::CpuTurbo.as_str(), form_token)),
                0,
            )),
        ])
    }

    fn render_gpu_fan_row(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let text = match readings.gpu_fan {
            None => String::from(super::model::EMPTY_VALUE),
            Some(0) => String::from("off"),
            Some(value) => format_percent(i64::from(value), tooltip),
        };
        collect_row(vec![
            regular_label_cell(
                self.cfg,
                Metric::GpuNvidiaFanSpeed.as_str(),
                form_token,
                tooltip,
                None,
                None,
            ),
            Some(value_cell(
                text,
                None,
                Some(&Ident::new(Metric::GpuNvidiaFanSpeed.as_str(), form_token)),
                PERCENT_PANEL_WIDTH,
            )),
        ])
    }

    fn render_hd_temp(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        if form_token == Some("pair") {
            return self.render_hd_temp_pair(readings, tooltip);
        }

        let mut rows = Vec::new();
        for label in self.hw.hd_temp_paths.keys() {
            let Some(value) = readings.hd_temps.get(label).copied().flatten() else {
                continue;
            };
            rows.extend(collect_row(vec![
                regular_label_cell(
                    self.cfg,
                    Metric::HdTemp.as_str(),
                    form_token,
                    tooltip,
                    Some(&format!(
                        "{} {}",
                        table_text(&self.cfg.labels, Metric::HdTemp.as_str()),
                        hd_label(label)
                    )),
                    None,
                ),
                Some(self.temp_value_cell(
                    Metric::HdTemp.as_str(),
                    form_token,
                    tooltip,
                    Some(value),
                    (
                        self.cfg.thresholds.hd_temp[0],
                        self.cfg.thresholds.hd_temp[1],
                    ),
                )),
            ]));
        }
        rows
    }

    fn render_disk_usage(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let used_width = readings
            .disk_usage
            .values()
            .filter_map(|usage| {
                usage
                    .as_ref()
                    .map(|u| format!("{}G", u.used_gib).chars().count())
            })
            .max()
            .unwrap_or(0);
        let total_width = readings
            .disk_usage
            .values()
            .filter_map(|usage| {
                usage
                    .as_ref()
                    .map(|u| format!("{}G", u.total_gib).chars().count())
            })
            .max()
            .unwrap_or(0);

        let mut rows = Vec::new();
        for (mount, usage) in &readings.disk_usage {
            let percent = usage.as_ref().map(|u| u.percent);
            let used_class = percent.map(|value| {
                css_class_from_thresholds(
                    f64::from(value),
                    (
                        f64::from(self.cfg.thresholds.disk_usage[0]),
                        f64::from(self.cfg.thresholds.disk_usage[1]),
                    ),
                )
            });
            let ident = Ident::new(Metric::DiskUsage.as_str(), form_token);
            let mut cells = Vec::new();
            cells.push(regular_label_cell(
                self.cfg,
                Metric::DiskUsage.as_str(),
                form_token,
                tooltip,
                Some(&disk_label(mount)),
                None,
            ));
            if tooltip {
                let space = fmt_disk_space(
                    usage.as_ref().map(|u| u.used_gib),
                    usage.as_ref().map(|u| u.total_gib),
                    used_class,
                    used_width,
                    total_width,
                );
                cells.push(Some(auxiliary_cell(space, None, Some(&ident), 1, 0, None)));
            }
            cells.push(Some(self.percent_value_cell(
                Metric::DiskUsage.as_str(),
                form_token,
                tooltip,
                percent,
                (
                    self.cfg.thresholds.disk_usage[0],
                    self.cfg.thresholds.disk_usage[1],
                ),
                true,
            )));
            rows.extend(collect_row(cells));
        }
        rows
    }

    fn render_disk_smart(&self, readings: &ReadingsSnapshot, tooltip: bool) -> Vec<Row> {
        if !self.cfg.disks.smart {
            return Vec::new();
        }

        let mut labels: Vec<String> = self
            .hw
            .hd_temp_paths
            .keys()
            .chain(self.hw.disk_smart_drives.keys())
            .cloned()
            .collect();
        labels.sort();
        labels.dedup();

        let mut pairs = Vec::new();
        for label in labels {
            let Some(value) = readings.disk_smart.get(&label).copied().flatten() else {
                continue;
            };
            let ident = Ident::new(Metric::DiskSmart.as_str(), Some("pair"));
            let row = (
                label_cell(
                    self.cfg,
                    Metric::DiskSmart.as_str(),
                    Some("pair"),
                    tooltip,
                    Some(&hd_label(&label)),
                    None,
                ),
                value_cell(
                    if value { "OK" } else { "KO" },
                    Some(if value { "active" } else { "deactive" }),
                    Some(&ident),
                    0,
                ),
            );
            pairs.push(row);
        }
        pair_rows(pairs, Metric::DiskSmart.as_str(), Some("pair"))
    }

    fn render_fan_speed(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        if form_token == Some("pair") {
            return self.render_fan_speed_pair(readings, tooltip);
        }

        let mut rows = Vec::new();
        for key in self.hw.fan_paths.keys() {
            let text = match readings.fan_speeds.get(key).copied().flatten() {
                Some(0) => String::from("off"),
                Some(rpm) if tooltip => format!("{rpm} rpm"),
                Some(rpm) => rpm.to_string(),
                None => String::from(super::model::EMPTY_VALUE),
            };
            rows.extend(collect_row(vec![
                regular_label_cell(
                    self.cfg,
                    Metric::FanSpeed.as_str(),
                    form_token,
                    tooltip,
                    Some(&format!("Fan{key}")),
                    None,
                ),
                Some(value_cell(
                    text,
                    None,
                    Some(&Ident::new(Metric::FanSpeed.as_str(), form_token)),
                    0,
                )),
            ]));
        }
        rows
    }

    fn render_battery_sys(&self, readings: &ReadingsSnapshot, tooltip: bool) -> Vec<Row> {
        readings
            .battery_sys
            .iter()
            .enumerate()
            .map(|(index, battery)| self.render_one_battery_sys(battery, tooltip, index))
            .collect()
    }

    fn render_one_battery_sys(
        &self,
        battery: &BatterySystemReading,
        tooltip: bool,
        index: usize,
    ) -> Row {
        let metric = Metric::BatterySystem.as_str();
        let ident = Ident::new(metric, Some("value"));
        let percent = battery.charge_percent;
        let icon = self.battery_sys_icon(battery, percent);
        let label = label_cell(
            self.cfg,
            metric,
            Some("value"),
            tooltip,
            Some(&format!("Battery {index}")),
            Some(&icon),
        );

        let class = if self.battery_sys_is_full(battery, percent) {
            None
        } else {
            Some(css_class_battery(
                i64::from(percent),
                i64::from(self.cfg.thresholds.battery_sys[0]),
                i64::from(self.cfg.thresholds.battery_sys[1]),
            ))
        };

        let rate_text = if battery.rate_watts > 0 {
            match battery.state {
                BatteryState::Charging => format!("+{}W", battery.rate_watts),
                BatteryState::Discharging => format!("-{}W", battery.rate_watts),
                _ => format!("{}W", battery.rate_watts),
            }
        } else {
            String::new()
        };

        if tooltip {
            let mut extra_parts = Vec::new();
            if !rate_text.is_empty() {
                extra_parts.push(rate_text);
            }
            if let Some(limit) = battery
                .charge_limit_percent
                .filter(|limit| percent >= *limit)
            {
                extra_parts.push(format!(
                    "{} {limit}%",
                    table_text(&self.cfg.icons, "battery_sys_limit"),
                ));
            }
            return render_three_col_row(
                label,
                auxiliary_cell(extra_parts.join(" "), None, Some(&ident), 0, 0, None),
                value_cell(
                    format_percent(i64::from(percent), true),
                    class,
                    Some(&ident),
                    0,
                ),
            );
        }

        let value = if !rate_text.is_empty()
            && ((self.unix_seconds() / BATTERY_ALTERNATE_SECONDS) % 2 == 0)
        {
            rate_text
        } else {
            format_percent(i64::from(percent), false)
        };
        vec![label, value_cell(value, class, Some(&ident), 0)]
    }

    fn render_battery_peripheral(
        &self,
        metric: &str,
        form_token: Option<&str>,
        battery: Option<&BatteryPeripheralReading>,
        thresholds: &[i32],
        tooltip: bool,
    ) -> Row {
        let default_label = label_cell(self.cfg, metric, form_token, tooltip, None, None);
        let Some(battery) = battery else {
            return vec![
                default_label,
                value_cell(
                    super::model::EMPTY_VALUE,
                    None,
                    Some(&Ident::new(metric, form_token)),
                    0,
                ),
            ];
        };

        let label = if tooltip && !battery.name.is_empty() {
            label_cell(
                self.cfg,
                metric,
                form_token,
                tooltip,
                Some(&battery.name),
                None,
            )
        } else {
            default_label
        };

        let class = if battery.charge_percent >= 100 {
            None
        } else {
            Some(css_class_battery(
                i64::from(battery.charge_percent),
                i64::from(thresholds[0]),
                i64::from(thresholds[1]),
            ))
        };
        vec![
            label,
            value_cell(
                format_percent(i64::from(battery.charge_percent), tooltip),
                class,
                Some(&Ident::new(metric, form_token)),
                0,
            ),
        ]
    }

    fn render_dual_rate_rows(
        &self,
        first_metric: &str,
        first_value: Option<u64>,
        second_metric: &str,
        second_value: Option<u64>,
        tooltip: bool,
    ) -> Vec<Row> {
        let first_ident = Ident::new(first_metric, None);
        let second_ident = Ident::new(second_metric, None);
        let first_label = label_cell(self.cfg, first_metric, None, tooltip, None, None);
        let second_label = label_cell(self.cfg, second_metric, None, tooltip, None, None);
        let mut first_rate = value_cell(
            net_fmt(first_value.unwrap_or(0)),
            None,
            Some(&first_ident),
            0,
        );
        let second_rate = value_cell(
            net_fmt(second_value.unwrap_or(0)),
            None,
            Some(&second_ident),
            0,
        );

        if tooltip || !self.vertical {
            first_rate.pad_right += 2;
            return vec![render_two_pair_row(
                first_label,
                first_rate,
                second_label,
                second_rate,
            )];
        }

        vec![
            vec![first_label, first_rate],
            vec![second_label, second_rate],
        ]
    }

    fn render_string_row(
        &self,
        metric: &str,
        form_token: Option<&str>,
        value: Option<&str>,
        cap: Option<usize>,
        tooltip: bool,
    ) -> Vec<Row> {
        let text = value.map_or_else(
            || String::from(super::model::EMPTY_VALUE),
            |value| cap.map_or_else(|| value.to_owned(), |cap| middle_ellipsis(value, cap)),
        );
        vec![vec![
            label_cell(self.cfg, metric, form_token, tooltip, None, None),
            value_cell(text, None, Some(&Ident::new(metric, form_token)), 0),
        ]]
    }

    fn render_wifi_signal(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let metric = Metric::WifiSignal.as_str();
        let class = readings.wifi_signal_percent.map(|value| {
            css_class_battery(
                i64::from(value),
                i64::from(self.cfg.thresholds.wifi_signal[0]),
                i64::from(self.cfg.thresholds.wifi_signal[1]),
            )
        });
        vec![vec![
            label_cell(self.cfg, metric, form_token, tooltip, None, None),
            value_cell(
                readings.wifi_signal_percent.map_or_else(
                    || String::from(super::model::EMPTY_VALUE),
                    |value| format_percent(i64::from(value), tooltip),
                ),
                class,
                Some(&Ident::new(metric, form_token)),
                PERCENT_PANEL_WIDTH,
            ),
        ]]
    }

    fn render_net_device_ip(&self, readings: &ReadingsSnapshot, tooltip: bool) -> Vec<Row> {
        let device = readings.net_device.as_deref().map_or_else(
            || String::from(super::model::EMPTY_VALUE),
            |device| middle_ellipsis(device, NETDEV_MAX),
        );
        let ip = readings
            .ip_address
            .clone()
            .unwrap_or_else(|| String::from(super::model::EMPTY_VALUE));
        vec![vec![
            label_cell(
                self.cfg,
                Metric::NetDeviceIp.as_str(),
                Some("value"),
                tooltip,
                None,
                None,
            ),
            value_cell(
                format!("\u{a0}{device} - {ip}"),
                None,
                Some(&Ident::new(Metric::NetDeviceIp.as_str(), Some("value"))),
                0,
            ),
        ]]
    }

    fn render_wifi_ssid_signal(&self, readings: &ReadingsSnapshot, tooltip: bool) -> Vec<Row> {
        let ssid = readings.wifi_ssid.as_deref().map_or_else(
            || String::from(super::model::EMPTY_VALUE),
            |ssid| middle_ellipsis(ssid, SSID_MAX),
        );
        let signal = readings.wifi_signal_percent.map_or_else(
            || String::from(super::model::EMPTY_VALUE),
            |value| {
                let class = css_class_battery(
                    i64::from(value),
                    i64::from(self.cfg.thresholds.wifi_signal[0]),
                    i64::from(self.cfg.thresholds.wifi_signal[1]),
                );
                format!(
                    r#"<span class="{class}">{}</span>"#,
                    format_percent(i64::from(value), tooltip)
                )
            },
        );
        vec![vec![
            label_cell(
                self.cfg,
                Metric::WifiSsidSignal.as_str(),
                Some("value"),
                tooltip,
                None,
                None,
            ),
            value_cell(
                format!("{ssid} - {signal}"),
                None,
                Some(&Ident::new(Metric::WifiSsidSignal.as_str(), Some("value"))),
                0,
            ),
        ]]
    }

    fn render_uptime(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Row {
        let metric = Metric::Uptime.as_str();
        let text = readings.uptime_seconds.map_or_else(
            || String::from(super::model::EMPTY_VALUE),
            |seconds| {
                let days = seconds / 86_400;
                let rem = seconds % 86_400;
                let hours = rem / 3600;
                let minutes = (rem / 60) % 60;
                let mut parts = Vec::new();
                if days > 0 {
                    parts.push(format!("{days}d"));
                }
                parts.push(format!("{hours}h"));
                parts.push(format!("{minutes}m"));
                parts.join(" ")
            },
        );
        vec![
            label_cell(self.cfg, metric, form_token, tooltip, None, None),
            value_cell(text, None, Some(&Ident::new(metric, form_token)), 0),
        ]
    }

    fn render_load_average(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Row {
        let metric = Metric::LoadAverage.as_str();
        let text = readings.load_average.map_or_else(
            || String::from(super::model::EMPTY_VALUE),
            |load| {
                let cores = self.hw.cpu_count.max(1) as f64;
                let one = color_span(
                    load.one,
                    css_class_from_thresholds(
                        load.one / cores,
                        (
                            self.cfg.thresholds.load_avg_1[0],
                            self.cfg.thresholds.load_avg_1[1],
                        ),
                    ),
                );
                let five = color_span(
                    load.five,
                    css_class_from_thresholds(
                        load.five / cores,
                        (
                            self.cfg.thresholds.load_avg_5[0],
                            self.cfg.thresholds.load_avg_5[1],
                        ),
                    ),
                );
                let fifteen = color_span(
                    load.fifteen,
                    css_class_from_thresholds(
                        load.fifteen / cores,
                        (
                            self.cfg.thresholds.load_avg_15[0],
                            self.cfg.thresholds.load_avg_15[1],
                        ),
                    ),
                );
                format!("{one} {five} {fifteen}")
            },
        );
        vec![
            label_cell(self.cfg, metric, form_token, tooltip, None, None),
            value_cell(text, None, Some(&Ident::new(metric, form_token)), 0),
        ]
    }

    fn render_top_process(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let mut rows = Vec::new();
        let max_len = self.cfg.display.top_process_name_max_len.max(0) as usize;
        for (index, process) in readings.top_process.iter().flatten().enumerate() {
            let name = if max_len > 0 && process.command.chars().count() > max_len {
                let mut truncated: String = process.command.chars().take(max_len - 1).collect();
                truncated.push('…');
                truncated
            } else {
                process.command.clone()
            };
            let label = label_cell(
                self.cfg,
                Metric::TopProcess.as_str(),
                form_token,
                tooltip,
                Some(&format!("Top {}", index + 1)),
                None,
            );
            let name_cell = auxiliary_cell(
                name,
                None,
                Some(&Ident::new(Metric::TopProcess.as_str(), form_token)),
                0,
                0,
                None,
            );
            let value = value_cell(
                format!("{}%", process.cpu_percent),
                None,
                Some(&Ident::new(Metric::TopProcess.as_str(), form_token)),
                0,
            );
            rows.push(render_three_col_row(label, name_cell, value));
        }
        rows
    }

    fn render_system_updates(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Row {
        let metric = Metric::SystemUpdates.as_str();
        let class = readings
            .system_updates
            .filter(|count| *count >= 1)
            .map(|_| "crit");
        vec![
            label_cell(self.cfg, metric, form_token, tooltip, None, None),
            value_cell(
                readings.system_updates.map_or_else(
                    || String::from(super::model::EMPTY_VALUE),
                    |value| value.to_string(),
                ),
                class,
                Some(&Ident::new(metric, form_token)),
                0,
            ),
        ]
    }

    fn render_server_check(
        &self,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Row {
        let metric = Metric::ServerCheck.as_str();
        let (text, class) = match readings.server_ok {
            Some(true) => (String::from("Ok"), None),
            Some(false) => (String::from("KO"), Some("crit")),
            None => (String::from(super::model::EMPTY_VALUE), None),
        };
        vec![
            label_cell(self.cfg, metric, form_token, tooltip, None, None),
            value_cell(text, class, Some(&Ident::new(metric, form_token)), 0),
        ]
    }

    fn render_hd_temp_pair(&self, readings: &ReadingsSnapshot, tooltip: bool) -> Vec<Row> {
        let mut pairs = Vec::new();
        for label in self.hw.hd_temp_paths.keys() {
            let Some(value) = readings.hd_temps.get(label).copied().flatten() else {
                continue;
            };
            let ident = Ident::new(Metric::HdTemp.as_str(), Some("pair"));
            pairs.push((
                label_cell(
                    self.cfg,
                    Metric::HdTemp.as_str(),
                    Some("pair"),
                    tooltip,
                    Some(&hd_label(label)),
                    None,
                ),
                value_cell(
                    if tooltip {
                        format!("{value}°{TEMP_SCALE}")
                    } else {
                        format!("{value}{TEMP_SCALE}")
                    },
                    Some(css_class_from_thresholds(
                        f64::from(value),
                        (
                            f64::from(self.cfg.thresholds.hd_temp[0]),
                            f64::from(self.cfg.thresholds.hd_temp[1]),
                        ),
                    )),
                    Some(&ident),
                    0,
                ),
            ));
        }
        pair_rows(pairs, Metric::HdTemp.as_str(), Some("pair"))
    }

    fn render_fan_speed_pair(&self, readings: &ReadingsSnapshot, tooltip: bool) -> Vec<Row> {
        let mut pairs = Vec::new();
        for key in self.hw.fan_paths.keys() {
            let Some(value) = readings.fan_speeds.get(key).copied().flatten() else {
                continue;
            };
            let ident = Ident::new(Metric::FanSpeed.as_str(), Some("pair"));
            let text = if value == 0 {
                String::from("off")
            } else {
                value.to_string()
            };
            pairs.push((
                label_cell(
                    self.cfg,
                    Metric::FanSpeed.as_str(),
                    Some("pair"),
                    tooltip,
                    Some(&format!("Fan{key}")),
                    None,
                ),
                value_cell(text, None, Some(&ident), 0),
            ));
        }
        pair_rows(pairs, Metric::FanSpeed.as_str(), Some("pair"))
    }

    fn spark_aux_cell(
        &self,
        metric: &str,
        form_token: Option<&str>,
        history: Option<&[i32]>,
        trace_metric: super::traces::TraceMetric,
        tooltip: bool,
    ) -> Cell {
        let html = spark_html(self.cfg, history, trace_metric, tooltip);
        let mut cell = auxiliary_cell(
            html,
            None,
            Some(&Ident::new(metric, form_token)),
            0,
            0,
            None,
        );
        if !cell.text.is_empty() {
            cell.pad_left = 1;
        }
        cell
    }

    fn braille_aux_cell(
        &self,
        metric: &str,
        form_token: Option<&str>,
        history: Option<&[i32]>,
        trace_metric: super::traces::TraceMetric,
        tooltip: bool,
        chars: Option<usize>,
    ) -> Cell {
        let html = braille_html(self.cfg, history, trace_metric, tooltip, chars);
        let mut cell = auxiliary_cell(
            html,
            None,
            Some(&Ident::new(metric, form_token)),
            0,
            0,
            None,
        );
        if !cell.text.is_empty() {
            cell.pad_left = 1;
        }
        cell
    }

    fn percent_value_cell(
        &self,
        metric: &str,
        form_token: Option<&str>,
        tooltip: bool,
        value: Option<i32>,
        thresholds: (i32, i32),
        reserve_panel_width: bool,
    ) -> Cell {
        match value {
            Some(value) => value_cell(
                format_percent(i64::from(value), tooltip),
                Some(css_class_from_thresholds(
                    f64::from(value),
                    (f64::from(thresholds.0), f64::from(thresholds.1)),
                )),
                Some(&Ident::new(metric, form_token)),
                if reserve_panel_width {
                    PERCENT_PANEL_WIDTH
                } else {
                    0
                },
            ),
            None => value_cell(
                super::model::EMPTY_VALUE,
                None,
                Some(&Ident::new(metric, form_token)),
                0,
            ),
        }
    }

    fn plain_percent_value_cell(
        &self,
        metric: &str,
        form_token: Option<&str>,
        tooltip: bool,
        value: Option<i32>,
    ) -> Cell {
        match value {
            Some(value) => value_cell(
                format_percent(i64::from(value), tooltip),
                None,
                Some(&Ident::new(metric, form_token)),
                PERCENT_PANEL_WIDTH,
            ),
            None => value_cell(
                super::model::EMPTY_VALUE,
                None,
                Some(&Ident::new(metric, form_token)),
                0,
            ),
        }
    }

    fn active_percent_value_cell(
        &self,
        metric: &str,
        form_token: Option<&str>,
        tooltip: bool,
        value: Option<i32>,
        threshold: i32,
    ) -> Cell {
        match value {
            Some(value) => value_cell(
                format_percent(i64::from(value), tooltip),
                css_class_active(i64::from(value), i64::from(threshold)),
                Some(&Ident::new(metric, form_token)),
                PERCENT_PANEL_WIDTH,
            ),
            None => value_cell(
                super::model::EMPTY_VALUE,
                None,
                Some(&Ident::new(metric, form_token)),
                0,
            ),
        }
    }

    fn temp_value_cell(
        &self,
        metric: &str,
        form_token: Option<&str>,
        tooltip: bool,
        value: Option<i32>,
        thresholds: (i32, i32),
    ) -> Cell {
        match value {
            Some(value) => value_cell(
                if tooltip {
                    format!("{value}°{TEMP_SCALE}")
                } else {
                    format!("{value}{TEMP_SCALE}")
                },
                Some(css_class_from_thresholds(
                    f64::from(value),
                    (f64::from(thresholds.0), f64::from(thresholds.1)),
                )),
                Some(&Ident::new(metric, form_token)),
                0,
            ),
            None => value_cell(
                super::model::EMPTY_VALUE,
                None,
                Some(&Ident::new(metric, form_token)),
                0,
            ),
        }
    }

    fn mem_space_cell(
        &self,
        metric: &str,
        form_token: Option<&str>,
        readings: &ReadingsSnapshot,
        tooltip: bool,
    ) -> Option<Cell> {
        if !tooltip {
            return None;
        }
        let (Some(used_gib), Some(total_gib)) = (readings.mem_used_gib, readings.mem_total_gib)
        else {
            return None;
        };
        let used_class = readings.mem_usage.map(|usage| {
            css_class_from_thresholds(
                f64::from(usage),
                (
                    f64::from(self.cfg.thresholds.mem_usage[0]),
                    f64::from(self.cfg.thresholds.mem_usage[1]),
                ),
            )
        });
        Some(auxiliary_cell(
            fmt_disk_space(used_gib.into(), total_gib.into(), used_class, 0, 0),
            None,
            Some(&Ident::new(metric, form_token)),
            1,
            0,
            None,
        ))
    }

    fn battery_sys_is_full(&self, battery: &BatterySystemReading, percent: i32) -> bool {
        percent >= 100
            || battery.state == BatteryState::FullyCharged
            || battery
                .charge_limit_percent
                .is_some_and(|limit| percent >= limit)
    }

    fn battery_sys_icon(&self, battery: &BatterySystemReading, percent: i32) -> String {
        if battery.state == BatteryState::Charging {
            return table_text(&self.cfg.icons, "battery_sys_charging").to_owned();
        }
        if self.battery_sys_is_full(battery, percent) {
            return table_text(&self.cfg.icons, "battery_sys_full").to_owned();
        }
        let level = ((percent + 5) / 10 * 10).clamp(10, 90);
        table_text(&self.cfg.icons, &format!("battery_sys_{level}")).to_owned()
    }

    fn unix_seconds(&self) -> u64 {
        self.now_unix.unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        })
    }

    fn maxed_readings(&self, readings: &ReadingsSnapshot) -> ReadingsSnapshot {
        let mut widened = readings.clone();
        widened.cpu_usage = Some(100);
        widened.mem_usage = Some(100);
        widened.swap_usage = Some(100);
        widened.cpu_temp = Some(100);
        widened.gpu_temp = Some(100);
        widened.gpu_usage = Some(100);
        widened.gpu_mem = Some(100);
        widened.gpu_dec = Some(100);
        widened.gpu_fan = Some(100);
        widened.gpu_intel_usage = Some(100);
        widened.gpu_intel_dec_usage = Some(100);
        widened.gpu_intel_freq = Some(9999);
        widened.cpu_freq_mhz = Some(9999.0);
        widened.screen_brightness = Some(100);
        widened.wifi_signal_percent = Some(100);
        widened.net_up_bps = Some(999_000_000);
        widened.net_down_bps = Some(999_000_000);
        widened.disk_read_bps = Some(999_000_000);
        widened.disk_write_bps = Some(999_000_000);
        widened.ip_address = Some(String::from("255.255.255.255"));
        if widened.net_device.is_none() {
            widened.net_device = self.hw.net_device.clone();
        }
        if let Some(total) = widened.mem_total_gib {
            widened.mem_used_gib = Some(total);
        }
        for usage in widened.disk_usage.values_mut().flatten() {
            usage.percent = 100;
            usage.used_gib = usage.total_gib;
        }
        widened.hd_temps = self
            .hw
            .hd_temp_paths
            .keys()
            .cloned()
            .map(|label| (label, Some(100)))
            .collect();
        widened.fan_speeds = self
            .hw
            .fan_paths
            .keys()
            .cloned()
            .map(|label| (label, Some(9999)))
            .collect();
        widened.disk_smart = self
            .hw
            .disk_smart_drives
            .keys()
            .cloned()
            .map(|label| (label, Some(true)))
            .collect();
        for battery in &mut widened.battery_sys {
            battery.charge_percent = 100;
            battery.rate_watts = 99;
            battery.state = BatteryState::Discharging;
            if battery.charge_limit_percent.is_none() {
                battery.charge_limit_percent = Some(80);
            }
        }
        if let Some(mouse) = &mut widened.battery_mouse {
            mouse.charge_percent = 100;
        }
        if let Some(keyboard) = &mut widened.battery_kbd {
            keyboard.charge_percent = 100;
        }
        if let Some(processes) = &mut widened.top_process {
            for process in processes {
                process.command = "X".repeat(15);
                process.cpu_percent = 100;
            }
        }
        let cores = self.hw.cpu_count.max(1) as f64;
        widened.load_average = Some(LoadAverage {
            one: cores,
            five: cores,
            fifteen: cores,
        });
        widened.uptime_seconds = Some(999 * 86_400 + 23 * 3600 + 59 * 60);
        widened
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoriedForm {
    Value,
    Bar,
    Spark,
    Braille,
    SparkValue,
    BrailleValue,
    BarSpark,
    BarBraille,
}

fn resolved_form(metric: Metric, form_token: Option<&str>) -> HistoriedForm {
    match (metric, form_token) {
        (_, Some("bar")) | (_, Some("column")) => HistoriedForm::Bar,
        (_, Some("spark")) => HistoriedForm::Spark,
        (_, Some("braille")) => HistoriedForm::Braille,
        (_, Some("spark_value")) => HistoriedForm::SparkValue,
        (_, Some("braille_value")) => HistoriedForm::BrailleValue,
        (_, Some("bar_spark")) => HistoriedForm::BarSpark,
        (_, Some("bar_braille")) => HistoriedForm::BarBraille,
        (_, Some("value")) | (_, None) => HistoriedForm::Value,
        _ => unreachable!("unsupported historied form token"),
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

fn color_span(value: f64, class: &str) -> String {
    format!(r#"<span class="{class}">{value:.2}</span>"#)
}

fn pair_rows(mut pairs: Vec<(Cell, Cell)>, metric: &str, form_token: Option<&str>) -> Vec<Row> {
    if pairs.is_empty() {
        return Vec::new();
    }
    if pairs.len() == 1 {
        if let Some((label, value)) = pairs.pop() {
            return vec![vec![label, value]];
        }
        return Vec::new();
    }

    let blank_label = Cell::classified(
        "",
        format!("label {}", Ident::new(metric, form_token).css()),
    );
    let blank_value = value_cell("", None, Some(&Ident::new(metric, form_token)), 0);
    let mut rows = Vec::new();
    let mut index = 0;
    while index < pairs.len() {
        let (label1, mut value1) = pairs[index].clone();
        value1.pad_right += 2;
        let (label2, value2) = pairs
            .get(index + 1)
            .cloned()
            .unwrap_or_else(|| (blank_label.clone(), blank_value.clone()));
        rows.push(render_two_pair_row(label1, value1, label2, value2));
        index += 2;
    }
    rows
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::too_many_lines)]

    use super::*;
    use crate::config::{BarConfig, BrailleConfig, ColumnConfig, Surface as ConfigSurface};
    use crate::config::{Section, SparkConfig, apply_canonical_width, load_config};
    use crate::domain::{DiskUsageReading, SmartDisk, TopProcessSummary};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn bare_hw() -> HardwareSnapshot {
        HardwareSnapshot {
            net_device: Some(String::from("enp0s3")),
            disk_io_device: Some(String::from("sda2")),
            cpu_count: 2,
            ..HardwareSnapshot::default()
        }
    }

    fn full_hw() -> HardwareSnapshot {
        HardwareSnapshot {
            cpu_temp_path: Some(PathBuf::from("/x")),
            cpu_freq_path: Some(PathBuf::from("/x")),
            hd_temp_paths: BTreeMap::from([
                (String::from("nvme0"), PathBuf::from("/x")),
                (String::from("sda"), PathBuf::from("/x")),
            ]),
            fan_paths: BTreeMap::from([
                (String::from("1"), PathBuf::from("/x")),
                (String::from("2"), PathBuf::from("/x")),
            ]),
            battery_sys_ids: vec![String::from("/BAT0")],
            has_nvidia: true,
            intel_gpu_freq_path: Some(PathBuf::from("/x")),
            intel_gpu_pci: Some(String::from("0000:00:02.0")),
            net_device: Some(String::from("wlan0")),
            disk_io_device: Some(String::from("nvme0n1")),
            cpu_count: 8,
            cpu_turbo_supported: true,
            has_backlight: true,
            has_wifi: true,
            battery_mouse_id: Some(String::from("/m")),
            battery_kbd_id: Some(String::from("/k")),
            disk_smart_drives: BTreeMap::from([
                (
                    String::from("nvme0"),
                    SmartDisk {
                        object_path: String::from("/d0"),
                        interface: crate::domain::DiskSmartInterface::Nvme,
                        rotational: false,
                    },
                ),
                (
                    String::from("sda"),
                    SmartDisk {
                        object_path: String::from("/d1"),
                        interface: crate::domain::DiskSmartInterface::Ata,
                        rotational: true,
                    },
                ),
            ]),
            ..HardwareSnapshot::default()
        }
    }

    fn full_readings() -> ReadingsSnapshot {
        ReadingsSnapshot {
            cpu_usage: Some(73),
            cpu_temp: Some(55),
            cpu_freq_mhz: Some(3200.0),
            cpu_turbo: Some(true),
            cpu_history: [10, 20, 30, 40, 50, 60, 70, 80, 73, 65].repeat(2),
            mem_history: [15, 25, 35, 45, 55, 42, 38, 44, 50, 42].repeat(2),
            uptime_seconds: Some(123_456),
            load_average: Some(LoadAverage {
                one: 1.2,
                five: 0.9,
                fifteen: 0.7,
            }),
            top_process: Some(vec![
                TopProcessSummary {
                    command: String::from("plasmashell"),
                    cpu_percent: 12,
                },
                TopProcessSummary {
                    command: String::from("firefox"),
                    cpu_percent: 8,
                },
            ]),
            mem_usage: Some(42),
            mem_used_gib: Some(13),
            mem_total_gib: Some(32),
            swap_usage: Some(10),
            net_up_bps: Some(500_000),
            net_down_bps: Some(2_000_000),
            net_device: Some(String::from("wlan0")),
            ip_address: Some(String::from("192.168.1.5")),
            wifi_ssid: Some(String::from("MyWifi")),
            wifi_signal_percent: Some(80),
            disk_read_bps: Some(1_500_000),
            disk_write_bps: Some(800_000),
            disk_usage: BTreeMap::from([
                (
                    String::from("/"),
                    Some(DiskUsageReading {
                        percent: 50,
                        used_gib: 100,
                        total_gib: 200,
                    }),
                ),
                (
                    String::from("/mnt/data"),
                    Some(DiskUsageReading {
                        percent: 70,
                        used_gib: 700,
                        total_gib: 1000,
                    }),
                ),
            ]),
            disk_smart: BTreeMap::from([
                (String::from("nvme0"), Some(true)),
                (String::from("sda"), Some(true)),
            ]),
            hd_temps: BTreeMap::from([
                (String::from("nvme0"), Some(45)),
                (String::from("sda"), Some(50)),
            ]),
            fan_speeds: BTreeMap::from([
                (String::from("1"), Some(1200)),
                (String::from("2"), Some(0)),
            ]),
            battery_sys: vec![BatterySystemReading {
                id: String::from("/BAT0"),
                charge_percent: 80,
                rate_watts: 15,
                state: BatteryState::Discharging,
                charge_limit_percent: Some(80),
            }],
            battery_mouse: Some(BatteryPeripheralReading {
                name: String::from("Logi Mouse"),
                charge_percent: 90,
            }),
            battery_kbd: Some(BatteryPeripheralReading {
                name: String::from("Logi Kbd"),
                charge_percent: 85,
            }),
            gpu_temp: Some(60),
            gpu_usage: Some(30),
            gpu_mem: Some(40),
            gpu_dec: Some(5),
            gpu_fan: Some(25),
            gpu_intel_freq: Some(900),
            gpu_intel_usage: Some(20),
            gpu_intel_dec_usage: Some(2),
            screen_brightness: Some(75),
            system_updates: Some(3),
            server_ok: Some(true),
            ..ReadingsSnapshot::default()
        }
    }

    fn canonical_guard_cfg() -> Config {
        let mut cfg = Config::default();
        cfg.pages.order = Vec::new();
        cfg.tooltip = ConfigSurface {
            sections: vec![Section {
                key: String::from("io"),
                title: String::from("IO"),
                items: vec![
                    String::from("net_device_ip"),
                    String::from("wifi_ssid_signal"),
                    String::from("load_avg"),
                    String::from("uptime"),
                    String::from("cpu_freq"),
                    String::from("cpu_temp"),
                ],
            }],
            glyphs: true,
        };
        cfg
    }

    #[test]
    fn net_and_label_helpers_match_python_behavior() {
        assert_eq!(net_fmt(0), "0");
        assert_eq!(net_fmt(500_000), "500K");
        assert_eq!(net_fmt(2_500_000), "2M");
        assert_eq!(
            middle_ellipsis("FRITZ!Box 7590 Guest", SSID_MAX)
                .chars()
                .count(),
            SSID_MAX
        );
    }

    #[test]
    fn canonical_width_exceeds_short_content() {
        let cfg = canonical_guard_cfg();
        let hw = HardwareSnapshot {
            net_device: Some(String::from("wlan0")),
            has_wifi: true,
            cpu_temp_path: Some(PathBuf::from("/x")),
            cpu_count: 8,
            ..HardwareSnapshot::default()
        };
        let formatter = PanelFormatter::new(&cfg, &hw);
        let readings = ReadingsSnapshot {
            net_device: Some(String::from("wlan0")),
            ip_address: Some(String::from("10.0.0.1")),
            wifi_ssid: Some(String::from("Home")),
            wifi_signal_percent: Some(50),
            load_average: Some(LoadAverage {
                one: 0.1,
                five: 0.2,
                fifteen: 0.3,
            }),
            uptime_seconds: Some(60),
            cpu_freq_mhz: Some(800.0),
            cpu_temp: Some(40),
            ..ReadingsSnapshot::default()
        };
        let actual = global_width_of(
            &group_rows_into_blocks(formatter.build_entries(&readings, true)),
            0,
        );
        assert!(formatter.canonical_width(&readings) > actual);
    }

    #[test]
    fn canonical_width_covers_every_tooltip_item() {
        let hw = full_hw();
        let lo = full_readings();
        let hi =
            PanelFormatter::with_now_unix(&Config::default(), &hw, 1_000_000).maxed_readings(&lo);
        let tokens = crate::domain::list_items()
            .into_iter()
            .filter(|(_, placement)| *placement != "panel only")
            .map(|(token, _)| token)
            .collect::<Vec<_>>();

        let mut failures = Vec::new();
        for token in tokens {
            let mut cfg = Config::default();
            cfg.pages.order = Vec::new();
            cfg.tooltip = ConfigSurface {
                sections: vec![Section {
                    key: String::from("s"),
                    title: String::from("S"),
                    items: vec![token.clone()],
                }],
                glyphs: true,
            };
            let formatter = PanelFormatter::with_now_unix(&cfg, &hw, 1_000_000);
            let wide = global_width_of(
                &group_rows_into_blocks(formatter.build_entries(&hi, true)),
                0,
            );
            if wide == 0 {
                continue;
            }
            let canonical = formatter.canonical_width(&lo);
            if canonical < wide {
                failures.push(format!(
                    "{token}: canonical {canonical} < wide render {wide}"
                ));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn available_sections_collapse_and_panel_omits_titles() {
        let mut cfg = Config::default();
        let sections = vec![
            Section {
                key: String::from("live"),
                title: String::from("Live"),
                items: vec![String::from("cpu_usage"), String::from("mem_usage")],
            },
            Section {
                key: String::from("thermal"),
                title: String::from("Thermal"),
                items: vec![String::from("cpu_temp"), String::from("fan_speed")],
            },
            Section {
                key: String::from("load"),
                title: String::from("Load"),
                items: vec![String::from("uptime"), String::from("load_avg")],
            },
        ];
        cfg.tooltip = ConfigSurface {
            sections: sections.clone(),
            glyphs: true,
        };
        cfg.panel = ConfigSurface {
            sections,
            glyphs: true,
        };

        let hw = bare_hw();
        let formatter = PanelFormatter::new(&cfg, &hw);
        let readings = ReadingsSnapshot {
            cpu_usage: Some(10),
            mem_usage: Some(20),
            uptime_seconds: Some(60),
            load_average: Some(LoadAverage {
                one: 0.1,
                five: 0.2,
                fifteen: 0.3,
            }),
            ..ReadingsSnapshot::default()
        };

        let tooltip_entries = formatter.build_entries(&readings, true);
        let panel_entries = formatter.build_entries(&readings, false);
        let tooltip_titles = tooltip_entries
            .iter()
            .filter_map(|entry| match entry {
                Entry::Row(row)
                    if row.len() == 1 && row[0].css_class.as_deref() == Some("title") =>
                {
                    Some(row[0].text.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(
            tooltip_titles,
            vec![String::from("Live"), String::from("Load")]
        );
        assert!(!matches!(
            tooltip_entries.first(),
            Some(Entry::Separator(_))
        ));
        assert!(panel_entries.iter().all(|entry| {
            !matches!(entry, Entry::Separator(_))
                && !matches!(entry, Entry::Row(row) if row.len() == 1 && row[0].css_class.as_deref() == Some("title"))
        }));
    }

    #[test]
    fn tooltip_and_panel_goldens_match_python_snapshots() {
        let hw = full_hw();
        let readings = full_readings();
        let cases = [
            ("panel_v", true, true),
            ("panel_h", false, true),
            ("tooltip", true, false),
        ];

        for (name, vertical, panel) in cases {
            let mut cfg = load_config(None, Some(vertical)).expect("load shipped config");
            reset_panel_autofit_fields(&mut cfg);
            let formatter = PanelFormatter::with_now_unix(&cfg, &hw, 1_000_000);
            if !panel {
                let canonical = formatter.canonical_width(&readings) as i32;
                apply_canonical_width(&mut cfg, canonical);
            }
            let formatter = PanelFormatter::with_now_unix(&cfg, &hw, 1_000_000);
            let html = if panel {
                formatter.format_panel(&readings, "")
            } else {
                formatter.format_tooltip(&readings, "")
            };
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/golden")
                .join(format!("{name}.html"));
            let expected = std::fs::read_to_string(path).expect("read golden");
            assert_eq!(html, expected, "golden mismatch for {name}");
        }
    }

    fn reset_panel_autofit_fields(cfg: &mut Config) {
        let defaults = Config::default();
        cfg.display.panel_font_size = defaults.display.panel_font_size;
        cfg.display.panel_min_width = defaults.display.panel_min_width;
        cfg.bar_panel = BarConfig {
            width: BarConfig::default().width,
            height: 3,
        };
        cfg.column_panel = ColumnConfig::default();
        cfg.spark_panel = SparkConfig::default();
        cfg.braille_panel = BrailleConfig::default();
    }
}
