use super::*;
use crate::render::model::{Entry, Row, Separator, SeparatorSize, group_rows_into_blocks};

fn label(text: &str) -> Cell {
    Cell::classified(text, "label")
}

fn value(text: &str) -> Cell {
    let mut cell = Cell::classified(text, "val");
    cell.align = Align::Right;
    cell
}

fn blocks(rows: Vec<Row>) -> Vec<Block> {
    group_rows_into_blocks(rows.into_iter().map(Entry::Row))
}

fn div_line_widths(html: &str) -> Vec<usize> {
    html.split("<div>")
        .skip(1)
        .filter_map(|fragment| fragment.split_once("</div>").map(|(line, _)| line))
        .map(visible_width)
        .collect()
}

#[test]
fn plain_rows_are_table_free_and_share_a_right_edge() {
    let blocks = blocks(vec![
        vec![label("A:"), value("1%")],
        vec![label("LongerLabel:"), value("100%")],
    ]);

    let html = render_blocks_monospace(&blocks, 0);
    let widths = div_line_widths(&html);

    assert!(!html.contains("<table"));
    assert_eq!(widths.len(), 2);
    assert_eq!(widths[0], widths[1]);
    assert!(widths[0] >= "LongerLabel:".len() + "100%".len());
    assert!(html.contains(r#">100%</span></div>"#));
}

#[test]
fn two_pair_row_splits_across_two_halves() {
    let blocks = blocks(vec![vec![
        label("Up:"),
        value("9K"),
        label("Down:"),
        value("1K"),
    ]]);

    let html = render_blocks_monospace(&blocks, 20);

    assert_eq!(div_line_widths(&html), vec![20]);
    for token in ["Up:", "9K", "Down:", "1K"] {
        assert!(html.contains(token));
    }
}

#[test]
fn four_left_aligned_cells_use_left_plan_not_two_pair() {
    let blocks = blocks(vec![vec![
        label("Cpu"),
        Cell::classified("▇", "aux"),
        label("Mem"),
        Cell::classified("▁", "aux"),
    ]]);

    let html = render_blocks_monospace(&blocks, 30);

    assert_eq!(div_line_widths(&html), vec![8]);
    assert!(!html.contains("&nbsp;"));
}

#[test]
fn center_middle_rows_align_slashes_against_block_value_width() {
    let mut middle1 = Cell::classified("1G / 9G", "aux");
    middle1.align = Align::Center;
    let mut middle2 = Cell::classified("10G / 99G", "aux");
    middle2.align = Align::Center;
    let blocks = blocks(vec![
        vec![label("Root:"), middle1, value("9%")],
        vec![label("Backup:"), middle2, value("100%")],
    ]);

    let html = render_blocks_monospace(&blocks, 30);

    assert_eq!(div_line_widths(&html), vec![30, 30]);
    assert!(html.contains("1G / 9G"));
    assert!(html.contains("10G / 99G"));
}

#[test]
fn layout_width_overrides_small_font_character_count() {
    let mut bar = Cell::classified("██████████", "aux");
    bar.layout_width = Some(2);
    let blocks = blocks(vec![vec![bar]]);

    assert_eq!(global_width_of(&blocks, 0), 2);
}

#[test]
fn explicit_small_and_big_separators_emit_only_requested_rules() {
    for (size, expected, absent) in [
        (
            SeparatorSize::Small,
            "separator-rule-small",
            "separator-rule-big",
        ),
        (
            SeparatorSize::Big,
            "separator-rule-big",
            "separator-rule-small",
        ),
    ] {
        let entries = vec![
            Entry::Row(vec![label("A:"), value("1")]),
            Entry::Separator(Separator { size }),
            Entry::Row(vec![label("B:"), value("2")]),
        ];
        let blocks = group_rows_into_blocks(entries);

        let html = render_blocks_monospace(&blocks, 0);

        assert!(html.contains(expected));
        assert!(!html.contains(absent));
    }
}

#[test]
fn title_is_left_aligned_and_rule_does_not_drive_width() {
    let blocks = blocks(vec![
        vec![Cell::classified("Title", "title")],
        vec![Cell::classified("", "title-rule")],
        vec![label("LongerLabel:"), value("100%")],
    ]);

    let html = render_blocks_monospace(&blocks, 0);

    assert!(html.contains(r#"<div><span class="title">Title</span></div>"#));
    assert!(html.contains(r#"<div width="100%" class="title-rule">&nbsp;</div>"#));
    assert!(html.contains(r#"<span class="val">100%</span></div>"#));
    assert_eq!(
        global_width_of(&blocks, 0),
        "LongerLabel:".len() + "100%".len()
    );
}

#[test]
fn minimum_width_floors_value_rows() {
    let blocks = blocks(vec![vec![label("A:"), value("1")]]);

    let html = render_blocks_monospace(&blocks, 12);

    assert_eq!(global_width_of(&blocks, 12), 12);
    assert_eq!(div_line_widths(&html), vec![12]);
}

#[test]
fn every_layout_plan_matches_fixed_python_byte_corpus() {
    let mut first_middle = Cell::classified("1G / 9G", "aux");
    first_middle.align = Align::Center;
    let mut second_middle = Cell::classified("10G / 99G", "aux");
    second_middle.align = Align::Center;
    let mut small_bar = Cell::classified(r#"<span class="bar-good">██</span>"#, "aux");
    small_bar.layout_width = Some(1);
    let entries = vec![
        Entry::Row(vec![Cell::classified("Title", "title")]),
        Entry::Row(vec![Cell::classified("", "title-rule")]),
        Entry::Separator(Separator {
            size: SeparatorSize::Small,
        }),
        Entry::Row(vec![label("A:"), value("9%")]),
        Entry::Row(vec![label("Long:"), value("100%")]),
        Entry::Row(vec![label("Disk:"), first_middle, value("9%")]),
        Entry::Row(vec![label("Disk 2:"), second_middle, value("100%")]),
        Entry::Row(vec![label("Up:"), value("9K"), label("Down:"), value("1K")]),
        Entry::Row(vec![
            label("Cpu"),
            small_bar,
            label("Mem"),
            Cell::classified("▁", "aux"),
        ]),
    ];
    let blocks = group_rows_into_blocks(entries);

    let actual = render_blocks_monospace(&blocks, 24);
    let expected = concat!(
        r#"<div><span class="title">Title</span></div>"#,
        r#"<div width="100%" class="title-rule">&nbsp;</div>"#,
        r#"<div width="100%" class="separator-rule-small">&nbsp;</div>"#,
        r#"<div><span class="label">A:</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;&nbsp;</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;</span>"#,
        r#"<span class="val">9%</span></div>"#,
        r#"<div><span class="label">Long:</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;</span>"#,
        r#"<span class="val">100%</span></div>"#,
        r#"<div><span class="label">Disk:</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;&nbsp;</span>"#,
        r#"<span class="aux">1G / 9G</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;</span>"#,
        r#"<span class="val">9%</span></div>"#,
        r#"<div><span class="label">Disk 2:</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;</span>"#,
        r#"<span class="aux">10G / 99G</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;</span>"#,
        r#"<span class="val">100%</span></div>"#,
        r#"<div><span class="label">Up:</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;</span>"#,
        r#"<span class="val">9K</span><span class="label">Down:</span>"#,
        r#"<span class="gap">&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;</span>"#,
        r#"<span class="val">1K</span></div>"#,
        r#"<div><span class="label">Cpu</span>"#,
        r#"<span class="aux"><span class="bar-good">██</span></span>"#,
        r#"<span class="label">Mem</span><span class="aux">▁</span></div>"#,
    );

    assert_eq!(actual.as_bytes(), expected.as_bytes());
}

#[test]
fn right_edges_remain_equal_across_width_sweep() {
    for label_width in 0..16 {
        for value_width in 1..6 {
            let first_label = "L".repeat(label_width);
            let first_value = "1".repeat(value_width);
            let blocks = blocks(vec![
                vec![label(&first_label), value(&first_value)],
                vec![label("fixed-label"), value("100%")],
            ]);

            let widths = div_line_widths(&render_blocks_monospace(&blocks, 20));

            assert_eq!(widths.len(), 2);
            assert_eq!(widths[0], widths[1]);
            assert!(widths[0] >= 20);
        }
    }
}
