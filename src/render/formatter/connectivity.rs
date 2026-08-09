//! Network rate, interface, IP, and Wi-Fi formatting.

use crate::domain::{DisplaySnapshot, Metric};

use super::super::cells::{NETDEV_MAX, SSID_MAX, label_cell, middle_ellipsis, net_fmt};
use super::super::model::{
    Ident, PERCENT_PANEL_WIDTH, Row, css_class_battery, format_percent, render_two_pair_row,
    value_cell,
};
use super::PanelFormatter;

impl PanelFormatter<'_> {
    pub(super) fn render_dual_rate_rows(
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

    pub(super) fn render_string_row(
        &self,
        metric: &str,
        form_token: Option<&str>,
        value: Option<&str>,
        cap: Option<usize>,
        tooltip: bool,
    ) -> Vec<Row> {
        let text = value.map_or_else(
            || String::from(super::super::model::EMPTY_VALUE),
            |value| cap.map_or_else(|| value.to_owned(), |cap| middle_ellipsis(value, cap)),
        );
        vec![vec![
            label_cell(self.cfg, metric, form_token, tooltip, None, None),
            value_cell(text, None, Some(&Ident::new(metric, form_token)), 0),
        ]]
    }

    pub(super) fn render_wifi_signal(
        &self,
        form_token: Option<&str>,
        readings: &DisplaySnapshot,
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
                    || String::from(super::super::model::EMPTY_VALUE),
                    |value| format_percent(i64::from(value), tooltip),
                ),
                class,
                Some(&Ident::new(metric, form_token)),
                PERCENT_PANEL_WIDTH,
            ),
        ]]
    }

    pub(super) fn render_net_device_ip(
        &self,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let device = readings.net_device.as_deref().map_or_else(
            || String::from(super::super::model::EMPTY_VALUE),
            |device| middle_ellipsis(device, NETDEV_MAX),
        );
        let ip = readings
            .ip_address
            .clone()
            .unwrap_or_else(|| String::from(super::super::model::EMPTY_VALUE));
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

    pub(super) fn render_wifi_ssid_signal(
        &self,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let ssid = readings.wifi_ssid.as_deref().map_or_else(
            || String::from(super::super::model::EMPTY_VALUE),
            |ssid| middle_ellipsis(ssid, SSID_MAX),
        );
        let signal = readings.wifi_signal_percent.map_or_else(
            || String::from(super::super::model::EMPTY_VALUE),
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
}
