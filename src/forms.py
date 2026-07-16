"""The CLOSED set of row forms — the "how" axis of an item, separate from the
"what" axis (the metrics, in metrics.py).

    Form   : the user's choice in the toml (after the colon, `cpu_usage:spark`).
             Each form declares which surfaces it makes sense on
             (FORM_SURFACES) — placement is DERIVED from that, not declared.

    Shape  : a row skeleton, only for the metrics that have their OWN
             (metrics.intrinsic_shape: net_speed/disk_io's DUO,
             top_process's TRIPLE_L). Generic forms don't carry one: how a row
             is actually laid out is read off its cells by mono_render._plan_row
             (see docs/LAYOUT.md), so a Form→Shape table here would be a second
             source of truth free to drift from the serializer.

The INTRINSIC trims of a single metric (cpu_freq's turbo, disk_usage's GB,
battery's rate) are NOT forms in this menu: they're that metric's own
rendering and live in metrics.py. Only the generic forms — the ones
applicable to several metrics — live here.
"""
from __future__ import annotations

from enum import Enum, Flag, auto


class Shape(Enum):
    """The skeleton of a metric with its own intrinsic rendering — the value of
    metrics.Metric.intrinsic_shape, which marks a metric as taking no form from
    the menu. Only the two skeletons such a metric actually uses are listed:
    the generic forms don't name a Shape (see the module docstring)."""
    TRIPLE_L = "triple_l"  # label · left-aligned aux · value (top_process)
    DUO      = "duo"       # two pairs on the same row (net_speed, disk_io)


class Surface(Flag):
    """The THREE real render contexts (not two): the tooltip and the panel in
    its two orientations. Keeping them distinct is what makes the net_speed
    case explicit (a pair in the tooltip and the horizontal panel, two rows in
    the vertical one)."""
    TOOLTIP = auto()
    PANEL_H = auto()   # horizontal panel: inline row, grows sideways
    PANEL_V = auto()   # vertical panel: monospace column, narrow

    PANEL = PANEL_H | PANEL_V
    ALL   = TOOLTIP | PANEL_H | PANEL_V


class Form(Enum):
    """The closed menu of generic forms, written after the colon in the toml."""
    VALUE         = "value"          # the number — the default (no suffix)
    BAR           = "bar"            # full bar/column (adapts to orientation)
    SPARK         = "spark"          # block spark
    BRAILLE       = "braille"        # braille spark
    SPARK_VALUE   = "spark_value"    # number + spark alongside
    BRAILLE_VALUE = "braille_value"  # number + braille alongside
    BAR_SPARK     = "bar_spark"      # "now" bar + "history" spark
    BAR_BRAILLE   = "bar_braille"    # "now" bar + "history" braille
    PAIR          = "pair"           # multiple instances, two-per-row


# ── form → admitted surfaces ──────────────────────────────────────────────────
# The surfaces from which placement is DERIVED: no item declares where it lives.
# config._drop_misplaced_items enforces this, so a form only ever renders where
# it's admitted. The logic behind the assignments:
#   - the full visuals (bar/spark/braille) replace the number and span: they
#     belong in the panel, not the tooltip (they carry no label at all).
#   - the "number + history" and "bar + history" combos are wide: tooltip only.
#   - VALUE goes anywhere; in the vertical panel a multi-value variant of it can
#     unpack (net_speed's adaptivity, handled separately as an exception).

FORM_SURFACES: dict[Form, Surface] = {
    Form.VALUE:         Surface.ALL,
    Form.BAR:           Surface.PANEL,
    Form.SPARK:         Surface.PANEL,
    Form.BRAILLE:       Surface.PANEL,
    Form.SPARK_VALUE:   Surface.TOOLTIP,
    Form.BRAILLE_VALUE: Surface.TOOLTIP,
    Form.BAR_SPARK:     Surface.TOOLTIP,
    Form.BAR_BRAILLE:   Surface.TOOLTIP,
    Form.PAIR:          Surface.TOOLTIP,
}


def form_from_token(token: str) -> Form:
    """Resolve the piece after the colon (`cpu_usage:spark_value` -> SPARK_VALUE).
    Empty/absent = VALUE. Raises ValueError on an unknown token, so a typo in
    the toml shows up right away instead of vanishing silently."""
    if not token:
        return Form.VALUE
    try:
        return Form(token)
    except ValueError:
        raise ValueError(f"unknown form: {token!r}") from None
