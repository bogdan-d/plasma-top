//! Table-free monospace serializer for tooltips and vertical panels.

use super::model::{
    Align, Block, Cell, cell_inner, non_breaking_spaces, separator_rule_html, visible_width,
};

fn padding(count: usize) -> String {
    if count == 0 {
        String::new()
    } else {
        format!(r#"<span class="gap">{}</span>"#, non_breaking_spaces(count))
    }
}

fn cell_width(cell: &Cell) -> usize {
    cell.pad_left
        + cell
            .layout_width
            .unwrap_or_else(|| visible_width(&cell.text))
        + cell.pad_right
}

fn span(cell: &Cell) -> String {
    let inner = cell_inner(cell);
    cell.css_class
        .as_deref()
        .filter(|classes| !classes.is_empty())
        .map_or(inner.clone(), |classes| {
            format!(r#"<span class="{classes}">{inner}</span>"#)
        })
}

fn is_title_rule(cell: &Cell) -> bool {
    cell.css_class
        .as_deref()
        .and_then(|classes| classes.split_whitespace().next())
        == Some("title-rule")
}

fn is_two_pair(row: &[Cell]) -> bool {
    row.len() == 4 && row[1].align == Align::Right && row[3].align == Align::Right
}

fn column_widths(block: &Block) -> Vec<usize> {
    let column_count = block.rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0; column_count];
    for row in &block.rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell_width(cell));
        }
    }
    widths
}

fn render_columns(cells: &[Cell], widths: &[usize]) -> (String, usize) {
    let mut html = String::new();
    for (cell, width) in cells.iter().zip(widths) {
        let gap = width.saturating_sub(cell_width(cell));
        if cell.align == Align::Right {
            html.push_str(&padding(gap));
            html.push_str(&span(cell));
        } else {
            html.push_str(&span(cell));
            html.push_str(&padding(gap));
        }
    }
    (html, widths.iter().sum())
}

enum Plan<'a> {
    Left {
        natural_width: usize,
        html: String,
    },
    RightValue {
        natural_width: usize,
        left_html: String,
        left_width: usize,
        value_html: String,
        value_width: usize,
    },
    CenterMiddle {
        natural_width: usize,
        left_html: String,
        left_width: usize,
        middle_html: String,
        middle_width: usize,
        value_html: String,
        value_width: usize,
        value_column_width: usize,
    },
    TwoPair {
        natural_width: usize,
        first_label: &'a Cell,
        first_value: &'a Cell,
        second_label: &'a Cell,
        second_value: &'a Cell,
    },
    TitleRule,
}

impl Plan<'_> {
    const fn natural_width(&self) -> usize {
        match self {
            Self::Left { natural_width, .. }
            | Self::RightValue { natural_width, .. }
            | Self::CenterMiddle { natural_width, .. }
            | Self::TwoPair { natural_width, .. } => *natural_width,
            Self::TitleRule => 0,
        }
    }

    fn emit(&self, global_width: usize) -> String {
        match self {
            Self::TwoPair {
                first_label,
                first_value,
                second_label,
                second_value,
                ..
            } => {
                let first_half = global_width / 2;
                let first_gap =
                    first_half.saturating_sub(cell_width(first_label) + cell_width(first_value));
                let second_gap = (global_width - first_half)
                    .saturating_sub(cell_width(second_label) + cell_width(second_value));
                format!(
                    "<div>{}{}{}{}{}{}</div>",
                    span(first_label),
                    padding(first_gap),
                    span(first_value),
                    span(second_label),
                    padding(second_gap),
                    span(second_value)
                )
            }
            Self::RightValue {
                left_html,
                left_width,
                value_html,
                value_width,
                ..
            } => {
                let middle = global_width.saturating_sub(left_width + value_width);
                format!("<div>{left_html}{}{value_html}</div>", padding(middle))
            }
            Self::CenterMiddle {
                left_html,
                left_width,
                middle_html,
                middle_width,
                value_html,
                value_width,
                value_column_width,
                ..
            } => {
                let region = global_width.saturating_sub(value_column_width + left_width);
                let before_middle = region.saturating_sub(*middle_width) / 2;
                let after_middle = global_width
                    .saturating_sub(value_width + left_width + before_middle + middle_width);
                format!(
                    "<div>{left_html}{}{middle_html}{}{value_html}</div>",
                    padding(before_middle),
                    padding(after_middle)
                )
            }
            Self::TitleRule => String::from(r#"<div width="100%" class="title-rule">&nbsp;</div>"#),
            Self::Left { html, .. } => format!("<div>{html}</div>"),
        }
    }
}

