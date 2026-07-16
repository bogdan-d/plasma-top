import re

from render_model import (
    Cell,
    Separator,
    css_class_active,
    css_class_battery,
    css_class_from_thresholds,
    group_rows_into_blocks,
    render_row_inline,
)


# ── css_class_from_thresholds ────────────────────────────────────────────────

def test_css_class_below_mid_is_good():
    assert css_class_from_thresholds(10, (40, 70)) == "good"


def test_css_class_at_mid_boundary_is_warn():
    assert css_class_from_thresholds(40, (40, 70)) == "warn"


def test_css_class_at_high_boundary_is_crit():
    assert css_class_from_thresholds(70, (40, 70)) == "crit"


def test_css_class_above_high_is_crit():
    assert css_class_from_thresholds(100, (40, 70)) == "crit"


# ── css_class_active ─────────────────────────────────────────────────────────

def test_css_class_active_above_threshold():
    assert css_class_active(2, 1) == "active"


def test_css_class_active_at_or_below_threshold():
    assert css_class_active(1, 1) is None


# ── css_class_battery (inverted: low charge is bad) ──────────────────────────

def test_css_class_battery_low_charge_is_crit():
    assert css_class_battery(5, 20, 80) == "crit"


def test_css_class_battery_mid_charge_is_warn():
    assert css_class_battery(50, 20, 80) == "warn"


def test_css_class_battery_high_charge_is_good():
    assert css_class_battery(90, 20, 80) == "good"


# ── group_rows_into_blocks ───────────────────────────────────────────────────

def _row(n: int) -> list[Cell]:
    return [Cell(text=str(i)) for i in range(n)]


def test_consecutive_same_shape_rows_form_one_block():
    blocks = group_rows_into_blocks([_row(2), _row(2), _row(2)])
    assert len(blocks) == 1
    assert len(blocks[0].rows) == 3


def test_separator_splits_into_two_blocks():
    blocks = group_rows_into_blocks([_row(2), Separator(size="big"), _row(2)])
    assert len(blocks) == 2
    assert len(blocks[0].rows) == 1
    assert len(blocks[1].rows) == 1


def test_shape_change_splits_without_explicit_separator():
    blocks = group_rows_into_blocks([_row(2), _row(2), _row(4), _row(4)])
    assert len(blocks) == 2
    assert len(blocks[0].rows) == 2
    assert len(blocks[1].rows) == 2


def test_spanning_row_gets_its_own_block():
    """A full-width spark row (a single cell) must not share a block with
    the surrounding 2-cell rows — sharing a block means sharing a column
    layout, and a spanning row has a different shape (a one-element role
    tuple). It always gets its own isolated block, even though it's visually
    meant to sit right above the row that follows (no separator there, so no
    visible gap either)."""
    span = [Cell(text="spark", css_class="aux")]
    blocks = group_rows_into_blocks([_row(2), span, _row(2), _row(2)])
    assert len(blocks) == 3
    assert len(blocks[0].rows) == 1   # the leading _row(2)
    assert len(blocks[1].rows) == 1   # the spanning row, alone
    assert len(blocks[2].rows) == 2   # the trailing two _row(2)


def test_same_cell_count_but_different_roles_splits():
    """Real bug caught via end-to-end smoke test: net_speed (label,val,label,val)
    and cpu_usage with bar+spark (label,val,aux,aux) both have 4 cells
    but mean different things — must not end up in the same block."""
    net_speed_row = [
        Cell(text="Up", css_class="label"), Cell(text="12K", css_class="val"),
        Cell(text="Down", css_class="label"), Cell(text="1K", css_class="val"),
    ]
    cpu_usage_row = [
        Cell(text="Cpu", css_class="label"), Cell(text="12%", css_class="val"),
        Cell(text="", css_class="aux"), Cell(text="bar", css_class="aux"),
    ]
    blocks = group_rows_into_blocks([net_speed_row, cpu_usage_row])
    assert len(blocks) == 2


def test_same_role_pattern_merges_even_with_different_state_classes():
    row1 = [Cell(text="Cpu", css_class="label"), Cell(text="12%", css_class="val good")]
    row2 = [Cell(text="Mem", css_class="label"), Cell(text="90%", css_class="val crit")]
    blocks = group_rows_into_blocks([row1, row2])
    assert len(blocks) == 1


def test_separator_marks_following_block_with_its_size():
    blocks = group_rows_into_blocks([_row(2), Separator(size="small"), _row(2)])
    assert blocks[0].separator_size is None
    assert blocks[1].separator_size == "small"


def test_shape_change_does_not_set_separator_size():
    blocks = group_rows_into_blocks([_row(2), _row(4)])
    assert blocks[1].separator_size is None


def test_leading_and_trailing_separators_produce_no_empty_block():
    blocks = group_rows_into_blocks([Separator(size="big"), _row(2), Separator(size="big")])
    assert len(blocks) == 1


def test_empty_input_produces_no_blocks():
    assert group_rows_into_blocks([]) == []


# ── render_row_inline (horizontal panel) ─────────────────────────────────────

def test_render_row_inline_no_table_tags():
    row = [Cell(text="Cpu usage", css_class="label"), Cell(text="12%", css_class="val good")]
    html = render_row_inline(row)
    assert "<table" not in html
    assert html == ('<span class="label">Cpu usage</span>'
                    '<span class="gap">&nbsp;</span>'
                    '<span class="val good">12%</span>')


def test_render_row_inline_separates_multi_cell_rows():
    """net_speed-style row (label,val,label,val): cells need a gap between
    them since there's no table border-spacing to provide it."""
    row = [Cell(text="Up", css_class="label"), Cell(text="12K", css_class="val"),
           Cell(text="Down", css_class="label"), Cell(text="1K", css_class="val")]
    html = render_row_inline(row)
    assert html.count("&nbsp;") == 3


def test_render_row_inline_reserves_min_width_fixed_footprint():
    """A right-aligned value cell with min_width keeps the same on-screen width
    as its content's digit count changes (e.g. cpu_usage 9% -> 10% -> 100), so
    the horizontal panel doesn't shift neighbours. The deficit is leading
    &nbsp; (right alignment), inside the value span."""
    def val_span(text):
        cell = Cell(text=text, css_class="val good", align="right", min_width=3)
        html = render_row_inline([cell])
        return re.search(r'<span class="val good">(.*?)</span>', html).group(1)

    assert val_span("9%") == "&nbsp;9%"   # 2 chars -> padded to 3
    assert val_span("10%") == "10%"        # already 3
    assert val_span("100") == "100"        # already 3
    # no min_width (default 0): no padding, footprint follows content
    bare = render_row_inline([Cell(text="9%", css_class="val", align="right")])
    assert '<span class="val">9%</span>' in bare
