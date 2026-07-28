#!/usr/bin/env python3
"""Render a QML or HTML fragment with Qt's rich-text engine and save a PNG.

Used to *see* the real render of the panel/tooltip — the same
`Text { textFormat: Text.RichText }` Plasma draws internally — without opening
a window: uses QQuickView.grabWindow(), which rasterizes offscreen
synchronously (the exact same engine as the widget, unlike QTextDocument,
which has a different CSS subset). Indispensable for pixel-level issues (where
a glyph's ink lands inside its box, font descents, alignment) that the
stripped HTML from `plasma-top render` can't show.

Usage:
    # HTML: wrapped in a RichText Text with the Nerd Font mono
    QT_QPA_PLATFORM=offscreen python3 tools/qt_shot.py --html panel.html out.png
    # ...or piping the HTML from stdin
    plasma-top render --format html && \
        QT_QPA_PLATFORM=offscreen python3 tools/qt_shot.py --html /tmp/plasma-top_render_panel.html out.png

    # QML: renders the file as-is (useful for test benches with several variants)
    QT_QPA_PLATFORM=offscreen python3 tools/qt_shot.py --qml bench.qml out.png

    # Tooltip: --fit sizes the window to the content (as wide as in Plasma)
    plasma-top render --component tooltip --format html && \
        QT_QPA_PLATFORM=offscreen python3 tools/qt_shot.py \
            --html /tmp/plasma-top_render_tooltip.html out.png --fit --scale 2

Options: --width/--height (viewport, REAL size), --bg (#rrggbb, panel-like
background), --font (family), --size (text size, --html), --point (interpret
--size as pointSize, like the real plasmoid, instead of pixelSize), --pad
(text inset), --lineheight, --scale (magnifier: renders at real size then
upscales the PNG xN nearest-neighbor — faithful, doesn't stretch the layout),
--fit (sizes the window to the content's NATURAL width/height instead of the
viewport — essential for the tooltip, which in Plasma is as wide as its
widest row, not as wide as --width).

For a faithful preview of the vertical panel (as wide as the real strip, e.g.
50px, plasmoid font in points): --width 50 --height 64 --point --size 8 --scale 8.
For the tooltip instead: --fit --scale 2 (no --width, it's computed from the content).

Requires PyQt6 (or adapt the imports to PySide6). QT_QPA_PLATFORM=offscreen
avoids opening windows; grabWindow() still works.
"""
from __future__ import annotations

import argparse
import re
import sys
import tempfile
from pathlib import Path

from PyQt6.QtCore import Qt, QObject, QUrl
from PyQt6.QtGui import QColor
from PyQt6.QtQuick import QQuickView
from PyQt6.QtWidgets import QApplication

_ANSI_COLORS = {
    30: "#000000", 31: "#aa0000", 32: "#00aa00", 33: "#aa6500",
    34: "#0000aa", 35: "#aa00aa", 36: "#00aaaa", 37: "#aaaaaa",
    90: "#656565", 91: "#ff6565", 92: "#65ff65", 93: "#ffff65",
    94: "#6565ff", 95: "#ff65ff", 96: "#65ffff", 97: "#ffffff",
}
_SGR_RE = re.compile(r"\x1b\[(\d+(?:;\d+)*)?m")