fn plan_row<'a>(row: &'a [Cell], widths: &[usize], value_column_width: usize) -> Plan<'a> {
    if row.len() == 3 && row[2].align == Align::Right && row[1].align == Align::Center {
        let (left_html, left_width) = render_columns(&row[..1], &widths[..1]);
        let middle_width = cell_width(&row[1]);
        let value_width = cell_width(&row[2]);
        return Plan::CenterMiddle {
            natural_width: left_width + middle_width + value_width.max(value_column_width),
            left_html,
            left_width,
            middle_html: span(&row[1]),
            middle_width,
            value_html: span(&row[2]),
            value_width,
            value_column_width,
        };
    }

    if let [cell] = row {
        if is_title_rule(cell) {
            return Plan::TitleRule;
        }
        return Plan::Left {
            natural_width: cell_width(cell),
            html: span(cell),
        };
    }

    if is_two_pair(row) {
        return Plan::TwoPair {
            natural_width: row.iter().map(cell_width).sum(),
            first_label: &row[0],
            first_value: &row[1],
            second_label: &row[2],
            second_value: &row[3],
        };
    }

    if let Some(last) = row.last().filter(|cell| cell.align == Align::Right) {
        let left_count = row.len() - 1;
        let (left_html, left_width) = render_columns(&row[..left_count], &widths[..left_count]);
        let value_width = cell_width(last);
        return Plan::RightValue {
            natural_width: left_width + value_width,
            left_html,
            left_width,
            value_html: span(last),
            value_width,
        };
    }

    let (html, natural_width) = render_columns(row, widths);
    Plan::Left {
        natural_width,
        html,
    }
}

fn block_value_column_width(block: &Block) -> usize {
    block
        .rows
        .iter()
        .filter_map(|row| {
            row.last()
                .filter(|cell| row.len() >= 2 && cell.align == Align::Right)
        })
        .map(cell_width)
        .max()
        .unwrap_or(0)
}

/// Returns the shared monospace width used to lay out all blocks.
#[must_use]
pub fn global_width_of(blocks: &[Block], min_width: usize) -> usize {
    let mut global_width = min_width;
    for block in blocks {
        let widths = column_widths(block);
        let value_column_width = block_value_column_width(block);
        for row in &block.rows {
            global_width =
                global_width.max(plan_row(row, &widths, value_column_width).natural_width());
        }
    }
    global_width
}

/// Serializes blocks to table-free, monospace-aligned HTML.
#[must_use]
pub fn render_blocks_monospace(blocks: &[Block], min_width: usize) -> String {
    let mut laid_out = Vec::with_capacity(blocks.len());
    let mut global_width = min_width;
    for block in blocks {
        let widths = column_widths(block);
        let value_column_width = block_value_column_width(block);
        let plans: Vec<_> = block
            .rows
            .iter()
            .map(|row| plan_row(row, &widths, value_column_width))
            .collect();
        for plan in &plans {
            global_width = global_width.max(plan.natural_width());
        }
        laid_out.push((block.separator_size, plans));
    }

    let mut output = String::new();
    for (separator_size, plans) in laid_out {
        if let Some(separator_size) = separator_size {
            output.push_str(&separator_rule_html(separator_size));
        }
        for plan in plans {
            output.push_str(&plan.emit(global_width));
        }
    }
    output
}

#[cfg(test)]
mod tests {
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
}
