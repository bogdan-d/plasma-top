from pathlib import Path
import sys


sys.path.insert(0, str(Path(__file__).resolve().parent))

import oracle


FIXTURE = Path(__file__).parent / "fixtures" / "oracle_render_full.toml"
GOLDEN = Path(__file__).parent / "golden"
CASES = ["panel_v", "panel_h", "tooltip"]


def test_oracle_fixture_matches_existing_goldens():
    for component in CASES:
        expected = (GOLDEN / f"{component}.html").read_text()
        assert oracle.render_fixture(FIXTURE, component) == expected
