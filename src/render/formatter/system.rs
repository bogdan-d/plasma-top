//! Uptime, load, process, update, and server-state formatting.

use crate::domain::{DisplaySnapshot, Metric};

use super::super::cells::label_cell;
use super::super::model::{
    Ident, Row, auxiliary_cell, css_class_from_thresholds, render_three_col_row, value_cell,
};
use super::PanelFormatter;

impl PanelFormatter<'_> {
    pub(super) fn render_uptime(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Row {
        let metric = Metric::Uptime.as_str();
        let text = readings.uptime_seconds.map_or_else(
            || String::from(super::super::model::EMPTY_VALUE),
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

    pub(super) fn render_load_average(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Row {
        let metric = Metric::LoadAverage.as_str();
        let text = readings.load_average.map_or_else(
            || String::from(super::super::model::EMPTY_VALUE),
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

    pub(super) fn render_top_process(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
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

    pub(super) fn render_system_updates(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
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
                    || String::from(super::super::model::EMPTY_VALUE),
                    |value| value.to_string(),
                ),
                class,
                Some(&Ident::new(metric, form_token)),
                0,
            ),
        ]
    }

    pub(super) fn render_server_check(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Row {
        let metric = Metric::ServerCheck.as_str();
        let (text, class) = match readings.server_ok {
            Some(true) => (String::from("Ok"), None),
            Some(false) => (String::from("KO"), Some("crit")),
            None => (String::from(super::super::model::EMPTY_VALUE), None),
        };
        vec![
            label_cell(self.cfg, metric, form_token, tooltip, None, None),
            value_cell(text, class, Some(&Ident::new(metric, form_token)), 0),
        ]
    }
}

fn color_span(value: f64, class: &str) -> String {
    format!(r#"<span class="{class}">{value:.2}</span>"#)
}
