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
mod tests;
