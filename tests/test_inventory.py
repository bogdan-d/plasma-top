"""Validation for the Phase 0 AST inventory reporter and markdown gate."""

from __future__ import annotations

from functools import cache
import json
from pathlib import Path
import re
import subprocess
import sys


_REPO = Path(__file__).resolve().parent.parent
_REPORTER = _REPO / "tools" / "inventory_ast_reporter.py"
_INVENTORY = _REPO / "plans" / "INVENTORY.md"
_SCAN_TARGETS = ("src", "tests", "tools")
_CALL_EDGE_HEADING = "## Call-edge accounting gate"
_CALL_EDGE_ROW = re.compile(
    r"^\| `(?P<path>[^`]+)` \| (?P<calls>\d+) \| (?P<unique>\d+) \|$"
)


@cache
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


def _file_counts(report: dict) -> dict[str, tuple[int, int]]:
    return {
        entry["path"]: (
            entry["total_call_sites"],
            entry["unique_syntactic_callee_count"],
        )
        for entry in report["files"]
    }


@cache
def _call_edge_rows() -> dict[str, tuple[int, int]]:
    text = _INVENTORY.read_text(encoding="utf-8")
    _before, heading, after = text.partition(_CALL_EDGE_HEADING)
    assert heading, f"missing `{_CALL_EDGE_HEADING}` in `{_INVENTORY.relative_to(_REPO)}`"

    section = after.split("\n## ", 1)[0]
    rows: dict[str, tuple[int, int]] = {}
    duplicates: list[str] = []

    for raw_line in section.splitlines():
        line = raw_line.strip()
        match = _CALL_EDGE_ROW.fullmatch(line)
        if not match:
            continue
        path = match.group("path")
        counts = (int(match.group("calls")), int(match.group("unique")))
        if path in rows:
            duplicates.append(path)
        rows[path] = counts

    assert rows, f"no call-edge rows found under `{_CALL_EDGE_HEADING}`"
    assert not duplicates, (
        "duplicate call-edge rows in `plans/INVENTORY.md`: "
        + ", ".join(sorted(duplicates))
    )
    return rows


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
        "tools/python_oracle.py",
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

    for busy_path in (
        "src/config.py",
        "src/sensors.py",
        "src/daemon.py",
        "tools/python_oracle.py",
    ):
        assert files[busy_path]["total_call_sites"] > 0

    assert files["src/sensors.py"]["unique_syntactic_callee_count"] > 10

    global_unique = {
        callee
        for file_report in report["files"]
        for callee in file_report["unique_syntactic_callees"]
    }
    assert report["summary"]["unique_syntactic_callee_count"] == len(global_unique)


def test_inventory_call_edge_table_matches_ast_reporter():
    report_counts = _file_counts(_run_report())
    table_counts = _call_edge_rows()
    problems: list[str] = []

    missing = sorted(report_counts.keys() - table_counts.keys())
    if missing:
        problems.append(
            "Add call-edge rows to `plans/INVENTORY.md` for:\n"
            + "\n".join(
                (
                    f"- `{path}`: Call sites {report_counts[path][0]}, "
                    f"Unique syntactic callees {report_counts[path][1]}"
                )
                for path in missing
            )
        )

    extra = sorted(table_counts.keys() - report_counts.keys())
    if extra:
        problems.append(
            "Remove or rename stale call-edge rows in `plans/INVENTORY.md` for:\n"
            + "\n".join(f"- `{path}`" for path in extra)
        )

    drift: list[str] = []
    for path in sorted(report_counts.keys() & table_counts.keys()):
        actual_calls, actual_unique = report_counts[path]
        table_calls, table_unique = table_counts[path]
        if actual_calls != table_calls or actual_unique != table_unique:
            drift.append(
                f"- `{path}`: Call sites {table_calls} -> {actual_calls}; "
                f"Unique syntactic callees {table_unique} -> {actual_unique}"
            )

    if drift:
        problems.append(
            "Update the `Call-edge accounting gate` table in `plans/INVENTORY.md`:\n"
            + "\n".join(drift)
        )

    assert not problems, "\n\n".join(problems)
