"""Trace forms: the bar / column / spark / braille encodings of a percentage.

These are the cpu_usage & mem_usage "own-skeleton" forms (see registry.py):
unlike the declarative row/per items, each composes its own cell(s), so instead
of living in the cell-factory library they form this small family. A trace turns
one number (the live value → bar/column) or a history buffer (spark/braille)
into inline HTML; the row builders wrap that HTML into aux/label cells.

Two axes organise the module:

  • WHAT is encoded — a live value (bar, column) or a history buffer
    (spark, braille);
  • HOW it's laid out — standalone (a lone full-width row, independently
    orderable via its section) or combined ("Live: <bar>  History: <spark>",
    the two `bar_*` combos, tooltip-only).

Colour is always a CSS class name (bar-<band>, grad-<prefix>-<n>); this module
never decides what a colour looks like — that lives in style-*.css. Every
function takes the PanelFormatter `f` for config/helper access, matching the
`(f, …)` shape the registry uses for its renderers.
"""
from __future__ import annotations

from typing import Optional

from config import BRAILLE_LENGTH_MULTIPLIER
from render_model import (
    Ident,
    Row,
    _aux_cell,
    css_class_from_thresholds,
    render_two_pair_row,
)


# 1-sample/char blocks, 8 height levels — the bar fill, the column glyph and the
# block spark all index into this ramp by value.
BLOCK_RAMP = "▁▂▃▄▅▆▇█"

# The bar's two glyphs: full block filled, light shade empty. The bar's SHAPE is
# fixed here rather than in config.toml — like BLOCK_RAMP above, it's what the
# form IS, not a per-surface setting. Only the bar's size (width/height) is
# configurable; its colour is CSS (bar-<band>/bar-empty).
BAR_FILL_CHAR  = "█"
BAR_EMPTY_CHAR = "░"

# Braille spark: 2 samples/char (left/right dot-column), 4 height levels each —
# a denser resolution than BLOCK_RAMP's 1-sample/8-level blocks. Bit order is
# bottom-to-top per column (cumulative fill, like a bar): left=dots 7,3,2,1;
# right=dots 8,6,5,4 (standard Braille Patterns dot numbering, U+2800 base).
_BRAILLE_LEFT_BITS  = [0b0100_0000, 0b0000_0100, 0b0000_0010, 0b0000_0001]
_BRAILLE_RIGHT_BITS = [0b1000_0000, 0b0010_0000, 0b0001_0000, 0b0000_1000]
_BRAILLE_GRADES = 8


def _surface_cfg(f, base: str, tooltip: bool):
    """The tooltip vs panel variant of a per-surface visual config: `_surface_cfg
    (f, "bar", True)` → cfg.bar_tooltip, `(f, "spark", False)` → cfg.spark_panel."""
    return getattr(f._cfg, f"{base}_{'tooltip' if tooltip else 'panel'}")


# ── value encodings: bar (horizontal), column (vertical glyph) ────────────────