def _plasmoid_output(text: str) -> str:
    """Apply main.qml's ANSI/newline conversion before RichText rendering."""
    if text.endswith("\n"):
        text = text[:-1]
    close_tags: list[str] = []
    bold = False

    def reset() -> str:
        nonlocal bold
        result = " ".join(close_tags)
        close_tags.clear()
        bold = False
        return result

    def color(tokens: list[int], index: int) -> str | None:
        if index + 1 >= len(tokens):
            return None
        mode = tokens[index + 1]
        if mode == 2 and index + 4 < len(tokens):
            r, g, b = (max(0, min(value, 255)) for value in tokens[index + 2:index + 5])
            return f"#{r:02x}{g:02x}{b:02x}"
        if mode != 5 or index + 2 >= len(tokens):
            return None
        value = tokens[index + 2]
        if 0 <= value <= 7:
            return _ANSI_COLORS[value + 30]
        if 8 <= value <= 15:
            return _ANSI_COLORS[value - 8 + 90]
        if 16 <= value <= 231:
            value -= 16
            # Keep main.qml's JavaScript division/modulo/Math.floor order.
            parts = (value / 36 % 6, value / 6 % 6, value % 6)
            levels = tuple(0 if part == 0 else int(40 * part + 55) for part in parts)
            return f"#{levels[0]:02x}{levels[1]:02x}{levels[2]:02x}"
        if 232 <= value <= 255:
            gray = (value - 232) * 10 + 8
            return f"#{gray:02x}{gray:02x}{gray:02x}"
        return None

    def replace(match: re.Match[str]) -> str:
        nonlocal bold
        tokens = [int(token) for token in (match.group(1) or "0").split(";")]
        output: list[str] = []
        for index, token in enumerate(tokens):
            if token == 0:
                output.append(reset())
            elif token == 1:
                close_tags.append("</b>")
                bold = True
                output.append("<b>")
            elif token in (38, 48):
                ansi_color = color(tokens, index)
                if token == 38 and ansi_color:
                    close_tags.append("</font>")
                    output.append(f'<font color="{ansi_color}">')
            elif token in _ANSI_COLORS:
                if bold and 30 <= token <= 37:
                    token += 60
                close_tags.append("</font>")
                output.append(f'<font color="{_ANSI_COLORS[token]}">')
        return "".join(output)

    converted = _SGR_RE.sub(replace, text) + reset()
    return converted.replace("\n", "<br>")

# Faithful to the applet's Text (package/contents/ui/main.qml):
# same wrapMode, lineHeight and — important — font.pointSize (not pixelSize) in
# the panel, so the glyph advance scales with DPI like the real thing. `pad` is
# the inset around the text (default 0: a narrow panel shouldn't be skewed by a
# fake margin).
#
# {width_line}/{wrapmode} change between the two modes: by default the Text
# fills the viewport (constrained width, Text.Wrap) — right for the panel,
# as wide as the real strip. With --fit instead there's no width constraint and
# Text.NoWrap, so the Text takes its NATURAL width from the content
# (implicitWidth/contentWidth) — like the tooltip popup in Plasma, which sizes
# itself to the widest row, not to a fixed viewport. The view is then shrunk to
# that measurement (see main).
_HTML_WRAPPER = """import QtQuick
Rectangle {{
    color: "{bg}"
    Text {{
        objectName: "txt"
        x: {pad}; y: {pad}
        {width_line}
        wrapMode: {wrapmode}
        textFormat: Text.RichText
        color: "white"
        font.family: "{font}"
        font.{sizemode}: {size}
        lineHeight: {lineheight}
        lineHeightMode: Text.ProportionalHeight
        text: {text}
    }}
}}
"""


