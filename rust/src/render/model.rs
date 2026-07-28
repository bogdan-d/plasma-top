//! Cells, rows, blocks, thresholds, and horizontal-panel serialization.

/// Text shown when a requested reading is unavailable.
pub const EMPTY_VALUE: &str = "--";

/// Reserved percentage width in the horizontal panel.
pub const PERCENT_PANEL_WIDTH: usize = 3;

/// Horizontal alignment used by the serializers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Align {
    /// Content starts at the left edge.
    #[default]
    Left,
    /// Content is centered in its available region.
    Center,
    /// Content ends at the right edge.
    Right,
}

/// One styled unit of renderable HTML text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// HTML text rendered inside the cell.
    pub text: String,
    /// Space-separated CSS classes applied to the cell span.
    pub css_class: Option<String>,
    /// Horizontal alignment used for structural padding.
    pub align: Align,
    /// Structural columns reserved before the text.
    pub pad_left: usize,
    /// Structural columns reserved after the text.
    pub pad_right: usize,
    /// Minimum horizontal-panel footprint.
    pub min_width: usize,
    /// Optional monospace footprint replacing the text's visible width.
    pub layout_width: Option<usize>,
}

impl Cell {
    /// Creates an unstyled, left-aligned cell.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            css_class: None,
            align: Align::Left,
            pad_left: 0,
            pad_right: 0,
            min_width: 0,
            layout_width: None,
        }
    }

    /// Creates a cell with the given CSS classes.
    #[must_use]
    pub fn classified(text: impl Into<String>, css_class: impl Into<String>) -> Self {
        Self {
            css_class: Some(css_class.into()),
            ..Self::new(text)
        }
    }
}

/// A row of render cells.
pub type Row = Vec<Cell>;

/// CSS identity on the metric and form axes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    metric: String,
    form: Option<String>,
}

impl Ident {
    /// Creates an identity from a metric CSS token and optional form token.
    #[must_use]
    pub fn new(metric: impl Into<String>, form: Option<&str>) -> Self {
        Self {
            metric: metric.into(),
            form: form.map(str::to_owned),
        }
    }

    /// Returns the combined `item-* form-*` CSS selector classes.
    #[must_use]
    pub fn css(&self) -> String {
        self.form.as_ref().map_or_else(
            || format!("item-{}", self.metric),
            |form| format!("item-{} form-{form}", self.metric),
        )
    }
}

/// Explicit separator size selected by a TOML separator entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeparatorSize {
    /// Thin separator rule.
    Small,
    /// Thick separator rule.
    Big,
}

impl SeparatorSize {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Small => "small",
            Self::Big => "big",
        }
    }
}

/// An explicit separator between render rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Separator {
    /// Rule size applied before the following block.
    pub size: SeparatorSize,
}

/// A row or explicit separator before block grouping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A row of cells.
    Row(Row),
    /// An explicit visual separator.
    Separator(Separator),
}

impl From<Row> for Entry {
    fn from(row: Row) -> Self {
        Self::Row(row)
    }
}

impl From<Separator> for Entry {
    fn from(separator: Separator) -> Self {
        Self::Separator(separator)
    }
}

/// Consecutive rows sharing one structural cell-role shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Rows laid out with shared column widths.
    pub rows: Vec<Row>,
    /// Explicit rule rendered before this block, if any.
    pub separator_size: Option<SeparatorSize>,
}

/// Returns the on-screen character width after removing tags and decoding
/// entities emitted by PlasmaTop renderers.
#[must_use]
pub fn visible_width(text: &str) -> usize {
    let without_tags = strip_tags(text);
    decoded_entity_width(&without_tags)
}

fn strip_tags(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character == '<' {
            let mut tag = String::from("<");
            let mut closed = false;
            for tag_character in chars.by_ref() {
                tag.push(tag_character);
                if tag_character == '>' {
                    closed = true;
                    break;
                }
            }
            if !closed || tag == "<>" {
                output.push_str(&tag);
            }
        } else {
            output.push(character);
        }
    }
    output
}

