"""
Cell/Row/Block model shared by the panel and tooltip renderers, plus the
pure value->CSS-class threshold functions. The HTML serialization itself
lives elsewhere: table-free monospace in mono_render (tooltip + vertical
panel), inline <span> in render_row_inline (horizontal panel). No Config/
color dependency here: colors live only in style-dark.css.
"""
from __future__ import annotations

import html as _html
import re as _re
from dataclasses import dataclass
from typing import NamedTuple, Optional


# What a value cell shows when the reading is missing (sensor absent, not read
# yet, hardware silent). One placeholder for every item on both surfaces, so a
# gap always reads the same way; it's the model's vocabulary, not a setting.
EMPTY_VALUE = "--"


@dataclass
class Cell:
    text: str
    css_class: Optional[str] = None
    align: str = "left"   # "left" | "right"
    # Structural inter-cell spacing, kept OUT of `text`: a minimum cosmetic gap
    # in monospace columns (one &nbsp; each), materialized at render time by the
    # serializers (mono_render / render_row_inline), never baked into the
    # content. Separating it from `text` keeps the visible-width math on real
    # content and makes the spacing explicit and inspectable. pad_left is the
    # leading gap (e.g. a value column sitting flush after a left-aligned
    # label); pad_right the trailing one (e.g. keeping a right-aligned value
    # from gluing onto the next half's label in a two-pair row).
    pad_left: int = 0
    pad_right: int = 0
    # Minimum on-screen width the cell reserves regardless of its current
    # content, so a value that changes digit count (e.g. cpu_usage 9% -> 10%)
    # doesn't change the cell's footprint and shift its neighbours. Only the
    # inline serializer (horizontal panel) honours it — mono_render already
    # pads every cell to a per-column width, so the vertical panel and tooltip
    # are stable without it. The deficit is padded on the side opposite the
    # alignment (right-aligned values get leading &nbsp;).
    min_width: int = 0
    # Override for the cell's monospace-column footprint, decoupling the LAYOUT
    # width from the on-screen character count. Needed when a cell is drawn at a
    # different font-size than the rest: a progress bar with a small
    # bar_panel.height paints N block chars but occupies far fewer full-size
    # columns in pixels, so the value rows aligning to it must reserve the
    # PIXEL-equivalent width, not N (see traces.bar_row / mono_render
    # _cell_width). None = use the real visible width.
    layout_width: Optional[int] = None


Row = list[Cell]


class Ident(NamedTuple):
    """A cell's identity on the two axes: the METRIC (key for glyph/label and
    the `item-` part of the class) and the FORM (the `form-` part; None = none,
    e.g. the net_speed_up/disk_io_read DUO parts, which have their own style).
    Cells write `css` as the final class — no flat name to rewrite afterwards.
    Built by `registry.render` from (metric, form)."""
    metric: str
    form: Optional[str] = None

    @property
    def css(self) -> str:
        return f"item-{self.metric}" + (f" form-{self.form}" if self.form else "")


_TAG_RE = _re.compile(r"<[^>]+>")


def visible_width(text: str) -> int:
    """On-screen monospace width of a cell's HTML text: strip tags (inner
    colour spans on bars/sparks/combos) and decode entities (&nbsp; -> one
    char). Every glyph — Nerd Font icon, braille, block element — is a single
    cell in the mono font, so a codepoint count is the visual width."""
    return len(_html.unescape(_TAG_RE.sub("", text)))


def _nbsp(n: int) -> str:
    """n non-breaking spaces (regular spaces collapse in the rich-text engine)."""
    return "&nbsp;" * n if n > 0 else ""


def cell_inner(cell: Cell) -> str:
    """A cell's on-screen content with its structural pad_left/pad_right
    materialized as &nbsp; around the text — but still INSIDE any CSS-class
    span the serializer wraps it in (the pad has no ink, so its color is
    irrelevant, and keeping it inside reproduces the old in-`text` spacing
    byte-for-byte)."""
    return _nbsp(cell.pad_left) + cell.text + _nbsp(cell.pad_right)


# ── Cell builders (role-tagged) ──────────────────────────────────────────────
# A value/aux cell carries its structural role ("val"/"aux") plus an optional
# per-value state class and an Ident's `item-<metrica> form-<forma>` hook (so any
# cell — not just labels — is individually targetable in CSS, e.g. ".tooltip
# .item-cpu_temp .val"). Kept here, free of any Config/formatter dependency, so
# the item registry can build cells without importing the formatter.