def _qml_for_html(html_text: str, bg: str, font: str, size: int,
                  point: bool, pad: int, lineheight: float, fit: bool) -> str:
    # QML string literal: pass the HTML as a triple-backtick string, quotes
    # already neutral (the formatter's HTML uses double quotes in attributes,
    # which are fine as-is inside a triple-backtick QML string).
    literal = '`' + html_text.replace('`', '\\`') + '`'
    # --fit: no width constraint + NoWrap → the Text sizes itself to the content.
    width_line = "" if fit else f"width: parent.width - {pad * 2}"
    wrapmode = "Text.NoWrap" if fit else "Text.Wrap"
    return _HTML_WRAPPER.format(
        bg=bg, font=font, size=size, text=literal, pad=pad, width_line=width_line,
        sizemode="pointSize" if point else "pixelSize", lineheight=lineheight,
        wrapmode=wrapmode)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    src = ap.add_mutually_exclusive_group(required=True)
    src.add_argument("--qml", type=Path, help="QML file to render as-is")
    src.add_argument("--html", type=Path, help="HTML file (wrapped in a RichText Text)")
    ap.add_argument("out", type=Path, help="output PNG")
    ap.add_argument("--width", type=int, default=600)
    ap.add_argument("--height", type=int, default=200)
    ap.add_argument("--bg", default="#3b4a5a", help="background color (default: panel-like grey-blue)")
    ap.add_argument("--font", default="NotoSansM Nerd Font Mono")
    ap.add_argument("--size", type=int, default=18, help="Text size (--html only): pixelSize, or pointSize with --point")
    ap.add_argument("--point", action="store_true", help="interpret --size as pointSize (like the real plasmoid), not pixelSize")
    ap.add_argument("--pad", type=int, default=0, help="text inset in px (--html only; 0 = no fake margin)")
    ap.add_argument("--lineheight", type=float, default=1.0, help="Text's proportional lineHeight (--html only)")
    ap.add_argument("--scale", type=int, default=1,
                    help="magnifier: renders at real size, then upscales the PNG xN nearest-neighbor (faithful, doesn't stretch the layout)")
    ap.add_argument("--fit", action="store_true",
                    help="sizes the window to the content's NATURAL width/height "
                         "instead of the viewport (--html only): makes the tooltip as wide as "
                         "in Plasma, not as wide as --width")
    ap.add_argument("--plasmoid-output", action="store_true",
                    help="apply main.qml ANSI/newline output conversion before rendering")
    args = ap.parse_args()

    app = QApplication(sys.argv)
    view = QQuickView()
    view.setColor(QColor(args.bg))
    view.setResizeMode(QQuickView.ResizeMode.SizeRootObjectToView)
    # REAL size (not × scale): layout must happen at the true geometry, the
    # magnification is only a lens applied afterwards on the PNG (see --scale).
    view.resize(args.width, args.height)

    tmp: Path | None = None
    if args.qml:
        qml_path = args.qml
    else:
        html_text = args.html.read_text(encoding="utf-8")
        if args.plasmoid_output:
            html_text = _plasmoid_output(html_text)
        qml = _qml_for_html(html_text, args.bg, args.font,
                            args.size, args.point, args.pad, args.lineheight, args.fit)
        tmp = Path(tempfile.mkstemp(suffix=".qml")[1])
        tmp.write_text(qml, encoding="utf-8")
        qml_path = tmp

    view.setSource(QUrl.fromLocalFile(str(qml_path)))
    if view.status() == QQuickView.Status.Error:
        for e in view.errors():
            print(e.toString(), file=sys.stderr)
        return 1
    view.show()
    app.processEvents()
    # --fit: the Text has sized itself to the content (NoWrap, no width
    # constraint); read its contentWidth/Height and shrink the view to that
    # (+ double pad), then redo the layout, so the PNG is as wide as the real
    # popup and not the starting viewport. Outside of --fit the fixed
    # --width/--height geometry stands.
    if args.fit and (txt := view.rootObject().findChild(QObject, "txt")) is not None:
        import math
        w = math.ceil(txt.property("contentWidth")) + 2 * args.pad
        h = math.ceil(txt.property("contentHeight")) + 2 * args.pad
        view.resize(w, h)
        app.processEvents()
    img = view.grabWindow()
    if args.scale > 1:
        # Nearest-neighbor magnifier: every real px becomes an N×N block, so
        # the PNG is the authentic render just enlarged (faithful), not a
        # rescaled layout.
        img = img.scaled(img.width() * args.scale, img.height() * args.scale,
                         Qt.AspectRatioMode.IgnoreAspectRatio,
                         Qt.TransformationMode.FastTransformation)
    img.save(str(args.out))
    print(f"written {args.out}  ({img.width()}x{img.height()})")
    if tmp:
        tmp.unlink(missing_ok=True)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
