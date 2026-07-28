"""Dead-code gate: nothing in the repo is defined and never used.

Runs vulture over the sources plus tests/vulture_whitelist.py, which marks the
names only reachable through a runtime getattr (see it for the why of each).
Skipped when vulture isn't installed, so the suite still runs on a bare
checkout — the repo has no dependencies and this test doesn't add one.

MIN_CONFIDENCE is vulture's own: 60 reports unused variables/attributes too
(where a dynamic lookup can fool it — hence the whitelist), 100 only the cases
it can prove. 60 is what catches a forgotten helper or a config field nothing
reads; the whitelist is the price.
"""
import pathlib

import pytest

vulture = pytest.importorskip("vulture", reason="vulture not installed")

_REPO = pathlib.Path(__file__).resolve().parent.parent
_WHITELIST = pathlib.Path(__file__).with_name("vulture_whitelist.py")
MIN_CONFIDENCE = 60


def test_no_dead_code():
    v = vulture.Vulture()
    v.scavenge([str(_REPO / "src"), str(_REPO / "tests"),
                str(_REPO / "tools" / "python_oracle.py"),
                str(_WHITELIST)])
    dead = [f"{pathlib.Path(i.filename).relative_to(_REPO)}:{i.first_lineno}: "
            f"unused {i.typ} {i.name!r}"
            for i in v.get_unused_code(min_confidence=MIN_CONFIDENCE)]
    assert not dead, (
        "dead code — remove it, or if a runtime getattr reaches it, add it to "
        "tests/vulture_whitelist.py with the lookup that does:\n  " + "\n  ".join(dead))
