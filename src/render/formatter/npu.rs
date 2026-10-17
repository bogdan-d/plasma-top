//! AMD NPU rows: one-decimal watts for small draws and `off` once the clock decays to zero.

use crate::domain::{DisplaySnapshot, Metric};

use super::super::cells::regular_label_cell;
use super::super::model::{Ident, Row, value_cell};
use super::{PanelFormatter, collect_row};

impl PanelFormatter<'_> {
    pub(super) fn render_npu(
        &self,
        metric: Metric,
        form: Option<&str>,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let name = metric.as_str();
        let text = match metric {
            Metric::NpuAmdUsage => {
                let Some(value) = readings
                    .npu_amd_usage
                    .filter(|value| (0..=100).contains(value))
                else {
                    return Vec::new();
                };
                let thresholds = &self.cfg.thresholds.npu_amd_usage;
                return self.render_percent_row(
                    name,
                    form,
                    tooltip,
                    Some(value),
                    (&thresholds[0], &thresholds[1]),
                    true,
                );
            }
            Metric::NpuAmdPower => readings.npu_amd_power_mw.map(watts),
            Metric::NpuAmdFreq => readings.npu_amd_freq.map(|mhz| {
                if mhz == 0 {
                    String::from("off")
                } else {
                    format!("{mhz} MHz")
                }
            }),
            _ => unreachable!("non-NPU metric routed to NPU formatter"),
        };
        let Some(text) = text else {
            return Vec::new();
        };
        collect_row(vec![
            regular_label_cell(self.cfg, name, form, tooltip, None, None),
            Some(value_cell(text, None, Some(&Ident::new(name, form)), 0)),
        ])
    }
}

// Rounds to tenths; the u16 source bounds the text at 65.5 W.
fn watts(milliwatts: u16) -> String {
    let tenths = (u32::from(milliwatts) + 50) / 100;
    format!("{}.{} W", tenths / 10, tenths % 10)
}
