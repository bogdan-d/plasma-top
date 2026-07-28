#!/usr/bin/env python3
"""Pixel-diff two PNGs through Qt's QImage (same engine as qt_shot.py).

Reports: dimensions match, mean abs per-channel delta, max delta, and the
fraction of differing pixels. Used by tools/p6_qt_matrix.sh to produce the
deterministic Qt-rendering parity report for P6.2.

Exit codes:
  0  dimensions and all configured delta limits pass
  1  dimensions differ or any configured delta limit fails
  2  usage / file-access error
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from PyQt6.QtGui import QImage


def _delta(a: QImage, b: QImage) -> tuple[float, int, float] | None:
    if a.size() != b.size():
        return None
    w, h = a.width(), a.height()
    total = w * h
    # RGBA8 view: compare bytes directly, independent of QImage format quirks.
    a8 = a.convertToFormat(QImage.Format.Format_RGBA8888)
    b8 = b.convertToFormat(QImage.Format.Format_RGBA8888)
    a_bits = a8.constBits()
    b_bits = b8.constBits()
    a_bits.setsize(total * 4)
    b_bits.setsize(total * 4)
    a_bytes = bytes(a_bits)
    b_bytes = bytes(b_bits)
    abs_sum = 0
    max_delta = 0
    differing = 0
    for i in range(total):
        off = i * 4
        d_r = abs(a_bytes[off] - b_bytes[off])
        d_g = abs(a_bytes[off + 1] - b_bytes[off + 1])
        d_b = abs(a_bytes[off + 2] - b_bytes[off + 2])
        d_a = abs(a_bytes[off + 3] - b_bytes[off + 3])
        m = max(d_r, d_g, d_b, d_a)
        if m > 0:
            differing += 1
        abs_sum += d_r + d_g + d_b + d_a
        if m > max_delta:
            max_delta = m
    mean = abs_sum / (total * 4)
    return mean, max_delta, differing / total


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument("baseline", type=Path, help="reference PNG")
    ap.add_argument("candidate", type=Path, help="PNG under test")
    ap.add_argument(
        "--max-mean",
        type=float,
        default=4.0,
        help="tolerance on mean abs per-channel delta (default 4.0/255)",
    )
    ap.add_argument(
        "--max-pixel",
        type=int,
        default=255,
        help="maximum allowed single-channel delta (default 255)",
    )
    ap.add_argument(
        "--max-fraction",
        type=float,
        default=1.0,
        help="maximum fraction of pixels allowed to differ (default 1.0)",
    )
    ap.add_argument("--json", action="store_true", help="emit a single JSON line")
    args = ap.parse_args()

    if not args.baseline.is_file():
        print(f"baseline missing: {args.baseline}", file=sys.stderr)
        return 2
    if not args.candidate.is_file():
        print(f"candidate missing: {args.candidate}", file=sys.stderr)
        return 2

    base = QImage(str(args.baseline))
    cand = QImage(str(args.candidate))
    if base.isNull():
        print(f"baseline unreadable: {args.baseline}", file=sys.stderr)
        return 2
    if cand.isNull():
        print(f"candidate unreadable: {args.candidate}", file=sys.stderr)
        return 2

    result = _delta(base, cand)
    if result is None:
        msg = {
            "dimensions_match": False,
            "baseline": f"{base.width()}x{base.height()}",
            "candidate": f"{cand.width()}x{cand.height()}",
            "status": "DIMENSION_MISMATCH",
        }
    else:
        mean, max_delta, frac = result
        ok = (
            mean <= args.max_mean
            and max_delta <= args.max_pixel
            and frac <= args.max_fraction
        )
        msg = {
            "dimensions_match": True,
            "size": f"{base.width()}x{base.height()}",
            "mean_abs_delta": round(mean, 4),
            "max_delta": max_delta,
            "differing_pixel_fraction": round(frac, 4),
            "tolerance_mean": args.max_mean,
            "tolerance_pixel": args.max_pixel,
            "tolerance_fraction": args.max_fraction,
            "status": "PASS" if ok else "FAIL_DELTA",
        }

    if args.json:
        import json

        print(json.dumps(msg))
    else:
        for k, v in msg.items():
            print(f"{k}: {v}")
    if result is None:
        return 1
    mean, max_delta, frac = result
    return 0 if (
        mean <= args.max_mean
        and max_delta <= args.max_pixel
        and frac <= args.max_fraction
    ) else 1


if __name__ == "__main__":
    raise SystemExit(main())