def _val_cell(text: str, cls: Optional[str] = None, ident: Optional[Ident] = None,
              min_width: int = 0) -> Cell:
    css = "val" + (f" {ident.css}" if ident else "") + (f" {cls}" if cls else "")
    return Cell(text=text, css_class=css, align="right", min_width=min_width)


def _aux_cell(text: str, cls: Optional[str] = None, ident: Optional[Ident] = None,
              pad_left: int = 0, pad_right: int = 0,
              layout_width: Optional[int] = None) -> Cell:
    """"aux"-role cell (spark/bar spans, a process name, a rate column).
    pad_left/pad_right are structural inter-cell gaps kept out of `text` (see Cell).
    layout_width overrides the column footprint when the cell is drawn at a
    different font-size than the value rows (small-font bar — see Cell)."""
    css = "aux" + (f" {ident.css}" if ident else "") + (f" {cls}" if cls else "")
    return Cell(text=text, css_class=css, pad_left=pad_left, pad_right=pad_right,
                layout_width=layout_width)


def _fmt_perc(pv: int, tooltip: bool) -> str:
    """Percent string: the compact panel drops the '%' and the cap at 100
    (so a full bar reads '100', not '100%'); the tooltip always keeps '%'."""
    if pv >= 100 and not tooltip:
        return str(pv)
    return f"{pv}%"


# Reserved width for percent value cells in the horizontal panel: 3 columns —
# the widest a panel % gets ("100", or "10%".."99%") — so a value going 9% ->
# 10% doesn't widen its item and shift its neighbours. Matches the natural
# width the vertical panel (mono_render) already gives the same column, so the
# two panels look the same. Only render_row_inline honours min_width; it's a
# floor, so the rare single-digit "9%" just sits right-aligned in the 3 cols.
PERCENT_PANEL_WIDTH = 3


@dataclass
class Separator:
    """An explicit TOML separator entry (separator_small / separator_big)
    passed to group_rows_into_blocks alongside Rows. `size` selects which CSS
    class — and therefore which font-size/thickness — the visible rule drawn
    before the next block gets."""
    size: str  # "small" | "big"


# Valid TOML separator item names → Separator size. Listing one in a section's
# `items` inserts an explicit gap (see formatter._build_entries); they aren't
# real items, so the registry guardrail (items.unknown_item_names) treats them
# as known rather than flagging them as typos.
SEPARATOR_ITEMS = {"separator_small": "small", "separator_big": "big"}


@dataclass
class Block:
    rows: list[Row]
    # Set if this block was opened by an explicit Separator entry, to the
    # separator's size — used by the renderer to draw the rule before the
    # block. Spacing is otherwise never automatic: a shape change alone
    # (different cell roles between consecutive items) closes the block but
    # leaves no visual gap unless the TOML places a separator there explicitly.
    separator_size: Optional[str] = None


# ── Threshold -> CSS class ──────────────────────────────────────────────────

def css_class_from_thresholds(v: float, thr: tuple[float, float]) -> str:
    """3-band: below mid -> good, mid..high -> warn, from high -> crit."""
    mid, high = thr
    if v >= high:
        return "crit"
    if v >= mid:
        return "warn"
    return "good"


def css_class_active(v: int, thr: int) -> Optional[str]:
    """Binary threshold: v > thr -> 'active', otherwise no class."""
    return "active" if v > thr else None


def css_class_battery(v: int, thr_low: int, thr_high: int) -> str:
    """Inverted 3-band: low charge is bad. thr_low/thr_high are the red/green cutoffs."""
    if v <= thr_low:
        return "crit"
    if v <= thr_high:
        return "warn"
    return "good"


# ── Row grouping ─────────────────────────────────────────────────────────────

def _cell_role(cell: Cell) -> str:
    """First word of css_class (e.g. 'val' out of 'val crit') — the
    structural role of the cell, ignoring per-value state classes."""
    return cell.css_class.split()[0] if cell.css_class else ""


def _row_shape(row: Row) -> tuple[str, ...]:
    """Structural signature of a row: the cell roles in order. Plain cell
    count isn't enough — e.g. net_speed (label,val,label,val) and cpu_usage
    with a bar+spark (label,val,extra,extra) both have 4 cells but are
    semantically different rows and must not share a block (and therefore a
    column layout). A single-cell spanning row (bar/spark/title) has its
    own one-element role tuple, so it stays isolated in its own block too."""
    return tuple(_cell_role(c) for c in row)


