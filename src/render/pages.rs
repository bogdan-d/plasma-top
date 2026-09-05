//! Deep-dive tooltip page rendering.

use crate::config::Config;
use crate::domain::{DisplaySnapshot, HardwareInventory};

use super::cells::{TOP_PROCESS_COMM_MIN, TOP_PROCESS_MIN_PID, TOP_PROCESS_SIDE_COLS, net_fmt};
use super::chart::{
    AreaChartOptions, BLUE_FILL, BLUE_LINE, GREEN_FILL, GREEN_LINE, ORANGE_LINE, PURPLE_FILL,
    PURPLE_LINE, RED_LINE, RGBA, TEAL_FILL, TEAL_LINE, area_chart_png,
};
use super::model::{EMPTY_VALUE, css_class_active, css_class_from_thresholds};
use super::traces::{TraceMetric, braille_html};

const GRAPH_HEIGHT: usize = 84;
const GRAPH_LEFT_PAD: usize = 18;

#[derive(Clone, Copy)]
struct ProcessLayout {
    pid_width: usize,
    command_width: usize,
}

struct ProcessLine<'a> {
    pid: &'a str,
    command: &'a str,
    cpu: &'a str,
    memory: &'a str,
    classes: [Option<&'a str>; 4],
}

type LegendEntry<'a> = (Option<RGBA>, &'a str, String);

/// Formats the deep-dive tooltip pages.
pub struct PageFormatter<'a> {
    cfg: &'a Config,
    hw: &'a HardwareInventory,
}

impl<'a> PageFormatter<'a> {
    /// Creates a page formatter bound to one config and hardware snapshot.
    #[must_use]
    pub fn new(cfg: &'a Config, hw: &'a HardwareInventory) -> Self {
        Self { cfg, hw }
    }

    /// Wraps arbitrary page HTML in the standard tooltip shell.
    #[must_use]
    pub fn format_page(&self, inner: &str, css: &str, header: &str, footer: &str) -> String {
        self.wrap_tooltip(inner, css, header, footer)
    }