def bar_html(f, v: Optional[int], thr: tuple[int, int], tooltip: bool) -> str:
    """Horizontal bar: `width` fill chars proportional to v, the rest empty
    chars, coloured by threshold band. height is a font-size in px on both spans
    (Qt RichText has no usable CSS height) so filled/empty stay aligned and a
    small height also narrows the bar in pixels — which keeps a wide bar from
    driving the vertical panel's width; 0 → inherit, no inline style. "" when
    there's no value or the bar is disabled (width ≤ 0)."""
    if v is None:
        return ""
    cfg = _surface_cfg(f, "bar", tooltip)
    if cfg.width <= 0:
        return ""
    filled = min(v * cfg.width // 100, cfg.width)
    empty  = cfg.width - filled
    cls    = css_class_from_thresholds(v, thr)
    sty    = f' style="font-size:{cfg.height}px"' if cfg.height > 0 else ""
    return (f'<span class="bar-{cls}"{sty}>{BAR_FILL_CHAR * filled}</span>'
            f'<span class="bar-empty"{sty}>{BAR_EMPTY_CHAR * empty}</span>')


def column_html(f, v: Optional[int], thr: tuple[int, int]) -> str:
    """Vertical column (horizontal panel only): a single eighth-block glyph
    (▁..█) picked by value and repeated `width` times for thickness, coloured by
    threshold band — where the bar grows sideways, this grows upward. height is a
    font-size in px (how tall it stands); 0 → inherit. "" when there's no value.
    Palette and grey track stay in style-*.css (.item-<metric>.form-column)."""
    if v is None:
        return ""
    idx = min(v * 8 // 100, 7)
    cls = css_class_from_thresholds(v, thr)
    cfg = f._cfg.column_panel
    w   = max(1, cfg.width)
    sty = f' style="font-size:{cfg.height}px"' if cfg.height > 0 else ""
    return f'<span class="bar-{cls}"{sty}>{BLOCK_RAMP[idx] * w}</span>'


# ── history encodings: spark (blocks), braille ───────────────────────────────

def spark_html(f, history: Optional[list[int]], hist_name: str, tooltip: bool) -> str:
    """Block spark: one BLOCK_RAMP char per sample of the recent history, each
    coloured by its own value's band (thresholds.<hist_name>, e.g. "cpu_spark").
    The full width is reserved from the first render — before `length` samples
    have accumulated the missing head is padded with a flat spark-empty run, so
    the spark (and the tooltip) doesn't widen as samples arrive. "" when there's
    no history buffer yet."""
    if history is None:
        return ""
    cfg    = _surface_cfg(f, "spark", tooltip)
    thr    = getattr(f._cfg.thresholds, hist_name)
    length = cfg.cpu_spark_length if hist_name == "cpu_spark" else cfg.mem_spark_length
    recent = history[-length:]
    missing = length - len(recent)
    out = f'<span class="spark-empty">{BLOCK_RAMP[0] * missing}</span>' if missing else ""
    for v in recent:
        idx = min(v * 8 // 100, 7)
        cls = css_class_from_thresholds(v, thr)
        out += f'<span class="bar-{cls}">{BLOCK_RAMP[idx]}</span>'
    return out


def _braille_level(v: int) -> int:
    """1..4 filled dots in a column. Ceiling, not floor: floor maps the whole
    1-24% range to 0 dots (invisible at any normal idle load) — ceiling instead
    guarantees any sample shows at least one dot, so a real 0% reading stays on
    the spark's baseline instead of a gap (only a None column, i.e. not-yet
    collected history, is empty — that distinction is handled by the caller)."""
    if v <= 0:
        return 1
    return min(4, -(-v * 4 // 100))  # ceil division


def _braille_char(v_left: Optional[int], v_right: Optional[int]) -> str:
    """One braille glyph carrying two samples: the left/right dot-columns filled
    bottom-to-top to each sample's level. A None column stays empty."""
    code = 0x2800
    if v_left is not None:
        for i in range(_braille_level(v_left)):
            code |= _BRAILLE_LEFT_BITS[i]
    if v_right is not None:
        for i in range(_braille_level(v_right)):
            code |= _BRAILLE_RIGHT_BITS[i]
    return chr(code)


def braille_html(f, history: Optional[list[int]], prefix: str, tooltip: bool,
                 chars: Optional[int] = None) -> str:
    """Braille spark: 2 samples/char (so it packs twice the history of the block
    spark at the same char width — sensors.py sizes the buffer via
    BRAILLE_LENGTH_MULTIPLIER). Coloured by an 8-grade continuous gradient
    (grad-<prefix>-N), not the 3-band thresholds; each glyph takes its more
    critical sample's grade, since one char can't show two colours. Not-yet
    collected head samples are None — a genuinely empty cell (⠀), no dots, no
    colour. "" when there's no history buffer yet. `chars` overrides the config
    length (in characters) — the cpu_cores page stretches the spark to fill the
    page width; the per-core buffer is sized to match (see sensors)."""
    if history is None:
        return ""
    cfg      = _surface_cfg(f, "braille", tooltip)
    base_len = chars if chars is not None else (cfg.cpu_braille_length if prefix == "cpu"
                                                else cfg.mem_braille_length)
    length   = base_len * BRAILLE_LENGTH_MULTIPLIER
    recent = history[-length:]
    padded: list[Optional[int]] = [None] * (length - len(recent)) + recent
    if len(padded) % 2:
        padded = [None] + padded
    out = ""
    for i in range(0, len(padded), 2):
        vl, vr = padded[i], padded[i + 1]
        if vl is None and vr is None:
            out += "⠀"
            continue
        grade = max(min(v * _BRAILLE_GRADES // 100, _BRAILLE_GRADES - 1)
                    for v in (vl, vr) if v is not None)
        out += f'<span class="grad-{prefix}-{grade}">{_braille_char(vl, vr)}</span>'
    return out


# ── row builders ──────────────────────────────────────────────────────────────

def _bar_layout_width(f, tooltip: bool) -> Optional[int]:
    """Column footprint the bar reserves for the value rows' right-edge
    alignment. With a small bar height the bar paints `width` block chars but is
    only `width * height / panel_font_size` full-size columns wide in pixels — so
    the value rows must align to THAT, not to `width`, or they'd be over-padded
    and wrap in a narrow vertical panel. None (no height) → use the real width."""
    cfg = _surface_cfg(f, "bar", tooltip)
    if cfg.height <= 0 or cfg.width <= 0:
        return None
    return max(1, round(cfg.width * cfg.height / f._cfg.display.panel_font_size))


def _standalone(html: str, ident: Ident, layout_width: Optional[int] = None) -> list[Row]:
    """A trace on its own row: one full-width aux cell, no label, independently
    orderable via its section's item list. Empty HTML (no data / disabled) →
    no row, so the item self-collapses."""
    if not html:
        return []
    return [[_aux_cell(html, ident=ident, layout_width=layout_width)]]


def bar_row(f, v: Optional[int], thr: tuple[int, int], tooltip: bool, ident: Ident) -> list[Row]:
    return _standalone(bar_html(f, v, thr, tooltip), ident,
                       layout_width=_bar_layout_width(f, tooltip))


def column_row(f, v: Optional[int], thr: tuple[int, int], ident: Ident) -> list[Row]:
    return _standalone(column_html(f, v, thr), ident)


def spark_row(f, history: Optional[list[int]], hist_name: str, tooltip: bool, ident: Ident) -> list[Row]:
    return _standalone(spark_html(f, history, hist_name, tooltip), ident)


def braille_row(f, history: Optional[list[int]], prefix: str, tooltip: bool, ident: Ident) -> list[Row]:
    return _standalone(braille_html(f, history, prefix, tooltip), ident)


def _bar_history_row(f, prefix: str, bar: str, hist: str, hist_form: str, tooltip: bool) -> list[Row]:
    """The shared skeleton of both combos: a side-by-side "Live: <bar>  History:
    <trace>" row (render_two_pair_row), tooltip-only. Both halves must have data
    or the whole row is dropped. The bar half is fixed; only the history half's
    renderer and its cell form (spark vs braille) differ between the two combos.

    The &nbsp; padding (1 leading / 2 trailing on the bar, 2 leading on the
    trace) is tuned by hand: the custom metric labels leave little breathing
    room, and going lower risks shrinking the popup's auto-sized width enough to
    clip content elsewhere (the popup width is pinned by the widest natural row
    across the whole tooltip — see the pirostats sizing note). The history
    label keeps the `spark` form in both variants: only the value glyph/colour
    changes, its label styling doesn't."""
    if not (bar and hist):
        return []
    usage = f"{prefix}_usage"
    live_label = f._label_cell(Ident(usage, "bar"), tooltip, text=f._cfg.labels.get(usage, ""))
    hist_label = f._label_cell(Ident(usage, "spark"), tooltip, text=f._cfg.labels.get("history", ""))
    return [render_two_pair_row(
        live_label, _aux_cell(bar, ident=Ident(usage, "bar"), pad_left=1, pad_right=2),
        hist_label, _aux_cell(hist, ident=Ident(usage, hist_form), pad_left=2),
    )]


def bar_spark_row(f, prefix: str, v: Optional[int], thr: tuple[int, int],
                  history: Optional[list[int]], hist_name: str, tooltip: bool) -> list[Row]:
    return _bar_history_row(
        f, prefix, bar_html(f, v, thr, tooltip),
        spark_html(f, history, hist_name, tooltip), "spark", tooltip)


def bar_braille_row(f, prefix: str, v: Optional[int], thr: tuple[int, int],
                    history: Optional[list[int]], tooltip: bool) -> list[Row]:
    return _bar_history_row(
        f, prefix, bar_html(f, v, thr, tooltip),
        braille_html(f, history, prefix, tooltip), "braille", tooltip)