def group_rows_into_blocks(entries: list[Row | Separator]) -> list[Block]:
    """Group consecutive rows of the same shape (cell role pattern) into a
    Block. A Separator entry always closes the current block and tags the
    next one with its size. A shape change with no separator in between also
    closes the block, keeping every block column-consistent — this is how a
    spanning row (e.g. a spark, a different shape from the 2-cell rows
    around it) ends up isolated in its own single-row block — but without a
    Separator it gets no visible gap, just a new block."""
    blocks: list[Block] = []
    current: list[Row] = []
    current_shape: Optional[tuple[str, ...]] = None
    next_size: Optional[str] = None  # separator size for the block being assembled in `current`

    def flush(size_after: Optional[str]) -> None:
        nonlocal current, current_shape, next_size
        if current:
            blocks.append(Block(rows=current, separator_size=next_size))
        current = []
        current_shape = None
        next_size = size_after

    for entry in entries:
        if isinstance(entry, Separator):
            flush(size_after=entry.size)
            continue
        shape = _row_shape(entry)
        if current_shape is not None and shape != current_shape:
            flush(size_after=None)
        current.append(entry)
        current_shape = shape

    flush(size_after=None)
    return blocks


# ── Row builders ─────────────────────────────────────────────────────────────

def render_two_pair_row(label1: Cell, val1: Cell, label2: Cell, val2: Cell) -> Row:
    """Assemble two label/value pairs sharing one row (net_speed's Up/Down,
    disk_io's Read/Write, the combined Live/History row). Spacing between the
    pairs (e.g. val1 touching label2, like "9KDown:") is each caller's own
    responsibility — set a pad_right on val1 (or pad_left on label2) before
    calling this. mono_render lays the two pairs out across two halves of the
    row (see _is_two_pair there)."""
    return [label1, val1, label2, val2]


def render_three_col_row(label: Cell, extra: Cell, val: Cell) -> Row:
    """Assemble a 3-cell row (label | extra | value) — used by top_process
    (label/process-name/percentage), battery_sys (label/rate-or-limit/
    percentage), the composed net/wifi rows, etc.

    Leading gap on `extra` (only if non-empty): the label is left-aligned
    with nothing reserving space after it, so it touches `extra` otherwise
    (e.g. "Top 1:plasmashell", "Battery 0:max 80%")."""
    if extra.text:
        extra.pad_left = 1
    return [label, extra, val]


# ── HTML serialization ──────────────────────────────────────────────────────

def _separator_rule_html(size: str) -> str:
    """A real visible line for an explicit TOML separator: a plain <div>,
    not a <table> — Qt's RichText engine ignores `height`/`padding` CSS on
    table/td (confirmed by hand) but honors `font-size` on a div's text
    content, which is what .separator-rule-small/-big in style-dark.css
    actually use to control thickness. The HTML `width` attribute (not CSS)
    pins it to the popup's full width; without it the div had no width
    constraint at all, which made the whole tooltip auto-size wider than
    every other block."""
    return f'<div width="100%" class="separator-rule-{size}">&nbsp;</div>'


def render_row_inline(row: Row) -> str:
    """Single-line (horizontal panel) rendering: cells become consecutive
    <span>, no table — there's only one physical line, no column to align.
    Cells are joined by a single &nbsp; wrapped in a .gap span (role-tagged
    like the cells, so the inspection overlay can colour the inter-cell gap;
    colourless otherwise). Structural pad_left/pad_right are materialized
    inside each span via cell_inner."""
    parts = []
    for cell in row:
        attrs = f' class="{cell.css_class}"' if cell.css_class else ""
        inner = cell_inner(cell)
        # Reserve min_width so the cell's footprint stays fixed as its content's
        # digit count changes (e.g. 9% -> 10%), keeping neighbours from shifting.
        # The deficit pads inside the span (part of the cell, not a .gap) on the
        # side opposite the alignment.
        deficit = cell.min_width - (cell.pad_left + visible_width(cell.text) + cell.pad_right)
        if deficit > 0:
            inner = (_nbsp(deficit) + inner) if cell.align == "right" else (inner + _nbsp(deficit))
        parts.append(f"<span{attrs}>{inner}</span>")
    return '<span class="gap">&nbsp;</span>'.join(parts)
