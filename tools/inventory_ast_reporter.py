#!/usr/bin/env python3
"""Machine-readable AST inventory for Python callables and call sites.

The report is intentionally syntactic: it walks ``ast.Call`` nodes and records
the literal callee expression, without attempting runtime import resolution or
dynamic dispatch. That makes it stable and stdlib-only, but it also means tools
like ``getattr(...)`` or decorator-driven replacement stay only partially
resolved. See ``tests/vulture_whitelist.py`` for concrete examples of dynamic
lookups that require human review outside a pure AST pass.

Per-file parse failures are recorded in the JSON ``errors`` list so a large tree
can still yield a partial report; the CLI exits non-zero whenever any such
errors are present.
"""

from __future__ import annotations

import argparse
import ast
import json
import sys
import tokenize
from contextlib import contextmanager
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Iterable


SCHEMA_VERSION = 1
DEFAULT_SCAN_PATHS = ("src", "tests", "tools", "pirostats")
LIMITATIONS = [
    "Syntactic AST only: call targets come from ast.Call.func text, not runtime resolution.",
    "Dynamic dispatch via getattr()/setattr(), monkeypatching, decorators that replace callables, and import alias indirection are not fully resolved.",
    "Only module-level functions/classes and methods defined directly on top-level classes are inventoried as callables; nested local defs stay attributed to their enclosing top-level scope.",
]


@dataclass(frozen=True)
class CallContext:
    qualname: str
    kind: str


def _display_path(path: Path, root: Path) -> str:
    try:
        return path.resolve().relative_to(root).as_posix()
    except ValueError:
        return path.resolve().as_posix()


def _read_source(path: Path) -> str:
    with tokenize.open(path) as handle:
        return handle.read()


def _callee_text(node: ast.AST) -> str:
    try:
        return ast.unparse(node)
    except Exception:
        return f"<{type(node).__name__}>"


def _callee_key(node: ast.AST) -> str:
    if isinstance(node, ast.Name):
        return node.id
    if isinstance(node, ast.Attribute):
        return f"{_callee_key(node.value)}.{node.attr}"
    if isinstance(node, ast.Call):
        return f"{_callee_key(node.func)}(...)"
    if isinstance(node, ast.Subscript):
        return f"{_callee_key(node.value)}[...]"
    if isinstance(node, ast.Lambda):
        return "<lambda>"
    if isinstance(node, ast.Await):
        return f"await {_callee_key(node.value)}"
    return _callee_text(node)


def _iter_targets(paths: Iterable[Path]) -> list[Path]:
    seen: set[Path] = set()
    out: list[Path] = []
    missing: list[str] = []

    for raw_path in paths:
        path = raw_path.resolve()
        if not path.exists():
            missing.append(raw_path.as_posix())
            continue
        if path.is_dir():
            for child in sorted(path.rglob("*.py")):
                child = child.resolve()
                if child.is_file() and child not in seen:
                    seen.add(child)
                    out.append(child)
            continue
        if path.is_file() and path not in seen:
            seen.add(path)
            out.append(path)

    if missing:
        joined = ", ".join(sorted(missing))
        raise FileNotFoundError(f"scan target not found: {joined}")

    return sorted(out)


