//! CPU, GPU, utilization, temperature, and shared value formatting.

use crate::domain::{DisplaySnapshot, Metric};

use super::super::cells::{TEMP_SCALE, fmt_disk_space, fmt_freq, label_cell, regular_label_cell};
use super::super::model::{
    Cell, Ident, PERCENT_PANEL_WIDTH, Row, auxiliary_cell, css_class_active,
    css_class_from_thresholds, format_percent, value_cell,
};
use super::super::registry::trace_metric;
use super::super::traces::{
    bar_braille_row, bar_row, bar_spark_row, braille_html, braille_row, column_row, spark_html,
    spark_row,
};
use super::{PanelFormatter, collect_row};

impl PanelFormatter<'_> {
    pub(super) fn render_historied(
        &self,
        metric: Metric,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
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

    pub(super) fn render_percent_row(
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

    pub(super) fn render_plain_percent_row(
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

    pub(super) fn render_active_percent_row(
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

    pub(super) fn render_temp_row(
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

    pub(super) fn render_freq_row(
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

    pub(super) fn render_cpu_freq_row(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
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

    pub(super) fn render_cpu_turbo_row(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let (text, class) = match readings.cpu_turbo {
            Some(true) => ("on", Some("active")),
            Some(false) => ("off", Some("crit")),
            None => (super::super::model::EMPTY_VALUE, None),
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

    pub(super) fn render_gpu_fan_row(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let text = match readings.gpu_fan {
            None => String::from(super::super::model::EMPTY_VALUE),
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

    fn spark_aux_cell(
        &self,
        metric: &str,
        form_token: Option<&str>,
        history: Option<&[i32]>,
        trace_metric: super::super::traces::TraceMetric,
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
        trace_metric: super::super::traces::TraceMetric,
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

    pub(super) fn percent_value_cell(
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
                super::super::model::EMPTY_VALUE,
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
                super::super::model::EMPTY_VALUE,
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
                super::super::model::EMPTY_VALUE,
                None,
                Some(&Ident::new(metric, form_token)),
                0,
            ),
        }
    }

    pub(super) fn temp_value_cell(
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
                super::super::model::EMPTY_VALUE,
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
        readings: &DisplaySnapshot,
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