    /// Formats the `cpu_cores` deep-dive page.
    #[must_use]
    pub fn format_cpu_cores(
        &self,
        readings: &DisplaySnapshot,
        css: &str,
        header: &str,
        pager_fn: Option<&dyn Fn(usize) -> String>,
    ) -> String {
        let Some(usage) = readings
            .cpu_core_usage
            .as_ref()
            .filter(|usage| !usage.is_empty())
        else {
            return self.wrap_tooltip(
                r#"<div class="page">cpu cores: no data yet</div>"#,
                css,
                header,
                "",
            );
        };

        let hist = readings.cpu_core_history.as_ref();
        let thresholds = (
            self.cfg.thresholds.cpu_usage[0],
            self.cfg.thresholds.cpu_usage[1],
        );
        let label_width = format!("Core {}:", usage.len().saturating_sub(1))
            .chars()
            .count();
        let value_width = 4usize;
        let min_width = self.cfg.display.tooltip_width.max(0) as usize;
        let braille_width = if min_width > 0 {
            Some((min_width.saturating_sub(label_width + value_width + 2)).max(1))
        } else {
            None
        };

        let mut lines = Vec::with_capacity(usage.len());
        for (index, current) in usage.iter().copied().enumerate() {
            let label = format!("Core {index}:");
            let label_html = format!(
                r#"<span class="label">{}{}</span>"#,
                label,
                "&nbsp;".repeat(label_width.saturating_sub(label.chars().count()) + 1)
            );
            let history = hist.and_then(|histories| histories.get(index).map(Vec::as_slice));
            let spark = braille_html(self.cfg, history, TraceMetric::Cpu, true, braille_width);
            let value = format!("{current}%");
            let class = css_class_from_thresholds(
                f64::from(current),
                (f64::from(thresholds.0), f64::from(thresholds.1)),
            );
            lines.push(format!(
                r#"{label_html}{spark}<span class="gap">&nbsp;</span><span class="val {class}">{}{value}</span>"#,
                "&nbsp;".repeat(value_width.saturating_sub(value.chars().count()))
            ));
        }

        let braille_columns = braille_width
            .unwrap_or_else(|| self.cfg.braille_tooltip.cpu_braille_length.max(0) as usize);
        let width = label_width + 1 + braille_columns + 1 + value_width;
        let footer = pager_fn.map_or_else(String::new, |pager_fn| pager_fn(width));
        self.wrap_tooltip(
            &format!(r#"<div class="page">{}</div>"#, lines.join("<br>")),
            css,
            header,
            &footer,
        )
    }

    /// Formats the `processes` deep-dive page.
    #[must_use]
    pub fn format_top_process(
        &self,
        readings: &DisplaySnapshot,
        css: &str,
        header: &str,
        pager_fn: Option<&dyn Fn(usize) -> String>,
    ) -> String {
        let Some(rows) = readings
            .top_process_full
            .as_ref()
            .filter(|rows| !rows.is_empty())
        else {
            return self.wrap_tooltip(
                r#"<div class="page">top processes: no data yet</div>"#,
                css,
                header,
                "",
            );
        };

        let shown = rows
            .iter()
            .take(crate::page_commands::top_process_page_rows())
            .collect::<Vec<_>>();
        let pid_width = shown
            .iter()
            .map(|row| row.pid.to_string().chars().count())
            .max()
            .unwrap_or(0)
            .max(TOP_PROCESS_MIN_PID);
        let tooltip_width = self.cfg.display.tooltip_width.max(0) as usize;
        let command_width = TOP_PROCESS_COMM_MIN
            .max(tooltip_width.saturating_sub(pid_width + TOP_PROCESS_SIDE_COLS));
        let cpu_thresholds = (
            self.cfg.thresholds.top_process_cpu[0],
            self.cfg.thresholds.top_process_cpu[1],
        );
        let mem_thresholds = (
            self.cfg.thresholds.top_process_mem[0],
            self.cfg.thresholds.top_process_mem[1],
        );

        let mut lines = Vec::with_capacity(shown.len() + 1);
        let layout = ProcessLayout {
            pid_width,
            command_width,
        };
        lines.push(render_process_line(
            ProcessLine {
                pid: "PID",
                command: "COMMAND",
                cpu: "%CPU",
                memory: "%MEM",
                classes: [Some("label"); 4],
            },
            layout,
        ));
        for row in shown {
            let cpu_class = format!(
                "val {}",
                css_class_from_thresholds(
                    f64::from(row.cpu_percent),
                    (f64::from(cpu_thresholds.0), f64::from(cpu_thresholds.1)),
                )
            );
            let mem_class = format!(
                "val {}",
                css_class_from_thresholds(
                    row.memory_percent,
                    (f64::from(mem_thresholds.0), f64::from(mem_thresholds.1))
                ),
            );
            let pid = row.pid.to_string();
            let command = clip_command(&row.command, command_width);
            let cpu = row.cpu_percent.to_string();
            let memory = format!("{:.1}", row.memory_percent);
            lines.push(render_process_line(
                ProcessLine {
                    pid: &pid,
                    command: &command,
                    cpu: &cpu,
                    memory: &memory,
                    classes: [Some("label"), None, Some(&cpu_class), Some(&mem_class)],
                },
                layout,
            ));
        }

        let footer = pager_fn.map_or_else(String::new, |pager_fn| {
            pager_fn(pid_width + command_width + TOP_PROCESS_SIDE_COLS)
        });
        self.wrap_tooltip(
            &format!(r#"<div class="page">{}</div>"#, lines.join("<br>")),
            css,
            header,
            &footer,
        )
    }

    /// Formats the `graphs` deep-dive page.
    #[must_use]
    pub fn format_graphs(
        &self,
        readings: &DisplaySnapshot,
        css: &str,
        header: &str,
        pager_fn: Option<&dyn Fn(usize) -> String>,
    ) -> String {
        let width = self.cfg.pages.graph_width.max(0) as usize;
        let cpu_png = area_chart_png(
            &readings
                .cpu_history
                .iter()
                .map(|value| f64::from(*value))
                .collect::<Vec<_>>(),
            width,
            GRAPH_HEIGHT,
            AreaChartOptions {
                left_pad: GRAPH_LEFT_PAD,
                line: BLUE_LINE,
                fill: BLUE_FILL,
                ..AreaChartOptions::default()
            },
        );
        let mem_png = area_chart_png(
            &readings
                .mem_history
                .iter()
                .map(|value| f64::from(*value))
                .collect::<Vec<_>>(),
            width,
            GRAPH_HEIGHT,
            AreaChartOptions {
                left_pad: GRAPH_LEFT_PAD,
                line: PURPLE_LINE,
                fill: PURPLE_FILL,
                ..AreaChartOptions::default()
            },
        );

        let mut blocks = vec![
            png_img(&cpu_png, width)
                + &legend(vec![(
                    Some(BLUE_LINE),
                    "CPU usage",
                    graph_value_band(
                        readings.cpu_usage,
                        Some((
                            self.cfg.thresholds.cpu_usage[0],
                            self.cfg.thresholds.cpu_usage[1],
                        )),
                    ),
                )]),
            png_img(&mem_png, width)
                + &legend(vec![(
                    Some(PURPLE_LINE),
                    "Memory usage",
                    graph_value_band(
                        readings.mem_usage,
                        Some((
                            self.cfg.thresholds.mem_usage[0],
                            self.cfg.thresholds.mem_usage[1],
                        )),
                    ),
                )]),
        ];

        let gpu = if self.hw.has_nvidia {
            Some((
                readings.gpu_usage,
                readings.gpu_dec,
                self.cfg.thresholds.gpu_nvidia_usage.as_slice(),
                self.cfg.thresholds.gpu_nvidia_dec_usage,
                "Decoder",
            ))
        } else if self.hw.amd_gpu.is_some() {
            Some((
                readings.gpu_amd_usage,
                readings.gpu_amd_codec_usage,
                self.cfg.thresholds.gpu_amd_usage.as_slice(),
                self.cfg.thresholds.gpu_amd_codec_usage,
                self.cfg
                    .labels
                    .get("gpu_amd_codec_usage")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("AMD GPU codec"),
            ))
        } else if self.hw.intel_gpu_pci.is_some() {
            Some((
                readings.gpu_intel_usage,
                readings.gpu_intel_dec_usage,
                self.cfg.thresholds.gpu_intel_usage.as_slice(),
                self.cfg.thresholds.gpu_intel_dec_usage,
                "Decoder",
            ))
        } else {
            None
        };
        if let Some((
            usage_value,
            secondary_value,
            usage_thresholds,
            secondary_threshold,
            secondary_label,
        )) = gpu
        {
            let usage = readings
                .gpu_usage_history
                .iter()
                .map(|value| f64::from(*value))
                .collect::<Vec<_>>();
            let overlay = readings
                .gpu_dec_history
                .iter()
                .map(|value| f64::from(*value))
                .collect::<Vec<_>>();
            let png = area_chart_png(
                &usage,
                width,
                GRAPH_HEIGHT,
                AreaChartOptions {
                    left_pad: GRAPH_LEFT_PAD,
                    line: GREEN_LINE,
                    fill: GREEN_FILL,
                    overlay: Some(&overlay),
                    overlay_line: ORANGE_LINE,
                    ..AreaChartOptions::default()
                },
            );
            blocks.push(
                png_img(&png, width)
                    + &legend(vec![
                        (
                            Some(GREEN_LINE),
                            "GPU usage",
                            graph_value_band(
                                usage_value,
                                Some((usage_thresholds[0], usage_thresholds[1])),
                            ),
                        ),
                        (
                            Some(ORANGE_LINE),
                            secondary_label,
                            graph_value_active(secondary_value, Some(secondary_threshold)),
                        ),
                    ]),
            );
        }

        if self.hw.net_device.is_some() {
            let down = readings.net_down_history.iter().copied().max().unwrap_or(0);
            let up = readings.net_up_history.iter().copied().max().unwrap_or(0);
            let peak = down.max(up).max(1) as f64;
            let grid_levels = [0.0];
            let overlay = readings
                .net_up_history
                .iter()
                .map(|value| *value as f64)
                .collect::<Vec<_>>();
            let png = area_chart_png(
                &readings
                    .net_down_history
                    .iter()
                    .map(|value| *value as f64)
                    .collect::<Vec<_>>(),
                width,
                GRAPH_HEIGHT,
                AreaChartOptions {
                    vmax: peak,
                    left_pad: GRAPH_LEFT_PAD,
                    grid_levels: &grid_levels,
                    label_values: false,
                    line: TEAL_LINE,
                    fill: TEAL_FILL,
                    overlay: Some(&overlay),
                    overlay_line: RED_LINE,
                    ..AreaChartOptions::default()
                },
            );
            blocks.push(
                png_img(&png, width)
                    + &legend(vec![
                        (
                            Some(TEAL_LINE),
                            "Download",
                            format!(
                                r#"<span class="val">{}</span>"#,
                                net_fmt(readings.net_down_bps.unwrap_or(0))
                            ),
                        ),
                        (
                            Some(RED_LINE),
                            "Upload",
                            format!(
                                r#"<span class="val">{}</span>"#,
                                net_fmt(readings.net_up_bps.unwrap_or(0))
                            ),
                        ),
                    ]),
            );
        }

        let top_gap = r#"<div style="font-size:6px">&nbsp;</div>"#;
        let spacer = r#"<div style="font-size:16px">&nbsp;</div>"#;
        let columns = if self.cfg.display.tooltip_width > 0 {
            self.cfg.display.tooltip_width as usize
        } else {
            width / 9
        };
        let footer = pager_fn.map_or_else(String::new, |pager_fn| pager_fn(columns));
        self.wrap_tooltip(
            &format!("{top_gap}{}", blocks.join(spacer)),
            css,
            header,
            &footer,
        )
    }

    fn wrap_tooltip(&self, body: &str, css: &str, header: &str, footer: &str) -> String {
        let style = if css.is_empty() {
            String::new()
        } else {
            format!("<style>{css}</style>")
        };
        format!(r#"{style}<div class="tooltip">{header}{body}{footer}</div>"#)
    }
}

fn render_process_line(line: ProcessLine<'_>, layout: ProcessLayout) -> String {
    format!(
        "{}&nbsp;&nbsp;{}&nbsp;{}&nbsp;{}",
        render_field(line.pid, layout.pid_width, line.classes[0], true),
        render_field(line.command, layout.command_width, line.classes[1], false,),
        render_field(line.cpu, 4, line.classes[2], true),
        render_field(line.memory, 5, line.classes[3], true),
    )
}

fn render_field(text: &str, width: usize, class: Option<&str>, right: bool) -> String {
    let padding = "&nbsp;".repeat(width.saturating_sub(text.chars().count()));
    let mut body = escape_html(text);
    if let Some(class) = class {
        body = format!(r#"<span class="{class}">{body}</span>"#);
    }
    if right {
        format!("{padding}{body}")
    } else {
        format!("{body}{padding}")
    }
}

fn clip_command(command: &str, width: usize) -> String {
    if command.chars().count() <= width {
        return String::from(command);
    }
    if width == 0 {
        return String::new();
    }
    let mut clipped = command.chars().take(width - 1).collect::<String>();
    clipped.push('…');
    clipped
}

fn graph_value_band(current: Option<i32>, thresholds: Option<(i32, i32)>) -> String {
    match current {
        None => format!(r#"<span class="val">{EMPTY_VALUE}</span>"#),
        Some(current) => {
            let class = thresholds.map_or("", |thresholds| {
                css_class_from_thresholds(
                    f64::from(current),
                    (f64::from(thresholds.0), f64::from(thresholds.1)),
                )
            });
            format!(r#"<span class="val {class}">{current}%</span>"#)
        }
    }
}

fn graph_value_active(current: Option<i32>, threshold: Option<i32>) -> String {
    match current {
        None => format!(r#"<span class="val">{EMPTY_VALUE}</span>"#),
        Some(current) => {
            let class = threshold
                .and_then(|threshold| css_class_active(i64::from(current), i64::from(threshold)))
                .unwrap_or("");
            format!(r#"<span class="val {class}">{current}%</span>"#)
        }
    }
}

fn png_img(png: &[u8], width: usize) -> String {
    let uri = format!("data:image/png;base64,{}", encode_base64(png));
    format!(
        r#"<div><img src="{uri}" width="{width}" height="{}"></div>"#,
        GRAPH_HEIGHT
    )
}

fn legend(entries: Vec<LegendEntry<'_>>) -> String {
    let mut lines = Vec::with_capacity(entries.len());
    for (color, label, value) in entries {
        let dot = if let Some((red, green, blue, _)) = color {
            format!(r#"<span style="color:rgb({red},{green},{blue})">●</span>&nbsp;"#)
        } else {
            String::new()
        };
        lines.push(format!(
            r#"{dot}<span class="label">{}:</span>&nbsp;{value}"#,
            escape_html(label)
        ));
    }
    format!(r#"<div class="page">{}</div>"#, lines.join("<br>"))
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = *chunk.get(1).unwrap_or(&0);
        let b2 = *chunk.get(2).unwrap_or(&0);
        let triple = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests;
