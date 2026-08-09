//! Disk, drive temperature, SMART, and fan formatting.

use crate::domain::{DisplaySnapshot, Metric};

use super::super::cells::{
    TEMP_SCALE, disk_label, fmt_disk_space, hd_label, label_cell, regular_label_cell, table_text,
};
use super::super::model::{
    Cell, Ident, Row, auxiliary_cell, css_class_from_thresholds, render_two_pair_row, value_cell,
};
use super::{PanelFormatter, collect_row};

impl PanelFormatter<'_> {
    pub(super) fn render_hd_temp(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
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

    pub(super) fn render_disk_usage(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
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

    pub(super) fn render_disk_smart(&self, readings: &DisplaySnapshot, tooltip: bool) -> Vec<Row> {
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

    pub(super) fn render_fan_speed(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
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
                None => String::from(super::super::model::EMPTY_VALUE),
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

    fn render_hd_temp_pair(&self, readings: &DisplaySnapshot, tooltip: bool) -> Vec<Row> {
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

    fn render_fan_speed_pair(&self, readings: &DisplaySnapshot, tooltip: bool) -> Vec<Row> {
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
