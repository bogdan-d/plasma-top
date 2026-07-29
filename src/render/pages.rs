//! Deep-dive tooltip page rendering.

use crate::config::Config;
use crate::domain::{HardwareSnapshot, ReadingsSnapshot};

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
    hw: &'a HardwareSnapshot,
}

impl<'a> PageFormatter<'a> {
    /// Creates a page formatter bound to one config and hardware snapshot.
    #[must_use]
    pub fn new(cfg: &'a Config, hw: &'a HardwareSnapshot) -> Self {
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
        readings: &ReadingsSnapshot,
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
        readings: &ReadingsSnapshot,
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
        readings: &ReadingsSnapshot,
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

        if self.hw.has_nvidia {
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
                                readings.gpu_usage,
                                Some((
                                    self.cfg.thresholds.gpu_nvidia_usage[0],
                                    self.cfg.thresholds.gpu_nvidia_usage[1],
                                )),
                            ),
                        ),
                        (
                            Some(ORANGE_LINE),
                            "Decoder",
                            graph_value_active(
                                readings.gpu_dec,
                                Some(self.cfg.thresholds.gpu_nvidia_dec_usage),
                            ),
                        ),
                    ]),
            );
        } else if self.hw.intel_gpu_pci.is_some() {
            let overlay = readings
                .gpu_dec_history
                .iter()
                .map(|value| f64::from(*value))
                .collect::<Vec<_>>();
            let png = area_chart_png(
                &readings
                    .gpu_usage_history
                    .iter()
                    .map(|value| f64::from(*value))
                    .collect::<Vec<_>>(),
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
                                readings.gpu_intel_usage,
                                Some((
                                    self.cfg.thresholds.gpu_intel_usage[0],
                                    self.cfg.thresholds.gpu_intel_usage[1],
                                )),
                            ),
                        ),
                        (
                            Some(ORANGE_LINE),
                            "Decoder",
                            graph_value_active(
                                readings.gpu_intel_dec_usage,
                                Some(self.cfg.thresholds.gpu_intel_dec_usage),
                            ),
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
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::config::Config;
    use crate::domain::{TopProcessDetails, TopProcessSummary};

    fn hw() -> HardwareSnapshot {
        HardwareSnapshot {
            cpu_count: 8,
            net_device: Some(String::from("wlan0")),
            ..HardwareSnapshot::default()
        }
    }

    fn readings() -> ReadingsSnapshot {
        ReadingsSnapshot {
            cpu_core_usage: Some(vec![10, 20]),
            cpu_core_history: Some(vec![vec![10, 20, 30, 40], vec![15, 25, 35, 45]]),
            top_process: Some(vec![TopProcessSummary {
                command: String::from("plasmashell"),
                cpu_percent: 12,
            }]),
            top_process_full: Some(vec![
                TopProcessDetails {
                    pid: 1234,
                    command: String::from("plasmashell --replace"),
                    cpu_percent: 12,
                    memory_percent: 3.2,
                },
                TopProcessDetails {
                    pid: 999_999,
                    command: String::from("firefox"),
                    cpu_percent: 70,
                    memory_percent: 12.4,
                },
            ]),
            cpu_history: vec![10, 40, 60, 30],
            mem_history: vec![20, 30, 50, 40],
            gpu_usage_history: vec![15, 35, 55, 45],
            gpu_dec_history: vec![0, 5, 10, 5],
            net_down_history: vec![1000, 2000, 3000],
            net_up_history: vec![500, 1000, 1500],
            cpu_usage: Some(42),
            mem_usage: Some(55),
            net_down_bps: Some(3_000_000),
            net_up_bps: Some(1_500_000),
            ..ReadingsSnapshot::default()
        }
    }

    fn normalize_png_uris(mut html: String) -> String {
        const PREFIX: &str = "data:image/png;base64,";
        while let Some(start) = html.find(PREFIX) {
            let end = html[start..]
                .find('"')
                .map_or(html.len(), |offset| start + offset);
            html.replace_range(start..end, "<PNG>");
        }
        html
    }

    #[test]
    fn format_page_wraps_tooltip_shell() {
        let cfg = Config::default();
        let hardware = hw();
        let formatter = PageFormatter::new(&cfg, &hardware);

        let html = formatter.format_page("<div class=\"page\">x</div>", "", "<h>", "<f>");

        assert_eq!(
            html,
            r#"<div class="tooltip"><h><div class="page">x</div><f></div>"#
        );
    }

    #[test]
    fn format_cpu_cores_uses_no_data_message_when_absent() {
        let cfg = Config::default();
        let hardware = hw();
        let formatter = PageFormatter::new(&cfg, &hardware);

        let html = formatter.format_cpu_cores(&ReadingsSnapshot::default(), "", "", None);

        assert_eq!(
            html,
            r#"<div class="tooltip"><div class="page">cpu cores: no data yet</div></div>"#
        );
    }

    #[test]
    fn format_cpu_cores_and_top_process_emit_expected_shell_bits() {
        let mut cfg = Config::default();
        cfg.display.tooltip_width = 34;
        let hardware = hw();
        let formatter = PageFormatter::new(&cfg, &hardware);
        let readings = readings();

        let cpu = formatter.format_cpu_cores(
            &readings,
            "",
            "<header>",
            Some(&|width| format!("<footer>{width}</footer>")),
        );
        let proc = formatter.format_top_process(
            &readings,
            "",
            "<header>",
            Some(&|width| format!("<footer>{width}</footer>")),
        );

        assert_eq!(
            cpu,
            "<div class=\"tooltip\"><header><div class=\"page\"><span class=\"label\">Core 0:&nbsp;</span>⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀<span class=\"grad-cpu-1\">⣀</span><span class=\"grad-cpu-3\">⣤</span><span class=\"gap\">&nbsp;</span><span class=\"val good\">&nbsp;10%</span><br><span class=\"label\">Core 1:&nbsp;</span>⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀<span class=\"grad-cpu-2\">⣀</span><span class=\"grad-cpu-3\">⣤</span><span class=\"gap\">&nbsp;</span><span class=\"val good\">&nbsp;20%</span></div><footer>34</footer></div>"
        );
        assert_eq!(
            proc,
            "<div class=\"tooltip\"><header><div class=\"page\">&nbsp;&nbsp;&nbsp;<span class=\"label\">PID</span>&nbsp;&nbsp;<span class=\"label\">COMMAND</span>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<span class=\"label\">%CPU</span>&nbsp;&nbsp;<span class=\"label\">%MEM</span><br>&nbsp;&nbsp;<span class=\"label\">1234</span>&nbsp;&nbsp;plasmashell --…&nbsp;&nbsp;&nbsp;<span class=\"val good\">12</span>&nbsp;&nbsp;&nbsp;<span class=\"val good\">3.2</span><br><span class=\"label\">999999</span>&nbsp;&nbsp;firefox&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<span class=\"val crit\">70</span>&nbsp;&nbsp;<span class=\"val good\">12.4</span></div><footer>34</footer></div>"
        );
    }

    #[test]
    fn format_graphs_embeds_pngs_and_legends() {
        let mut cfg = Config::default();
        cfg.display.tooltip_width = 35;
        cfg.pages.graph_width = 120;
        let hardware = hw();
        let formatter = PageFormatter::new(&cfg, &hardware);
        let readings = readings();

        let html = formatter.format_graphs(
            &readings,
            "",
            "<header>",
            Some(&|width| format!("<footer>{width}</footer>")),
        );

        assert!(html.contains("data:image/png;base64,"));
        assert!(html.contains("CPU usage"));
        assert!(html.contains("Memory usage"));
        assert!(html.contains("Download"));
        assert!(html.contains("Upload"));
        assert!(html.contains("<footer>35</footer>"));
        assert_eq!(html.matches("data:image/png;base64,").count(), 3);
        assert_eq!(html.matches("width=\"120\" height=\"84\"").count(), 3);
        assert_eq!(
            normalize_png_uris(html),
            r#"<div class="tooltip"><header><div style="font-size:6px">&nbsp;</div><div><img src="<PNG>" width="120" height="84"></div><div class="page"><span style="color:rgb(61,174,233)">●</span>&nbsp;<span class="label">CPU usage:</span>&nbsp;<span class="val good">42%</span></div><div style="font-size:16px">&nbsp;</div><div><img src="<PNG>" width="120" height="84"></div><div class="page"><span style="color:rgb(163,102,255)">●</span>&nbsp;<span class="label">Memory usage:</span>&nbsp;<span class="val warn">55%</span></div><div style="font-size:16px">&nbsp;</div><div><img src="<PNG>" width="120" height="84"></div><div class="page"><span style="color:rgb(26,188,156)">●</span>&nbsp;<span class="label">Download:</span>&nbsp;<span class="val">3M</span><br><span style="color:rgb(231,76,60)">●</span>&nbsp;<span class="label">Upload:</span>&nbsp;<span class="val">1M</span></div><footer>35</footer></div>"#
        );
    }

    #[test]
    fn format_top_process_escapes_commands_and_caps_rows() {
        let mut cfg = Config::default();
        cfg.display.tooltip_width = 34;
        let hardware = hw();
        let formatter = PageFormatter::new(&cfg, &hardware);
        let rows = (0..16)
            .map(|index| TopProcessDetails {
                pid: 1000 + index,
                command: if index == 0 {
                    String::from("<worker>")
                } else {
                    format!("worker-{index}")
                },
                cpu_percent: 1,
                memory_percent: 1.0,
            })
            .collect();
        let readings = ReadingsSnapshot {
            top_process_full: Some(rows),
            ..ReadingsSnapshot::default()
        };

        let html = formatter.format_top_process(&readings, "", "", None);

        assert!(html.contains("&lt;worker&gt;"));
        assert!(!html.contains("worker-15"));
        assert!(!html.contains("<table"));
    }

    #[test]
    fn format_graphs_prefers_nvidia_and_omits_absent_network() {
        let mut cfg = Config::default();
        cfg.pages.graph_width = 120;
        cfg.thresholds.gpu_nvidia_usage = vec![10, 20];
        cfg.thresholds.gpu_intel_usage = vec![80, 90];
        let hardware = HardwareSnapshot {
            has_nvidia: true,
            intel_gpu_pci: Some(String::from("0000:00:02.0")),
            ..HardwareSnapshot::default()
        };
        let formatter = PageFormatter::new(&cfg, &hardware);
        let readings = ReadingsSnapshot {
            cpu_history: vec![10],
            mem_history: vec![20],
            gpu_usage_history: vec![60],
            gpu_dec_history: vec![5],
            gpu_usage: Some(60),
            gpu_dec: Some(5),
            gpu_intel_usage: Some(1),
            gpu_intel_dec_usage: Some(1),
            ..ReadingsSnapshot::default()
        };

        let html = formatter.format_graphs(&readings, "", "", None);

        assert_eq!(html.matches("data:image/png;base64,").count(), 3);
        assert!(html.contains(r#"<span class="val crit">60%</span>"#));
        assert!(!html.contains("Download"));
    }

    #[test]
    fn graph_value_helpers_match_threshold_classes() {
        assert!(graph_value_band(None, Some((50, 70))).contains(EMPTY_VALUE));
        assert_eq!(
            graph_value_active(Some(0), Some(1)),
            r#"<span class="val ">0%</span>"#
        );
    }
}
