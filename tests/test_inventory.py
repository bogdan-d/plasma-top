"""Smoke validation for the Phase 0 AST inventory reporter.

The invariants stay broad on purpose: this slice adds the generator and checks
that it finds representative symbols and files without pinning the exact counts
to today's source tree size.
"""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys


_REPO = Path(__file__).resolve().parent.parent
_REPORTER = _REPO / "tools" / "inventory_ast_reporter.py"
_SCAN_TARGETS = ("src", "tests", "tools", "pirostats")


def _run_report() -> dict:
    proc = subprocess.run(
        [sys.executable, str(_REPORTER), *_SCAN_TARGETS],
        cwd=_REPO,
        capture_output=True,
        text=True,
    )
    assert proc.returncode == 0, (
        "inventory_ast_reporter failed:\n"
        f"stdout:\n{proc.stdout}\n"
        f"stderr:\n{proc.stderr}"
    )
    return json.loads(proc.stdout)


def _names(report: dict) -> set[str]:
    return {entry["name"] for entry in report["callables"]}


def test_inventory_ast_reporter_workspace_smoke():
    report = _run_report()

    assert report["schema_version"] == 1
    assert report["requested_paths"] == list(_SCAN_TARGETS)
    assert not report["errors"]
    assert report["summary"]["file_count"] == len(report["files"])
    assert report["summary"]["file_count"] >= 10

    limitations = " ".join(report["limitations"]).lower()
    assert "syntactic" in limitations
    assert "getattr" in limitations or "dynamic" in limitations

    files = {entry["path"]: entry for entry in report["files"]}
    for required_path in (
        "src/config.py",
        "src/sensors.py",
        "src/daemon.py",
        "src/mono_render.py",
        "tests/test_config.py",
        "tools/qt_shot.py",
        "pirostats",
    ):
        assert required_path in files

    for file_report in report["files"]:
        assert file_report["callable_count"] == len(file_report["callables"])
        assert file_report["total_call_sites"] == len(file_report["call_sites"])
        assert file_report["top_level_call_site_count"] == len(file_report["top_level_calls"])
        assert file_report["unique_syntactic_callee_count"] == len(file_report["unique_syntactic_callees"])
        assert file_report["top_level_call_site_count"] <= file_report["total_call_sites"]
        if file_report["call_sites"]:
            assert min(call["line"] for call in file_report["call_sites"]) >= 1
            assert all(call["callee"] for call in file_report["call_sites"])

    assert "load_config" in _names(files["src/config.py"])
    assert "collect" in _names(files["src/sensors.py"])
    assert "main" in _names(files["src/daemon.py"])
    assert "render_blocks_monospace" in _names(files["src/mono_render.py"])

    for busy_path in ("src/config.py", "src/sensors.py", "src/daemon.py", "pirostats"):
        assert files[busy_path]["total_call_sites"] > 0

    assert files["src/sensors.py"]["unique_syntactic_callee_count"] > 10

    global_unique = {
        callee
        for file_report in report["files"]
        for callee in file_report["unique_syntactic_callees"]
    }
    assert report["summary"]["unique_syntactic_callee_count"] == len(global_unique)