fn decoded_entity_width(text: &str) -> usize {
    let mut width = 0;
    let mut rest = text;
    while let Some(entity_start) = rest.find('&') {
        width += rest[..entity_start].chars().count();
        let entity = &rest[entity_start..];
        let Some(relative_end) = entity.find(';') else {
            return width + entity.chars().count();
        };
        let candidate = &entity[1..relative_end];
        if decoded_entity(candidate).is_some() {
            width += 1;
            rest = &entity[relative_end + 1..];
        } else {
            width += 1;
            rest = &entity[1..];
        }
    }
    width + rest.chars().count()
}

fn decoded_entity(entity: &str) -> Option<char> {
    match entity {
        "nbsp" => Some('\u{a0}'),
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        numeric if numeric.starts_with("#x") || numeric.starts_with("#X") => {
            u32::from_str_radix(&numeric[2..], 16)
                .ok()
                .and_then(char::from_u32)
        }
        numeric if numeric.starts_with('#') => {
            numeric[1..].parse::<u32>().ok().and_then(char::from_u32)
        }
        _ => None,
    }
}

pub(crate) fn non_breaking_spaces(count: usize) -> String {
    "&nbsp;".repeat(count)
}

pub(crate) fn cell_inner(cell: &Cell) -> String {
    let mut inner =
        String::with_capacity(cell.text.len() + (cell.pad_left + cell.pad_right) * "&nbsp;".len());
    inner.push_str(&non_breaking_spaces(cell.pad_left));
    inner.push_str(&cell.text);
    inner.push_str(&non_breaking_spaces(cell.pad_right));
    inner
}

/// Builds a right-aligned value cell with role, identity, and state classes.
#[must_use]
pub fn value_cell(
    text: impl Into<String>,
    state_class: Option<&str>,
    ident: Option<&Ident>,
    min_width: usize,
) -> Cell {
    let mut classes = String::from("val");
    if let Some(ident) = ident {
        classes.push(' ');
        classes.push_str(&ident.css());
    }
    if let Some(state_class) = state_class.filter(|class| !class.is_empty()) {
        classes.push(' ');
        classes.push_str(state_class);
    }
    Cell {
        css_class: Some(classes),
        align: Align::Right,
        min_width,
        ..Cell::new(text)
    }
}

/// Builds a left-aligned auxiliary cell used by traces and extra columns.
#[must_use]
pub fn auxiliary_cell(
    text: impl Into<String>,
    state_class: Option<&str>,
    ident: Option<&Ident>,
    pad_left: usize,
    pad_right: usize,
    layout_width: Option<usize>,
) -> Cell {
    let mut classes = String::from("aux");
    if let Some(ident) = ident {
        classes.push(' ');
        classes.push_str(&ident.css());
    }
    if let Some(state_class) = state_class.filter(|class| !class.is_empty()) {
        classes.push(' ');
        classes.push_str(state_class);
    }
    Cell {
        css_class: Some(classes),
        pad_left,
        pad_right,
        layout_width,
        ..Cell::new(text)
    }
}

/// Formats a percentage using the compact panel and explicit tooltip rules.
#[must_use]
pub fn format_percent(value: i64, tooltip: bool) -> String {
    if value >= 100 && !tooltip {
        value.to_string()
    } else {
        format!("{value}%")
    }
}

/// Maps a value onto the `good`, `warn`, or `crit` threshold band.
#[must_use]
pub fn css_class_from_thresholds(value: f64, thresholds: (f64, f64)) -> &'static str {
    let (middle, high) = thresholds;
    if value >= high {
        "crit"
    } else if value >= middle {
        "warn"
    } else {
        "good"
    }
}

/// Returns `active` only when the value is strictly above the threshold.
#[must_use]
pub const fn css_class_active(value: i64, threshold: i64) -> Option<&'static str> {
    if value > threshold {
        Some("active")
    } else {
        None
    }
}

