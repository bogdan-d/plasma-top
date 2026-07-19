//! Percentage trace encodings and their standalone/combo rows.

use toml::{Table, Value};

use crate::config::{BRAILLE_LENGTH_MULTIPLIER, Config};

use super::model::{
    Cell, Ident, Row, auxiliary_cell, css_class_from_thresholds, render_two_pair_row,
};

const BLOCK_RAMP: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
const BAR_FILL_CHAR: char = '█';
const BAR_EMPTY_CHAR: char = '░';
const BRAILLE_LEFT_BITS: [u32; 4] = [0b0100_0000, 0b0000_0100, 0b0000_0010, 0b0000_0001];
const BRAILLE_RIGHT_BITS: [u32; 4] = [0b1000_0000, 0b0010_0000, 0b0001_0000, 0b0000_1000];
const BRAILLE_GRADES: i32 = 8;

/// The cpu/memory trace family shared by bar, spark, and braille forms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceMetric {
    /// CPU usage / history traces.
    Cpu,
    /// Memory usage / history traces.
    Memory,
}

impl TraceMetric {
    const fn usage_metric(self) -> &'static str {
        match self {
            Self::Cpu => "cpu_usage",
            Self::Memory => "mem_usage",
        }
    }

    const fn gradient_prefix(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Memory => "mem",
        }
    }

    fn spark_thresholds(self, cfg: &Config) -> (i32, i32) {
        match self {
            Self::Cpu => threshold_pair(&cfg.thresholds.cpu_spark),
            Self::Memory => threshold_pair(&cfg.thresholds.mem_spark),
        }
    }

    fn spark_length(self, cfg: &Config, tooltip: bool) -> usize {
        let raw = if tooltip {
            match self {
                Self::Cpu => cfg.spark_tooltip.cpu_spark_length,
                Self::Memory => cfg.spark_tooltip.mem_spark_length,
            }
        } else {
            match self {
                Self::Cpu => cfg.spark_panel.cpu_spark_length,
                Self::Memory => cfg.spark_panel.mem_spark_length,
            }
        };
        raw.max(0) as usize
    }

    fn braille_length(self, cfg: &Config, tooltip: bool) -> usize {
        let raw = if tooltip {
            match self {
                Self::Cpu => cfg.braille_tooltip.cpu_braille_length,
                Self::Memory => cfg.braille_tooltip.mem_braille_length,
            }
        } else {
            match self {
                Self::Cpu => cfg.braille_panel.cpu_braille_length,
                Self::Memory => cfg.braille_panel.mem_braille_length,
            }
        };
        raw.max(0) as usize
    }
}

fn threshold_pair(values: &[i32]) -> (i32, i32) {
    match values {
        [middle, high, ..] => (*middle, *high),
        [single] => (*single, *single),
        [] => (0, 0),
    }
}

fn table_text<'a>(table: &'a Table, key: &str) -> &'a str {
    table.get(key).and_then(Value::as_str).unwrap_or("")
}

