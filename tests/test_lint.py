"""Lint gate: ruff over the whole tree, config in ruff.toml at the repo root.

Companion to test_deadcode.py. Where vulture catches code nothing reaches, ruff
(its default E4/E7/E9 + F rules) catches the bugs a static reader can see in one
file: a dead import, an unused local, an undefined or redefined name. ruff.toml
silences only the pycodestyle style rules this repo breaks on purpose, so a green
tree stays green and a failure here is something new worth fixing.

Skipped when ruff isn't installed, so the suite still runs on a bare checkout —
the repo has no dependencies and this test doesn't add one. `pacman -S ruff` /
`pip install ruff`.
"""
import pathlib
import shutil
import subprocess

import pytest

_REPO = pathlib.Path(__file__).resolve().parent.parent
_RUFF = shutil.which("ruff")


@pytest.mark.skipif(_RUFF is None, reason="ruff not installed")
def test_ruff_clean():
    proc = subprocess.run(
        [_RUFF, "check", "--output-format", "concise", "."],
        cwd=_REPO, capture_output=True, text=True,
    )
    assert proc.returncode == 0, (
        "ruff found issues — fix them, or if it's a deliberate house style, add the "
        "rule to ruff.toml's ignore list with the reason:\n" + (proc.stdout or proc.stderr))