/// Maps battery charge onto inverted critical, warning, and good bands.
#[must_use]
pub const fn css_class_battery(
    value: i64,
    low_threshold: i64,
    high_threshold: i64,
) -> &'static str {
    if value <= low_threshold {
        "crit"
    } else if value <= high_threshold {
        "warn"
    } else {
        "good"
    }
}

fn cell_role(cell: &Cell) -> &str {
    cell.css_class
        .as_deref()
        .and_then(|classes| classes.split_whitespace().next())
        .unwrap_or("")
}

fn row_shape(row: &[Cell]) -> Vec<&str> {
    row.iter().map(cell_role).collect()
}

/// Groups consecutive rows by structural role shape and applies explicit
/// separators to the block that follows them.
#[must_use]
pub fn group_rows_into_blocks(entries: impl IntoIterator<Item = Entry>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut current = Vec::new();
    let mut current_shape: Option<Vec<String>> = None;
    let mut next_separator = None;

    let flush = |blocks: &mut Vec<Block>,
                 current: &mut Vec<Row>,
                 current_shape: &mut Option<Vec<String>>,
                 next_separator: &mut Option<SeparatorSize>,
                 separator_after: Option<SeparatorSize>| {
        if !current.is_empty() {
            blocks.push(Block {
                rows: std::mem::take(current),
                separator_size: *next_separator,
            });
        }
        *current_shape = None;
        *next_separator = separator_after;
    };

    for entry in entries {
        match entry {
            Entry::Separator(separator) => flush(
                &mut blocks,
                &mut current,
                &mut current_shape,
                &mut next_separator,
                Some(separator.size),
            ),
            Entry::Row(row) => {
                let shape: Vec<String> = row_shape(&row).into_iter().map(str::to_owned).collect();
                if current_shape
                    .as_ref()
                    .is_some_and(|current| *current != shape)
                {
                    flush(
                        &mut blocks,
                        &mut current,
                        &mut current_shape,
                        &mut next_separator,
                        None,
                    );
                }
                current.push(row);
                current_shape = Some(shape);
            }
        }
    }
    flush(
        &mut blocks,
        &mut current,
        &mut current_shape,
        &mut next_separator,
        None,
    );
    blocks
}

/// Assembles two adjacent label/value pairs.
#[must_use]
pub fn render_two_pair_row(label1: Cell, value1: Cell, label2: Cell, value2: Cell) -> Row {
    vec![label1, value1, label2, value2]
}

/// Assembles a label, auxiliary cell, and right-side value.
#[must_use]
pub fn render_three_col_row(label: Cell, mut extra: Cell, value: Cell) -> Row {
    if !extra.text.is_empty() {
        extra.pad_left = 1;
    }
    vec![label, extra, value]
}

pub(crate) fn separator_rule_html(size: SeparatorSize) -> String {
    format!(
        r#"<div width="100%" class="separator-rule-{}">&nbsp;</div>"#,
        size.as_str()
    )
}

/// Serializes one horizontal-panel row as consecutive spans with explicit
/// non-breaking-space gaps.
#[must_use]
pub fn render_row_inline(row: &[Cell]) -> String {
    let mut parts = Vec::with_capacity(row.len());
    for cell in row {
        let attributes = cell
            .css_class
            .as_deref()
            .filter(|classes| !classes.is_empty())
            .map_or_else(String::new, |classes| format!(r#" class="{classes}""#));
        let mut inner = cell_inner(cell);
        let used_width = cell.pad_left + visible_width(&cell.text) + cell.pad_right;
        let deficit = cell.min_width.saturating_sub(used_width);
        if deficit > 0 {
            if cell.align == Align::Right {
                inner.insert_str(0, &non_breaking_spaces(deficit));
            } else {
                inner.push_str(&non_breaking_spaces(deficit));
            }
        }
        parts.push(format!("<span{attributes}>{inner}</span>"));
    }
    parts.join(r#"<span class="gap">&nbsp;</span>"#)
}

#[cfg(test)]
mod tests {
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
}
