//! System and peripheral battery formatting.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::{
    BatteryPeripheralReading, BatteryState, BatterySystemReading, DisplaySnapshot, Metric,
};

use super::super::cells::{BATTERY_ALTERNATE_SECONDS, label_cell, table_text};
use super::super::model::{
    Ident, Row, auxiliary_cell, css_class_battery, format_percent, render_three_col_row, value_cell,
};
use super::PanelFormatter;

impl PanelFormatter<'_> {
    pub(super) fn render_battery_sys(&self, readings: &DisplaySnapshot, tooltip: bool) -> Vec<Row> {
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

    pub(super) fn render_battery_peripheral(
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
                    super::super::model::EMPTY_VALUE,
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
}