fn font_size_style(height: i32) -> String {
    if height > 0 {
        format!(r#" style="font-size:{height}px""#)
    } else {
        String::new()
    }
}

fn block_ramp_at(index: usize) -> char {
    BLOCK_RAMP[index.min(BLOCK_RAMP.len() - 1)]
}

fn round_half_even(numerator: i64, denominator: i64) -> i64 {
    if denominator <= 0 {
        return 0;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    match (remainder * 2).cmp(&denominator) {
        std::cmp::Ordering::Less => quotient,
        std::cmp::Ordering::Greater => quotient + 1,
        std::cmp::Ordering::Equal => quotient + i64::from(quotient % 2 != 0),
    }
}

fn trace_label_cell(
    cfg: &Config,
    metric: TraceMetric,
    form: &str,
    tooltip: bool,
    text: &str,
) -> Cell {
    let ident = Ident::new(metric.usage_metric(), Some(form));
    let css_class = format!("label {}", ident.css());
    let glyph = table_text(&cfg.icons, metric.usage_metric());
    if !tooltip {
        return Cell::classified(glyph, css_class);
    }
    let word = if text.is_empty() {
        table_text(&cfg.labels, metric.usage_metric())
    } else {
        text
    };
    let delimiter = table_text(&cfg.labels, "delimiter");
    Cell::classified(format!("{glyph} {word}{delimiter}"), css_class)
}

fn bar_layout_width(cfg: &Config, tooltip: bool) -> Option<usize> {
    let bar_cfg = if tooltip {
        &cfg.bar_tooltip
    } else {
        &cfg.bar_panel
    };
    if bar_cfg.height <= 0 || bar_cfg.width <= 0 || cfg.display.panel_font_size <= 0 {
        return None;
    }
    let scaled = round_half_even(
        i64::from(bar_cfg.width) * i64::from(bar_cfg.height),
        i64::from(cfg.display.panel_font_size),
    );
    Some((scaled.max(1)) as usize)
}

fn standalone(html: &str, ident: &Ident, layout_width: Option<usize>) -> Vec<Row> {
    if html.is_empty() {
        return Vec::new();
    }
    vec![vec![auxiliary_cell(
        html,
        None,
        Some(ident),
        0,
        0,
        layout_width,
    )]]
}

fn braille_level(value: i32) -> usize {
    if value <= 0 {
        return 1;
    }
    ((value * 4 + 99) / 100).min(4) as usize
}

fn braille_char(left: Option<i32>, right: Option<i32>) -> char {
    let mut code = 0x2800;
    if let Some(value) = left {
        for bit in BRAILLE_LEFT_BITS.iter().take(braille_level(value)) {
            code |= bit;
        }
    }
    if let Some(value) = right {
        for bit in BRAILLE_RIGHT_BITS.iter().take(braille_level(value)) {
            code |= bit;
        }
    }
    char::from_u32(code).unwrap_or('\u{2800}')
}

fn history_tail(history: &[i32], length: usize) -> &[i32] {
    if history.len() > length {
        &history[history.len() - length..]
    } else {
        history
    }
}

/// Renders the `:bar` form HTML for one percentage value.
#[must_use]
pub fn bar_html(cfg: &Config, value: Option<i32>, thresholds: (i32, i32), tooltip: bool) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let bar_cfg = if tooltip {
        &cfg.bar_tooltip
    } else {
        &cfg.bar_panel
    };
    if bar_cfg.width <= 0 {
        return String::new();
    }
    let width = bar_cfg.width as usize;
    let filled = (((i64::from(value) * i64::from(bar_cfg.width)) / 100)
        .clamp(0, i64::from(bar_cfg.width))) as usize;
    let empty = width.saturating_sub(filled);
    let class = css_class_from_thresholds(
        f64::from(value),
        (f64::from(thresholds.0), f64::from(thresholds.1)),
    );
    let style = font_size_style(bar_cfg.height);
    format!(
        r#"<span class="bar-{class}"{style}>{}</span><span class="bar-empty"{style}>{}</span>"#,
        BAR_FILL_CHAR.to_string().repeat(filled),
        BAR_EMPTY_CHAR.to_string().repeat(empty),
    )
}

/// Renders the horizontal-panel `:column` form HTML for one percentage value.
#[must_use]
pub fn column_html(cfg: &Config, value: Option<i32>, thresholds: (i32, i32)) -> String {
    let Some(value) = value else {
        return String::new();
    };
    let class = css_class_from_thresholds(
        f64::from(value),
        (f64::from(thresholds.0), f64::from(thresholds.1)),
    );
    let width = cfg.column_panel.width.max(1) as usize;
    let index = (((i64::from(value) * 8) / 100).clamp(0, 7)) as usize;
    let style = font_size_style(cfg.column_panel.height);
    format!(
        r#"<span class="bar-{class}"{style}>{}</span>"#,
        block_ramp_at(index).to_string().repeat(width),
    )
}

/// Renders the `:spark` form HTML for a recent history buffer.
#[must_use]
pub fn spark_html(
    cfg: &Config,
    history: Option<&[i32]>,
    metric: TraceMetric,
    tooltip: bool,
) -> String {
    let Some(history) = history else {
        return String::new();
    };
    let length = metric.spark_length(cfg, tooltip);
    let recent = history_tail(history, length);
    let missing = length.saturating_sub(recent.len());
    let mut output = String::new();
    if missing > 0 {
        output.push_str(&format!(
            r#"<span class="spark-empty">{}</span>"#,
            BLOCK_RAMP[0].to_string().repeat(missing)
        ));
    }
    let thresholds = metric.spark_thresholds(cfg);
    for &value in recent {
        let index = (((i64::from(value) * 8) / 100).clamp(0, 7)) as usize;
        let class = css_class_from_thresholds(
            f64::from(value),
            (f64::from(thresholds.0), f64::from(thresholds.1)),
        );
        output.push_str(&format!(
            r#"<span class="bar-{class}">{}</span>"#,
            block_ramp_at(index)
        ));
    }
    output
}

/// Renders the `:braille` form HTML for a recent history buffer.
#[must_use]
pub fn braille_html(
    cfg: &Config,
    history: Option<&[i32]>,
    metric: TraceMetric,
    tooltip: bool,
    chars: Option<usize>,
) -> String {
    let Some(history) = history else {
        return String::new();
    };
    let base_len = chars.unwrap_or_else(|| metric.braille_length(cfg, tooltip));
    let length = base_len.saturating_mul(BRAILLE_LENGTH_MULTIPLIER as usize);
    let recent = history_tail(history, length);
    let mut padded = Vec::with_capacity(length.max(recent.len()));
    padded.resize(length.saturating_sub(recent.len()), None);
    padded.extend(recent.iter().copied().map(Some));
    if padded.len() % 2 != 0 {
        padded.insert(0, None);
    }

    let mut output = String::new();
    for pair in padded.chunks_exact(2) {
        let left = pair[0];
        let right = pair[1];
        if left.is_none() && right.is_none() {
            output.push('\u{2800}');
            continue;
        }
        let grade = [left, right]
            .into_iter()
            .flatten()
            .map(|value| ((value * BRAILLE_GRADES) / 100).clamp(0, BRAILLE_GRADES - 1))
            .max()
            .unwrap_or(0);
        output.push_str(&format!(
            r#"<span class="grad-{}-{grade}">{}</span>"#,
            metric.gradient_prefix(),
            braille_char(left, right),
        ));
    }
    output
}

/// Builds a standalone one-cell row for the `:bar` form.
#[must_use]
pub fn bar_row(
    cfg: &Config,
    value: Option<i32>,
    thresholds: (i32, i32),
    tooltip: bool,
    ident: &Ident,
) -> Vec<Row> {
    standalone(
        &bar_html(cfg, value, thresholds, tooltip),
        ident,
        bar_layout_width(cfg, tooltip),
    )
}

/// Builds a standalone one-cell row for the `:column` form.
#[must_use]
pub fn column_row(
    cfg: &Config,
    value: Option<i32>,
    thresholds: (i32, i32),
    ident: &Ident,
) -> Vec<Row> {
    standalone(&column_html(cfg, value, thresholds), ident, None)
}

/// Builds a standalone one-cell row for the `:spark` form.
#[must_use]
pub fn spark_row(
    cfg: &Config,
    history: Option<&[i32]>,
    metric: TraceMetric,
    tooltip: bool,
    ident: &Ident,
) -> Vec<Row> {
    standalone(&spark_html(cfg, history, metric, tooltip), ident, None)
}

/// Builds a standalone one-cell row for the `:braille` form.
#[must_use]
pub fn braille_row(
    cfg: &Config,
    history: Option<&[i32]>,
    metric: TraceMetric,
    tooltip: bool,
    ident: &Ident,
) -> Vec<Row> {
    standalone(
        &braille_html(cfg, history, metric, tooltip, None),
        ident,
        None,
    )
}

fn bar_history_row(
    cfg: &Config,
    metric: TraceMetric,
    bar: &str,
    history: &str,
    history_form: &str,
    tooltip: bool,
) -> Vec<Row> {
    if bar.is_empty() || history.is_empty() {
        return Vec::new();
    }
    let bar_ident = Ident::new(metric.usage_metric(), Some("bar"));
    let history_value_ident = Ident::new(metric.usage_metric(), Some(history_form));
    let live_label = trace_label_cell(
        cfg,
        metric,
        "bar",
        tooltip,
        table_text(&cfg.labels, metric.usage_metric()),
    );
    let history_label = trace_label_cell(
        cfg,
        metric,
        "spark",
        tooltip,
        table_text(&cfg.labels, "history"),
    );
    vec![render_two_pair_row(
        live_label,
        auxiliary_cell(bar, None, Some(&bar_ident), 1, 2, None),
        history_label,
        auxiliary_cell(history, None, Some(&history_value_ident), 2, 0, None),
    )]
}

/// Builds the combined `Live: <bar>  History: <spark>` row.
#[must_use]
pub fn bar_spark_row(
    cfg: &Config,
    metric: TraceMetric,
    value: Option<i32>,
    thresholds: (i32, i32),
    history: Option<&[i32]>,
    tooltip: bool,
) -> Vec<Row> {
    let bar = bar_html(cfg, value, thresholds, tooltip);
    let spark = spark_html(cfg, history, metric, tooltip);
    bar_history_row(cfg, metric, &bar, &spark, "spark", tooltip)
}

/// Builds the combined `Live: <bar>  History: <braille>` row.
#[must_use]
pub fn bar_braille_row(
    cfg: &Config,
    metric: TraceMetric,
    value: Option<i32>,
    thresholds: (i32, i32),
    history: Option<&[i32]>,
    tooltip: bool,
) -> Vec<Row> {
    let bar = bar_html(cfg, value, thresholds, tooltip);
    let braille = braille_html(cfg, history, metric, tooltip, None);
    bar_history_row(cfg, metric, &bar, &braille, "braille", tooltip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::render::model::Align;
    use toml::Value;

    fn ident(metric: &str, form: &str) -> Ident {
        Ident::new(metric, Some(form))
    }

    fn cfg() -> Config {
        Config::default()
    }

    fn cfg_with_small_tooltip_bar() -> Config {
        let mut cfg = cfg();
        cfg.bar_tooltip.width = 6;
        cfg.bar_tooltip.height = 3;
        cfg.display.panel_font_size = 4;
        cfg
    }

    #[test]
    fn bar_html_is_empty_without_value_or_width() {
        let mut cfg = cfg();
        cfg.bar_tooltip.width = 6;
        assert!(bar_html(&cfg, None, (50, 70), true).is_empty());

        cfg.bar_tooltip.width = 0;
        assert!(bar_html(&cfg, Some(50), (50, 70), true).is_empty());
    }

    #[test]
    fn bar_and_column_html_match_fixed_python_bytes() {
        let cfg = cfg_with_small_tooltip_bar();

        assert_eq!(
            bar_html(&cfg, Some(50), (50, 70), true),
            r#"<span class="bar-warn" style="font-size:3px">███</span><span class="bar-empty" style="font-size:3px">░░░</span>"#
        );
        assert_eq!(
            bar_html(&cfg, Some(83), (50, 70), false),
            r#"<span class="bar-crit">██████████████████</span><span class="bar-empty">░░░░</span>"#
        );
        assert_eq!(
            column_html(&cfg, Some(83), (50, 70)),
            r#"<span class="bar-crit">▇</span>"#
        );
    }

    #[test]
    fn spark_html_is_empty_without_history() {
        assert!(spark_html(&cfg(), None, TraceMetric::Cpu, true).is_empty());
    }

    #[test]
    fn spark_html_matches_fixed_python_bytes_for_cpu_and_mem() {
        let cfg = cfg_with_small_tooltip_bar();

        assert_eq!(
            spark_html(&cfg, Some(&[10, 20, 30]), TraceMetric::Cpu, true),
            r#"<span class="spark-empty">▁▁</span><span class="bar-good">▁</span><span class="bar-good">▂</span><span class="bar-good">▃</span>"#
        );
        assert_eq!(
            spark_html(&cfg, Some(&[15, 42, 61]), TraceMetric::Memory, false),
            r#"<span class="spark-empty">▁▁</span><span class="bar-good">▂</span><span class="bar-warn">▄</span><span class="bar-crit">▅</span>"#
        );
    }

    #[test]
    fn braille_helpers_match_python_boundaries() {
        assert_eq!(braille_level(0), 1);
        assert_eq!(braille_level(1), 1);
        assert_eq!(braille_level(25), 1);
        assert_eq!(braille_level(26), 2);
        assert_eq!(braille_level(100), 4);
        assert_eq!(braille_char(Some(10), Some(20)), '⣀');
        assert_eq!(braille_char(Some(50), Some(60)), '⣴');
        assert_eq!(braille_char(None, None), '⠀');
    }

    #[test]
    fn braille_html_matches_fixed_python_bytes_and_char_override() {
        let cfg = cfg_with_small_tooltip_bar();

        assert_eq!(
            braille_html(
                &cfg,
                Some(&[10, 20, 30, 40, 50, 60]),
                TraceMetric::Cpu,
                true,
                None
            ),
            r#"⠀⠀<span class="grad-cpu-1">⣀</span><span class="grad-cpu-3">⣤</span><span class="grad-cpu-4">⣴</span>"#
        );
        assert_eq!(
            braille_html(&cfg, Some(&[15, 42, 61]), TraceMetric::Memory, false, None),
            r#"⠀⠀⠀<span class="grad-mem-1">⢀</span><span class="grad-mem-4">⣴</span>"#
        );
        assert_eq!(
            braille_html(
                &cfg,
                Some(&[10, 20, 30, 40]),
                TraceMetric::Cpu,
                true,
                Some(1)
            ),
            r#"<span class="grad-cpu-3">⣤</span>"#
        );
    }

    #[test]
    fn bar_layout_width_matches_python_half_even_rounding() {
        let mut cfg = cfg();
        cfg.display.panel_font_size = 4;
        cfg.bar_tooltip.width = 10;
        cfg.bar_tooltip.height = 1;
        assert_eq!(bar_layout_width(&cfg, true), Some(2));

        cfg.bar_tooltip.width = 14;
        assert_eq!(bar_layout_width(&cfg, true), Some(4));

        cfg.bar_tooltip.height = 0;
        assert_eq!(bar_layout_width(&cfg, true), None);
    }

    #[test]
    fn standalone_rows_collapse_on_empty_html() {
        assert!(standalone("", &ident("cpu_usage", "bar"), None).is_empty());
    }

    #[test]
    fn standalone_row_builders_match_fixed_python_rows() {
        let cfg = cfg_with_small_tooltip_bar();

        assert_eq!(
            bar_row(&cfg, Some(50), (50, 70), true, &ident("cpu_usage", "bar")),
            vec![vec![Cell {
                text: String::from(
                    r#"<span class="bar-warn" style="font-size:3px">███</span><span class="bar-empty" style="font-size:3px">░░░</span>"#,
                ),
                css_class: Some(String::from("aux item-cpu_usage form-bar")),
                align: Align::Left,
                pad_left: 0,
                pad_right: 0,
                min_width: 0,
                layout_width: Some(4),
            }]],
        );

        assert_eq!(
            spark_row(
                &cfg,
                Some(&[10, 20, 30]),
                TraceMetric::Cpu,
                true,
                &ident("cpu_usage", "spark"),
            ),
            vec![vec![Cell {
                text: String::from(
                    r#"<span class="spark-empty">▁▁</span><span class="bar-good">▁</span><span class="bar-good">▂</span><span class="bar-good">▃</span>"#,
                ),
                css_class: Some(String::from("aux item-cpu_usage form-spark")),
                align: Align::Left,
                pad_left: 0,
                pad_right: 0,
                min_width: 0,
                layout_width: None,
            }]],
        );

        assert_eq!(
            braille_row(
                &cfg,
                Some(&[10, 20, 30, 40]),
                TraceMetric::Cpu,
                true,
                &ident("cpu_usage", "braille"),
            ),
            vec![vec![Cell {
                text: String::from(
                    r#"⠀⠀⠀<span class="grad-cpu-1">⣀</span><span class="grad-cpu-3">⣤</span>"#,
                ),
                css_class: Some(String::from("aux item-cpu_usage form-braille")),
                align: Align::Left,
                pad_left: 0,
                pad_right: 0,
                min_width: 0,
                layout_width: None,
            }]],
        );
    }

    #[test]
    fn combo_rows_drop_half_built_rows_and_share_presence_logic() {
        let mut cfg = cfg();
        cfg.bar_tooltip.width = 0;
        assert!(
            bar_spark_row(
                &cfg,
                TraceMetric::Cpu,
                Some(50),
                (50, 70),
                Some(&[10, 20]),
                true
            )
            .is_empty()
        );

        let cfg = cfg_with_small_tooltip_bar();
        assert!(!bar_row(&cfg, Some(50), (50, 70), true, &ident("cpu_usage", "bar")).is_empty());
        assert!(
            !spark_row(
                &cfg,
                Some(&[10, 20]),
                TraceMetric::Cpu,
                true,
                &ident("cpu_usage", "spark")
            )
            .is_empty()
        );
        assert!(
            !bar_spark_row(
                &cfg,
                TraceMetric::Cpu,
                Some(50),
                (50, 70),
                Some(&[10, 20]),
                true
            )
            .is_empty()
        );
    }

    #[test]
    fn combo_rows_match_fixed_python_structure() {
        let cfg = cfg_with_small_tooltip_bar();

        assert_eq!(
            bar_spark_row(
                &cfg,
                TraceMetric::Cpu,
                Some(50),
                (50, 70),
                Some(&[10, 20, 30]),
                true
            ),
            vec![vec![
                Cell {
                    text: String::from(" "),
                    css_class: Some(String::from("label item-cpu_usage form-bar")),
                    align: Align::Left,
                    pad_left: 0,
                    pad_right: 0,
                    min_width: 0,
                    layout_width: None,
                },
                Cell {
                    text: String::from(
                        r#"<span class="bar-warn" style="font-size:3px">███</span><span class="bar-empty" style="font-size:3px">░░░</span>"#,
                    ),
                    css_class: Some(String::from("aux item-cpu_usage form-bar")),
                    align: Align::Left,
                    pad_left: 1,
                    pad_right: 2,
                    min_width: 0,
                    layout_width: None,
                },
                Cell {
                    text: String::from(" "),
                    css_class: Some(String::from("label item-cpu_usage form-spark")),
                    align: Align::Left,
                    pad_left: 0,
                    pad_right: 0,
                    min_width: 0,
                    layout_width: None,
                },
                Cell {
                    text: String::from(
                        r#"<span class="spark-empty">▁▁</span><span class="bar-good">▁</span><span class="bar-good">▂</span><span class="bar-good">▃</span>"#,
                    ),
                    css_class: Some(String::from("aux item-cpu_usage form-spark")),
                    align: Align::Left,
                    pad_left: 2,
                    pad_right: 0,
                    min_width: 0,
                    layout_width: None,
                },
            ]],
        );

        assert_eq!(
            bar_braille_row(
                &cfg,
                TraceMetric::Memory,
                Some(50),
                (40, 60),
                Some(&[10, 20, 30, 40]),
                true
            ),
            vec![vec![
                Cell {
                    text: String::from(" "),
                    css_class: Some(String::from("label item-mem_usage form-bar")),
                    align: Align::Left,
                    pad_left: 0,
                    pad_right: 0,
                    min_width: 0,
                    layout_width: None,
                },
                Cell {
                    text: String::from(
                        r#"<span class="bar-warn" style="font-size:3px">███</span><span class="bar-empty" style="font-size:3px">░░░</span>"#,
                    ),
                    css_class: Some(String::from("aux item-mem_usage form-bar")),
                    align: Align::Left,
                    pad_left: 1,
                    pad_right: 2,
                    min_width: 0,
                    layout_width: None,
                },
                Cell {
                    text: String::from(" "),
                    css_class: Some(String::from("label item-mem_usage form-spark")),
                    align: Align::Left,
                    pad_left: 0,
                    pad_right: 0,
                    min_width: 0,
                    layout_width: None,
                },
                Cell {
                    text: String::from(
                        r#"⠀⠀⠀<span class="grad-mem-1">⣀</span><span class="grad-mem-3">⣤</span>"#,
                    ),
                    css_class: Some(String::from("aux item-mem_usage form-braille")),
                    align: Align::Left,
                    pad_left: 2,
                    pad_right: 0,
                    min_width: 0,
                    layout_width: None,
                },
            ]],
        );
    }

    #[test]
    fn combo_rows_pull_icons_and_labels_from_config() {
        let mut cfg = cfg_with_small_tooltip_bar();
        cfg.icons
            .insert(String::from("cpu_usage"), Value::String(String::from("")));
        cfg.labels.insert(
            String::from("cpu_usage"),
            Value::String(String::from("CPU")),
        );
        cfg.labels.insert(
            String::from("history"),
            Value::String(String::from("History")),
        );
        cfg.labels
            .insert(String::from("delimiter"), Value::String(String::from(":")));

        let rows = bar_spark_row(
            &cfg,
            TraceMetric::Cpu,
            Some(50),
            (50, 70),
            Some(&[10, 20]),
            true,
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0].text, " CPU:");
        assert_eq!(rows[0][2].text, " History:");
    }
}
