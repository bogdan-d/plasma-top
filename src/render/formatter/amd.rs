//! AMDGPU rows retain vendor labels and driver allocation semantics.

use crate::domain::{DisplaySnapshot, Metric};

use super::super::cells::regular_label_cell;
use super::super::model::{Ident, Row, auxiliary_cell, value_cell};
use super::{PanelFormatter, collect_row};

/// Bounds the displayed scalar width without changing captured readings.
const MAX_SCALAR: u32 = 99_999;
const MAX_TEMPERATURE: i32 = 999;

impl PanelFormatter<'_> {
    pub(super) fn render_amd(
        &self,
        metric: Metric,
        form: Option<&str>,
        readings: &DisplaySnapshot,
        tooltip: bool,
    ) -> Vec<Row> {
        let name = metric.as_str();
        match metric {
            Metric::GpuAmdUsage => {
                let Some(value) = readings
                    .gpu_amd_usage
                    .filter(|value| (0..=100).contains(value))
                else {
                    return Vec::new();
                };
                let thresholds = &self.cfg.thresholds.gpu_amd_usage;
                self.render_percent_row(
                    name,
                    form,
                    tooltip,
                    Some(value),
                    (&thresholds[0], &thresholds[1]),
                    true,
                )
            }
            Metric::GpuAmdCodecUsage => {
                let Some(value) = readings
                    .gpu_amd_codec_usage
                    .filter(|value| (0..=100).contains(value))
                else {
                    return Vec::new();
                };
                self.render_active_percent_row(
                    name,
                    form,
                    tooltip,
                    Some(value),
                    self.cfg.thresholds.gpu_amd_codec_usage,
                    true,
                )
            }
            Metric::GpuAmdTemp => {
                let Some(value) = readings
                    .gpu_amd_temp
                    .filter(|value| (-273..=MAX_TEMPERATURE).contains(value))
                else {
                    return Vec::new();
                };
                let thresholds = &self.cfg.thresholds.gpu_amd_temp;
                self.render_temp_row(
                    name,
                    form,
                    tooltip,
                    Some(value),
                    (&thresholds[0], &thresholds[1]),
                )
            }
            Metric::GpuAmdMemUsage => {
                let Some(memory) = readings.gpu_amd_mem_usage else {
                    return Vec::new();
                };
                let Some(percent) = memory.percent() else {
                    return Vec::new();
                };
                let thresholds = &self.cfg.thresholds.gpu_amd_mem_usage;
                let space = tooltip.then(|| {
                    auxiliary_cell(
                        format!(
                            "{} / {} MiB",
                            memory_mib(memory.used_bytes),
                            memory_mib(memory.total_bytes)
                        ),
                        None,
                        Some(&Ident::new(name, form)),
                        1,
                        0,
                        None,
                    )
                });
                collect_row(vec![
                    regular_label_cell(self.cfg, name, form, tooltip, None, None),
                    space,
                    Some(self.percent_value_cell(
                        name,
                        form,
                        tooltip,
                        Some(percent),
                        (thresholds[0], thresholds[1]),
                        true,
                    )),
                ])
            }
            Metric::GpuAmdFreq => {
                self.amd_scalar(name, form, readings.gpu_amd_freq, "MHz", tooltip)
            }
            Metric::GpuAmdPower => {
                self.amd_scalar(name, form, readings.gpu_amd_power, "W", tooltip)
            }
            Metric::GpuAmdFanSpeed => {
                self.amd_scalar(name, form, readings.gpu_amd_fan_speed, "RPM", tooltip)
            }
            _ => unreachable!("non-AMD metric routed to AMD formatter"),
        }
    }

    fn amd_scalar(
        &self,
        name: &str,
        form: Option<&str>,
        value: Option<u32>,
        unit: &str,
        tooltip: bool,
    ) -> Vec<Row> {
        let Some(value) = value else {
            return Vec::new();
        };
        let text = if unit == "RPM" && value == 0 {
            String::from("off")
        } else if value > MAX_SCALAR {
            format!("{MAX_SCALAR}+ {unit}")
        } else {
            format!("{value} {unit}")
        };
        collect_row(vec![
            regular_label_cell(self.cfg, name, form, tooltip, None, None),
            Some(value_cell(text, None, Some(&Ident::new(name, form)), 0)),
        ])
    }
}

// Bound allocation text independently of hardware capacity, preserving an overflow marker.
fn memory_mib(bytes: u64) -> String {
    let mib = bytes / (1 << 20);
    if mib > 999_999 {
        String::from("999999+")
    } else {
        mib.to_string()
    }
}
