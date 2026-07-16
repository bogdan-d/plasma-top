#!/usr/bin/env python3
"""Render a QML or HTML fragment with Qt's rich-text engine and save a PNG.

Used to *see* the real render of the panel/tooltip — the same
`Text { textFormat: Text.RichText }` Plasma draws internally — without opening
a window: uses QQuickView.grabWindow(), which rasterizes offscreen
synchronously (the exact same engine as the widget, unlike QTextDocument,
which has a different CSS subset). Indispensable for pixel-level issues (where
a glyph's ink lands inside its box, font descents, alignment) that the
stripped HTML from `pirostats render` can't show.

Usage:
    # HTML: wrapped in a RichText Text with the Nerd Font mono
    QT_QPA_PLATFORM=offscreen python3 tools/qt_shot.py --html panel.html out.png
    # ...or piping the HTML from stdin
    pirostats render --format html && \
        QT_QPA_PLATFORM=offscreen python3 tools/qt_shot.py --html /tmp/pirostats_render_panel.html out.png

    # QML: renders the file as-is (useful for test benches with several variants)
    QT_QPA_PLATFORM=offscreen python3 tools/qt_shot.py --qml bench.qml out.png

    # Tooltip: --fit sizes the window to the content (as wide as in Plasma)
    pirostats render --component tooltip --format html && \
        QT_QPA_PLATFORM=offscreen python3 tools/qt_shot.py \
            --html /tmp/pirostats_render_tooltip.html out.png --fit --scale 2

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
import sys
import tempfile
from pathlib import Path

from PyQt6.QtCore import Qt, QObject, QUrl
from PyQt6.QtGui import QColor
from PyQt6.QtQuick import QQuickView
from PyQt6.QtWidgets import QApplication

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
        qml = _qml_for_html(args.html.read_text(encoding="utf-8"), args.bg, args.font,
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
