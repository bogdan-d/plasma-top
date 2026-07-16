"""Pure-stdlib PNG charts for the tooltip (no Pillow/matplotlib).

The tooltip can show a raster image via a data: URI (Qt RichText won't take
SVG — it crashes plasmashell). This module rasterizes a small area chart of a
history series (cpu/mem/…) and encodes it as a PNG, entirely with zlib+struct.

Only the plot geometry lives in the image — grid, filled area, top line. All
text (title, legend, the current value) is normal tooltip HTML around it, like
plasma-systemmonitor's graph box + text legend. So no font rasterization here.

Colors are baked into the pixels (a rasterized image can't read the CSS the way
the text cells do), so this file carries a small theme-agnostic palette: a
transparent background and hues that read on both light and dark tooltips.

Rendered directly at target resolution (no supersampling): grid and area are
axis-aligned and need no antialiasing, so only the top line is antialiased, by
vertical coverage — which keeps a redraw well under the poll budget.
"""
from __future__ import annotations

import struct
import zlib

RGBA = tuple[int, int, int, int]

# Theme-agnostic palette (baked into the PNG, unlike the CSS-styled text). The
# hues mirror the spark gradients: cpu blue, mem purple.
GRID: RGBA = (128, 128, 128, 70)        # faint, reads on light and dark
LABEL: RGBA = (140, 140, 140, 210)      # y-axis digits, muted but legible
BLUE_LINE: RGBA = (61, 174, 233, 255)   # plasma blue (cpu)
BLUE_FILL: RGBA = (61, 174, 233, 70)
PURPLE_LINE: RGBA = (163, 102, 255, 255)   # mem
PURPLE_FILL: RGBA = (163, 102, 255, 70)
GREEN_LINE: RGBA = (46, 204, 113, 255)     # gpu usage (area)
GREEN_FILL: RGBA = (46, 204, 113, 70)
ORANGE_LINE: RGBA = (230, 126, 34, 255)    # gpu decoder (line overlay)
TEAL_LINE: RGBA = (26, 188, 156, 255)      # net download (area)
TEAL_FILL: RGBA = (26, 188, 156, 70)
RED_LINE: RGBA = (231, 76, 60, 255)        # net upload (line overlay)

# 3×5 pixel font for the y-axis digits (baked into the PNG — no CSS/font here).
_DIGITS = {
    "0": ("111", "101", "101", "101", "111"),
    "1": ("110", "010", "010", "010", "111"),
    "2": ("111", "001", "111", "100", "111"),
    "3": ("111", "001", "111", "001", "111"),
    "4": ("101", "101", "111", "001", "001"),
    "5": ("111", "100", "111", "001", "111"),
    "6": ("111", "100", "111", "101", "111"),
    "7": ("111", "001", "001", "010", "010"),
    "8": ("111", "101", "111", "101", "111"),
    "9": ("111", "101", "111", "001", "111"),
}
_DIGIT_W, _DIGIT_H = 3, 5


def _encode_png(width: int, height: int, pixels: bytearray) -> bytes:
    """RGBA pixel buffer (row-major, top-to-bottom, 4 bytes/px) → PNG bytes.
    Color type 6 (RGBA), 8-bit, filter 0 on every scanline."""
    stride = width * 4
    raw = bytearray()
    for y in range(height):
        raw.append(0)                       # filter type 0 (None)
        raw += pixels[y * stride:(y + 1) * stride]

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xffffffff))

    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
            + chunk(b"IEND", b""))