class FileAnalyzer(ast.NodeVisitor):
    def __init__(self, display_path: str, source: str) -> None:
        self._display_path = display_path
        self._source = source
        self._contexts: list[CallContext] = []
        self.callables: list[dict[str, Any]] = []
        self.calls: list[dict[str, Any]] = []

    def analyze(self, tree: ast.Module) -> dict[str, Any]:
        for stmt in tree.body:
            self._visit_module_stmt(stmt)

        unique_callees = sorted({call["callee"] for call in self.calls})
        top_level_calls = [call for call in self.calls if call["top_level"]]
        function_count = sum(1 for entry in self.callables if entry["kind"] == "function")
        class_count = sum(1 for entry in self.callables if entry["kind"] == "class")
        method_count = sum(1 for entry in self.callables if entry["kind"] == "method")

        return {
            "path": self._display_path,
            "callable_count": len(self.callables),
            "function_count": function_count,
            "class_count": class_count,
            "method_count": method_count,
            "callables": self.callables,
            "total_call_sites": len(self.calls),
            "top_level_call_site_count": len(top_level_calls),
            "unique_syntactic_callee_count": len(unique_callees),
            "unique_syntactic_callees": unique_callees,
            "top_level_calls": top_level_calls,
            "call_sites": self.calls,
        }

    def _visit_module_stmt(self, stmt: ast.stmt) -> None:
        if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
            self._visit_recorded_function(stmt, qualname=stmt.name, kind="function", parent=None)
            return
        if isinstance(stmt, ast.ClassDef):
            self._visit_recorded_class(stmt, qualname=stmt.name, parent=None)
            return
        self.visit(stmt)

    def _visit_recorded_class(self, node: ast.ClassDef, *, qualname: str, parent: str | None) -> None:
        entry = self._record_callable(node, kind="class", qualname=qualname, parent=parent)
        self._visit_class_header(node)
        with self._context(entry["qualname"], entry["kind"]):
            for stmt in node.body:
                if isinstance(stmt, (ast.FunctionDef, ast.AsyncFunctionDef)):
                    self._visit_recorded_function(
                        stmt,
                        qualname=f"{entry['qualname']}.{stmt.name}",
                        kind="method",
                        parent=entry["qualname"],
                    )
                else:
                    self.visit(stmt)

    def _visit_recorded_function(
        self,
        node: ast.FunctionDef | ast.AsyncFunctionDef,
        *,
        qualname: str,
        kind: str,
        parent: str | None,
    ) -> None:
        entry = self._record_callable(node, kind=kind, qualname=qualname, parent=parent)
        self._visit_function_header(node)
        with self._context(entry["qualname"], entry["kind"]):
            for stmt in node.body:
                self.visit(stmt)

    def _visit_class_header(self, node: ast.ClassDef) -> None:
        for decorator in node.decorator_list:
            self.visit(decorator)
        for base in node.bases:
            self.visit(base)
        for keyword in node.keywords:
            self.visit(keyword)
        for type_param in getattr(node, "type_params", []):
            self.visit(type_param)

    def _visit_function_header(self, node: ast.FunctionDef | ast.AsyncFunctionDef) -> None:
        for decorator in node.decorator_list:
            self.visit(decorator)
        self.visit(node.args)
        if node.returns is not None:
            self.visit(node.returns)
        for type_param in getattr(node, "type_params", []):
            self.visit(type_param)

    def _record_callable(
        self,
        node: ast.FunctionDef | ast.AsyncFunctionDef | ast.ClassDef,
        *,
        kind: str,
        qualname: str,
        parent: str | None,
    ) -> dict[str, Any]:
        entry = {
            "name": node.name,
            "qualname": qualname,
            "kind": kind,
            "parent": parent,
            "is_async": isinstance(node, ast.AsyncFunctionDef),
            "line": node.lineno,
            "column": node.col_offset,
            "end_line": getattr(node, "end_lineno", node.lineno),
            "end_column": getattr(node, "end_col_offset", node.col_offset),
        }
        self.callables.append(entry)
        return entry

    @contextmanager
    def _context(self, qualname: str, kind: str):
        self._contexts.append(CallContext(qualname=qualname, kind=kind))
        try:
            yield
        finally:
            self._contexts.pop()

    def _current_context(self) -> CallContext | None:
        return self._contexts[-1] if self._contexts else None

    def visit_FunctionDef(self, node: ast.FunctionDef) -> None:
        self._visit_function_header(node)
        for stmt in node.body:
            self.visit(stmt)

    def visit_AsyncFunctionDef(self, node: ast.AsyncFunctionDef) -> None:
        self._visit_function_header(node)
        for stmt in node.body:
            self.visit(stmt)

    def visit_ClassDef(self, node: ast.ClassDef) -> None:
        self._visit_class_header(node)
        for stmt in node.body:
            self.visit(stmt)

    def visit_Call(self, node: ast.Call) -> None:
        context = self._current_context()
        enclosing_kind = context.kind if context is not None else "module"
        self.calls.append(
            {
                "callee": _callee_key(node.func),
                "callee_source": _callee_text(node.func),
                "callee_kind": type(node.func).__name__,
                "enclosing": context.qualname if context is not None else "<module>",
                "enclosing_kind": enclosing_kind,
                "top_level": enclosing_kind in {"module", "class"},
                "line": node.lineno,
                "column": node.col_offset,
                "end_line": getattr(node, "end_lineno", node.lineno),
                "end_column": getattr(node, "end_col_offset", node.col_offset),
            }
        )
        self.generic_visit(node)


def analyze_file(path: Path, *, root: Path) -> dict[str, Any]:
    source = _read_source(path)
    tree = ast.parse(source, filename=str(path), type_comments=True)
    analyzer = FileAnalyzer(_display_path(path, root), source)
    return analyzer.analyze(tree)


def build_report(scan_paths: Iterable[str], *, root: Path | None = None) -> dict[str, Any]:
    base = (root or Path.cwd()).resolve()
    requested = [Path(path) for path in scan_paths]
    files = _iter_targets(requested)

    file_reports: list[dict[str, Any]] = []
    errors: list[dict[str, Any]] = []

    for path in files:
        try:
            file_reports.append(analyze_file(path, root=base))
        except (OSError, SyntaxError, UnicodeDecodeError, ValueError) as exc:
            errors.append({
                "path": _display_path(path, base),
                "error_type": type(exc).__name__,
                "message": str(exc),
            })

    global_unique_callees = sorted(
        {callee for report in file_reports for callee in report["unique_syntactic_callees"]}
    )
    total_top_level_calls = sum(report["top_level_call_site_count"] for report in file_reports)

    return {
        "schema_version": SCHEMA_VERSION,
        "requested_paths": [path.as_posix() for path in requested],
        "limitations": LIMITATIONS,
        "summary": {
            "file_count": len(file_reports),
            "parse_error_count": len(errors),
            "callable_count": sum(report["callable_count"] for report in file_reports),
            "total_call_sites": sum(report["total_call_sites"] for report in file_reports),
            "top_level_call_site_count": total_top_level_calls,
            "unique_syntactic_callee_count": len(global_unique_callees),
        },
        "errors": errors,
        "files": file_reports,
    }


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        prog="inventory_ast_reporter.py",
        description="Emit a JSON AST inventory of Python callables and call sites.",
        epilog=(
            "Syntactic AST only: callees are reported from literal call syntax, so "
            "runtime getattr(), monkeypatching, and decorator-driven replacement stay "
            "only partially resolved. See tests/vulture_whitelist.py for examples of "
            "dynamic lookups that still require human review."
        ),
    )
    parser.add_argument(
        "paths",
        nargs="*",
        default=list(DEFAULT_SCAN_PATHS),
        help="Files or directories to scan (default: src tests tools pirostats)",
    )
    parser.add_argument(
        "--pretty",
        action="store_true",
        help="Pretty-print the JSON report instead of emitting compact JSON.",
    )
    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    try:
        report = build_report(args.paths)
    except FileNotFoundError as exc:
        parser.error(str(exc))

    json.dump(
        report,
        sys.stdout,
        indent=2 if args.pretty else None,
        ensure_ascii=False,
        separators=None if args.pretty else (",", ":"),
    )
    sys.stdout.write("\n")
    return 1 if report["errors"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
