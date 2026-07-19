//! Table-free render model and monospace HTML serialization.

pub mod cells;
pub mod chart;
pub mod formatter;
pub mod model;
pub mod mono;
pub mod registry;
pub mod traces;

pub use chart::{
    AreaChartOptions, BLUE_FILL, BLUE_LINE, GREEN_FILL, GREEN_LINE, GRID, LABEL, ORANGE_LINE,
    PURPLE_FILL, PURPLE_LINE, RED_LINE, RGBA, TEAL_FILL, TEAL_LINE, area_chart_png,
};
pub use formatter::PanelFormatter;
pub use model::{
    Align, Block, Cell, EMPTY_VALUE, Entry, Ident, PERCENT_PANEL_WIDTH, Row, Separator,
    SeparatorSize, auxiliary_cell, css_class_active, css_class_battery, css_class_from_thresholds,
    format_percent, group_rows_into_blocks, render_row_inline, render_three_col_row,
    render_two_pair_row, value_cell, visible_width,
};
pub use mono::{global_width_of, render_blocks_monospace};
pub use traces::{
    TraceMetric, bar_braille_row, bar_html, bar_row, bar_spark_row, braille_html, braille_row,
    column_html, column_row, spark_html, spark_row,
};