def area_chart_png(series: list[int | float], width: int, height: int,
                   *, vmax: float = 100.0,
                   line: RGBA = BLUE_LINE, fill: RGBA = BLUE_FILL,
                   grid: RGBA = GRID, label: RGBA = LABEL, left_pad: int = 0,
                   grid_levels: tuple[float, ...] = (0, 25, 50, 75, 100),
                   overlay: list | None = None, overlay_line: RGBA = GRID,
                   label_values: bool = True) -> bytes:
    """Area chart of `series` (values in 0..vmax) at `width`×`height` px: faint
    horizontal grid, a filled area and its antialiased top line. `left_pad` > 0
    reserves that many px on the left for right-aligned y-axis digit labels (one
    per grid level); `label_values=False` keeps the margin (so stacked charts
    line up) but draws no digits, for a non-percent scale like network rates.
    `overlay` draws a second series as a line only (in `overlay_line`), e.g. gpu
    decoder over gpu usage. Transparent background."""
    buf = bytearray(width * height * 4)              # transparent (all zero)
    top_pad = 2                                      # keep vmax off the very top
    floor_y = height - 1                             # 0% sits on the bottom row
    span = height - 1 - top_pad
    plot_x0 = left_pad                               # plot starts after the labels
    plot_w = width - left_pad

    def set_px(x: int, y: int, c: RGBA, cov: float = 1.0) -> None:
        if not (0 <= x < width and 0 <= y < height):
            return
        a = int(c[3] * cov)
        if a <= 0:
            return
        p = (y * width + x) * 4
        ba = buf[p + 3]
        if a >= 255 or ba == 0:                      # opaque src, or empty dst
            buf[p] = c[0]; buf[p + 1] = c[1]; buf[p + 2] = c[2]; buf[p + 3] = a
        else:                                        # src-over onto current
            inv = 255 - a
            bai = ba * inv // 255
            out_a = a + bai
            if out_a:
                buf[p]     = min(255, (c[0] * a + buf[p]     * bai) // out_a)
                buf[p + 1] = min(255, (c[1] * a + buf[p + 1] * bai) // out_a)
                buf[p + 2] = min(255, (c[2] * a + buf[p + 2] * bai) // out_a)
                buf[p + 3] = min(255, out_a)

    def value_yf(v: float) -> float:
        v = 0.0 if v < 0 else vmax if v > vmax else v
        return floor_y - (v / vmax) * span

    def draw_digits(text: str, right_x: int, cy: int) -> None:
        """Digits right-aligned so their right edge ends at right_x, vertically
        centered on cy but kept fully inside the image (edge rows aren't clipped)."""
        top = max(0, min(cy - _DIGIT_H // 2, height - _DIGIT_H))
        x = right_x
        for ch in reversed(text):
            x -= _DIGIT_W
            glyph = _DIGITS[ch]
            for ry in range(_DIGIT_H):
                bits = glyph[ry]
                for rx in range(_DIGIT_W):
                    if bits[rx] == "1":
                        set_px(x + rx, top + ry, label)
            x -= 1                                   # inter-digit gap

    def curve_yf(s: list) -> dict:
        """Float y of the series at each plot column (series stretched across it)."""
        n = len(s)
        yf = {}
        for x in range(plot_x0, width):
            t = (x - plot_x0) / (plot_w - 1) * (n - 1) if n > 1 and plot_w > 1 else 0.0
            i = int(t)
            frac = t - i
            v = s[i] if i + 1 >= n else s[i] * (1 - frac) + s[i + 1] * frac
            yf[x] = value_yf(v)
        return yf

    def draw_line(yf: dict, color: RGBA) -> None:
        """Top line antialiased along its slope (no full-opacity vertical bars),
        so steep segments stay smooth instead of segmented."""
        prev = None
        for x in range(plot_x0, width):
            y = yf[x]
            if prev is None:                         # first column: 2px vert AA
                yi = int(y); fr = y - yi
                set_px(x, yi, color, cov=1 - fr); set_px(x, yi + 1, color, cov=fr)
            else:
                x0, y0f = prev
                dy = y - y0f
                if -1.0 <= dy <= 1.0:                # shallow: 2px vert AA at x
                    yi = int(y); fr = y - yi
                    set_px(x, yi, color, cov=1 - fr); set_px(x, yi + 1, color, cov=fr)
                else:                                # steep: step along y, AA in x
                    ystep = 1 if dy > 0 else -1
                    yi, yend = int(round(y0f)), int(round(y))
                    while True:
                        t = (yi - y0f) / dy
                        t = 0.0 if t < 0 else 1.0 if t > 1 else t
                        xf = x0 + t                  # sub-pixel x between columns
                        xb = int(xf); fx = xf - xb
                        set_px(xb, yi, color, cov=1 - fx); set_px(xb + 1, yi, color, cov=fx)
                        if yi == yend:
                            break
                        yi += ystep
            prev = (x, y)

    if series:
        yf = curve_yf(series)
        fr, fg, fb, fa = fill
        stride = width * 4
        # filled area: solid below the curve — written inline straight into the
        # still-transparent buffer (grid goes on top afterwards, so no per-pixel
        # blend here), with one antialiased partial pixel on the top edge
        for x in range(plot_x0, width):
            y = yf[x]
            y0 = int(y)
            set_px(x, y0, fill, cov=(y0 + 1 - y))    # top edge coverage
            p = ((y0 + 1) * width + x) * 4
            for _yy in range(y0 + 1, floor_y + 1):
                buf[p] = fr; buf[p + 1] = fg; buf[p + 2] = fb; buf[p + 3] = fa
                p += stride
        draw_line(yf, line)
    # optional second series, drawn as a line only (e.g. gpu decoder over usage)
    if overlay:
        draw_line(curve_yf(overlay), overlay_line)

    # horizontal grid (+ left y-axis labels) on top of the fill, plot area only
    for lv in grid_levels:
        gy = round(value_yf(lv))
        for x in range(plot_x0, width):
            set_px(x, gy, grid)
        if left_pad > 0 and label_values:
            draw_digits(str(int(lv)), left_pad - 2, gy)

    return _encode_png(width, height, buf)
