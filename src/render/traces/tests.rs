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
