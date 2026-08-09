use super::*;

fn row(count: usize) -> Row {
    (0..count)
        .map(|index| Cell::new(index.to_string()))
        .collect()
}

fn separator(size: SeparatorSize) -> Entry {
    Separator { size }.into()
}

#[test]
fn visible_width_strips_tags_and_decodes_entities() {
    assert_eq!(visible_width(r#"<span class="val crit">12%</span>"#), 3);
    assert_eq!(visible_width("a&nbsp;b"), 3);
    assert_eq!(visible_width("<b>x</b>&nbsp;&nbsp;"), 3);
    assert_eq!(visible_width("&lt;&#x41;&#65;&amp;"), 4);
    assert_eq!(visible_width(" ▇ ⣿"), 5);
    assert_eq!(visible_width("<>"), 2);
}

#[test]
fn threshold_boundaries_match_python() {
    assert_eq!(css_class_from_thresholds(10.0, (40.0, 70.0)), "good");
    assert_eq!(css_class_from_thresholds(40.0, (40.0, 70.0)), "warn");
    assert_eq!(css_class_from_thresholds(70.0, (40.0, 70.0)), "crit");
    assert_eq!(css_class_from_thresholds(100.0, (40.0, 70.0)), "crit");
    assert_eq!(css_class_active(2, 1), Some("active"));
    assert_eq!(css_class_active(1, 1), None);
    assert_eq!(css_class_battery(5, 20, 80), "crit");
    assert_eq!(css_class_battery(50, 20, 80), "warn");
    assert_eq!(css_class_battery(90, 20, 80), "good");
}

#[test]
fn cell_builders_preserve_role_identity_and_state_order() {
    let ident = Ident::new("cpu_usage", Some("value"));
    let value = value_cell("90%", Some("crit"), Some(&ident), 3);
    let auxiliary = auxiliary_cell("▇", None, Some(&ident), 1, 2, Some(1));

    assert_eq!(
        value.css_class.as_deref(),
        Some("val item-cpu_usage form-value crit")
    );
    assert_eq!(value.align, Align::Right);
    assert_eq!(value.min_width, 3);
    assert_eq!(
        auxiliary.css_class.as_deref(),
        Some("aux item-cpu_usage form-value")
    );
    assert_eq!((auxiliary.pad_left, auxiliary.pad_right), (1, 2));
    assert_eq!(auxiliary.layout_width, Some(1));
    assert_eq!(format_percent(100, false), "100");
    assert_eq!(format_percent(100, true), "100%");
    assert_eq!(format_percent(42, false), "42%");
}

#[test]
fn grouping_matches_shapes_and_explicit_separators() {
    let blocks = group_rows_into_blocks([
        Entry::Row(row(2)),
        Entry::Row(row(2)),
        separator(SeparatorSize::Small),
        Entry::Row(row(4)),
        Entry::Row(row(4)),
    ]);

    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].rows.len(), 2);
    assert_eq!(blocks[0].separator_size, None);
    assert_eq!(blocks[1].rows.len(), 2);
    assert_eq!(blocks[1].separator_size, Some(SeparatorSize::Small));
}

#[test]
fn grouping_splits_same_cell_count_with_different_roles() {
    let paired = vec![
        Cell::classified("Up", "label"),
        Cell::classified("12K", "val"),
        Cell::classified("Down", "label"),
        Cell::classified("1K", "val"),
    ];
    let traces = vec![
        Cell::classified("Cpu", "label"),
        Cell::classified("12%", "val"),
        Cell::classified("", "aux"),
        Cell::classified("bar", "aux"),
    ];

    let blocks = group_rows_into_blocks([paired.into(), traces.into()]);

    assert_eq!(blocks.len(), 2);
}

#[test]
fn grouping_ignores_state_classes_but_isolates_spanning_rows() {
    let good = vec![
        Cell::classified("Cpu", "label"),
        Cell::classified("12%", "val good"),
    ];
    let critical = vec![
        Cell::classified("Mem", "label"),
        Cell::classified("90%", "val crit"),
    ];
    let spanning = vec![Cell::classified("spark", "aux")];

    let merged = group_rows_into_blocks([good.clone().into(), critical.clone().into()]);
    let isolated = group_rows_into_blocks([good.into(), spanning.into(), critical.into()]);

    assert_eq!(merged.len(), 1);
    assert_eq!(isolated.len(), 3);
}

#[test]
fn grouping_discards_leading_trailing_and_empty_separators() {
    let blocks = group_rows_into_blocks([
        separator(SeparatorSize::Big),
        Entry::Row(row(2)),
        separator(SeparatorSize::Big),
    ]);
    let empty = group_rows_into_blocks(Vec::<Entry>::new());

    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks[0].separator_size, Some(SeparatorSize::Big));
    assert!(empty.is_empty());
}

#[test]
fn three_column_row_adds_gap_only_for_present_extra_text() {
    let nonempty = render_three_col_row(Cell::new("A"), Cell::new("B"), Cell::new("C"));
    let empty = render_three_col_row(Cell::new("A"), Cell::new(""), Cell::new("C"));

    assert_eq!(nonempty[1].pad_left, 1);
    assert_eq!(empty[1].pad_left, 0);
}

#[test]
fn inline_render_is_byte_identical_and_table_free() {
    let row = vec![
        Cell::classified("Cpu usage", "label"),
        Cell::classified("12%", "val good"),
    ];

    let html = render_row_inline(&row);

    assert_eq!(
        html,
        r#"<span class="label">Cpu usage</span><span class="gap">&nbsp;</span><span class="val good">12%</span>"#
    );
    assert!(!html.contains("<table"));
}

#[test]
fn inline_render_reserves_right_aligned_minimum_width() {
    let mut value = Cell::classified("9%", "val good");
    value.align = Align::Right;
    value.min_width = 3;

    assert_eq!(
        render_row_inline(&[value]),
        r#"<span class="val good">&nbsp;9%</span>"#
    );
}

#[test]
fn inline_render_separates_every_cell_in_multi_pair_row() {
    let row = render_two_pair_row(
        Cell::classified("Up", "label"),
        value_cell("12K", None, None, 0),
        Cell::classified("Down", "label"),
        value_cell("1K", None, None, 0),
    );

    assert_eq!(render_row_inline(&row).matches("&nbsp;").count(), 3);
}
