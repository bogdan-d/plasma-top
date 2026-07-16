#!/usr/bin/env python3
"""Manual visual test: writes a static, fixed-content tooltip to
/tmp/pirostats_tooltip.html so a CSS property can be tweaked and re-checked in
the real plasmoid tooltip without live data (percentages, bars) changing
between two looks and confusing the comparison.

Covers every structural row shape the real tooltip can produce (see
render_model.group_rows_into_blocks / formatter.py callers):
  - plain (label, val) row
  - a spanning bar/spark row (single cell) above/below a plain row
  - a 4-cell row (net_speed-style: label, val, label, val)
  - a 3-cell row (top_process-style: label, extra, val)
  - a section title row (title_N: full-width text, own "title" CSS role)
  - two consecutive blocks separated by a rule

Usage:
  systemctl --user stop pirostats   # daemon would overwrite the file otherwise
  python3 tools/manual_tooltip_preview.py
  # hover the panel icon to inspect, edit style/style-dark.css, re-run
  systemctl --user start pirostats  # when done
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from daemon import _read_css
from render_model import Cell, Separator, group_rows_into_blocks
from mono_render import render_blocks_monospace
from runtime import TOOLTIP_FILE, ensure_dirs

CSS_PATH = Path(__file__).resolve().parent.parent / "style/style-dark.css"


def _val(text: str, cls: str | None = None) -> Cell:
    css = "val" + (f" {cls}" if cls else "")
    return Cell(text=text, css_class=css, align="right")


def build_entries():
    entries = []

    # Spanning spark row, then a plain row right below it (no gap wanted).
    entries.append([Cell(text="▁▂▃▄▅▆▇█", css_class="aux")])
    entries.append([Cell(text=" Cpu usage:", css_class="label"), _val("42%", "warn")])
    entries.append([Cell(text="████░░░░", css_class="aux")])

    entries.append(Separator(size="big"))  # separator -> new block + visible rule

    # net_speed-style 4-cell row
    entries.append([
        Cell(text=" Up:", css_class="label"), _val("123K"),
        Cell(text=" Down:", css_class="label"), _val("4M"),
    ])

    # top_process-style 3-cell rows, same block (different shape from the
    # 4-cell row above, so group_rows_into_blocks isolates it on its own)
    entries.append([Cell(text=" Top 1:", css_class="label"), Cell(text="firefox", css_class="aux"), _val("87%", "crit")])
    entries.append([Cell(text=" Top 2:", css_class="label"), Cell(text="code", css_class="aux"), _val("12%")])
    entries.append([Cell(text=" Top 3:", css_class="label"), Cell(text="claude", css_class="aux"), _val("5%")])

    entries.append(Separator(size="small"))  # thin rule between blocks (separator_small_N)

    # Section title row (title_N in [tooltip]): own "title" CSS role, so it
    # always lands in its own block, no separator/rule needed around it.
    entries.append([Cell(text="Battery", css_class="title")])

    # plain multi-row block (battery-like)
    entries.append([Cell(text=" Battery:", css_class="label"), _val("80%", "good")])
    entries.append([Cell(text=" Brightness:", css_class="label"), _val("50%")])

    return entries


def main():
    css = _read_css(CSS_PATH)
    blocks = group_rows_into_blocks(build_entries())
    body = render_blocks_monospace(blocks)
    html = f"<style>{css}</style><div class=\"tooltip\">{body}</div>"
    ensure_dirs()
    TOOLTIP_FILE.write_text(html, encoding="utf-8")
    print(f"written: {TOOLTIP_FILE}")


if __name__ == "__main__":
    main()
