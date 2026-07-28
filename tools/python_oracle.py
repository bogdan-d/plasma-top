#!/usr/bin/env python3
"""Explicit launcher for the retained Python compatibility oracle."""
import sys
from pathlib import Path


_src = Path(__file__).resolve().parent.parent / "src"
if str(_src) not in sys.path:
    sys.path.insert(0, str(_src))

# Keep paging cheap for baseline captures that exercise one process per notch.
if sys.argv[1:2] == ["page"]:
    from pagestate import step_page
    raise SystemExit(step_page(sys.argv[2] if len(sys.argv) > 2 else ""))

from daemon import main


if __name__ == "__main__":
    main()
