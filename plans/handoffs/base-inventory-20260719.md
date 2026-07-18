# Handoff: `BASE` / `base-inventory-20260719`

## Contract

- Objective: Implement Phase 0.6 starter only: add a stdlib-only AST callable/call-edge reporter plus a focused validation test, without touching production runtime code or updating `plans/INVENTORY.md` yet.
- Integration base SHA: `ea3e2fcd033f18ae143d95b99b767c8bf4b9ca7a`
- Branch/worktree: `rust-migration-base-bootstrap` @ `/var/mnt/xdata/code/_self/plasma-stats`
- Owned paths:
  - `tools/inventory_ast_reporter.py`
  - `tests/test_inventory.py`
  - `plans/handoffs/base-inventory-20260719.md`
- Forbidden/shared paths:
  - `src/**`
  - existing `tests/**` files other than `tests/test_inventory.py`
  - `config/**`
  - `style/**`
  - `plans/STATUS.md`
  - `plans/LANES.md`
  - `plans/INVENTORY.md`
  - `README.md`
  - `AGENTS.md`
  - `packaging/**`
  - `install.sh`
  - `uninstall.sh`
  - `.gitignore` files
- Dependencies verified integrated: none; this starter slice is standalone and stdlib-only.

## Result

- Status: `handoff`
- Commits: none (working tree changes only)
- Changed files:
  - `tools/inventory_ast_reporter.py` — stdlib-only JSON reporter for Python callables, call sites, and per-file call-edge summary counts
  - `tests/test_inventory.py` — focused CLI smoke test that asserts broad inventory invariants across `src/`, `tests/`, `tools/`, and `pirostats`
  - `plans/handoffs/base-inventory-20260719.md` — this handoff
- Behavior implemented/preserved:
  - reporter recursively scans passed paths and includes the root `pirostats` entrypoint when requested
  - report lists module-level functions, module-level classes, and methods defined directly on top-level classes
  - report emits JSON to stdout by default, with `--pretty` for readable output
  - each file report includes discovered callables, total call sites, top-level call-site subset, and unique normalized syntactic callees
  - limitations are explicit in both the report metadata and CLI help text: syntactic AST only, no full runtime resolution for dynamic lookup or dispatch
  - nested local defs are not promoted to top-level inventory entries; their call sites stay attributed to the enclosing top-level scope
  - no production Python files, configs, styles, or existing tests were modified
- Explicitly not implemented:
  - no update to `plans/INVENTORY.md`
  - no exact-count gate against the historical markdown totals
  - no runtime import resolution, decorator replacement analysis, or `getattr`/monkeypatch tracing
  - no CI integration or persisted inventory artifact beyond the local smoke output

## Parity evidence

- Current Python symbols/files covered:
  - `src/config.py` → representative callable `load_config`
  - `src/sensors.py` → representative callable `collect`
  - `src/daemon.py` → representative callable `main`
  - `src/mono_render.py` → representative callable `render_blocks_monospace`
  - root `pirostats` script → included in the report as a scanned Python entrypoint without a `.py` suffix
- Oracle fixtures/cases:
  - `tests/test_inventory.py::test_inventory_ast_reporter_workspace_smoke`
- Exact differences remaining:
  - the report is intentionally syntactic, so dynamic lookup patterns documented in `tests/vulture_whitelist.py` still need human interpretation when the controller later maps edges into `plans/INVENTORY.md`
  - unique callee normalization is stable and useful for accounting, but the controller should decide whether these normalized families are the canonical form for the later markdown gate
- Inventory entries proposed resolved: none; this slice delivers the generator + validation only.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `. .venv/bin/activate && python -m pytest tests/test_inventory.py -v` | pass | Focused validation passed: `1 passed`; exercises the reporter CLI against `src`, `tests`, `tools`, and `pirostats`. |
| `.venv/bin/python tools/inventory_ast_reporter.py src tests tools pirostats >/tmp/inventory_ast_report.json` | pass | Direct tool smoke succeeded and wrote a machine-readable report artifact to `/tmp/inventory_ast_report.json`. |
| `.venv/bin/python - <<'PY' ...` | pass | Summary readback from `/tmp/inventory_ast_report.json`: `file_count=39`, `parse_error_count=0`; sampled file counts included `src/config.py 47/211/94`, `src/sensors.py 92/575/265`, `src/daemon.py 27/388/148`, `src/mono_render.py 12/75/25`, `pirostats 0/10/9` (callables / call sites / unique callees). |
| `. .venv/bin/activate && python -m pytest tests/test_lint.py tests/test_deadcode.py -v` | pass | `tests/test_deadcode.py` passed, and the repo-wide lint gate is now green after the integration owner fixed `tests/oracle.py` by deferring the checkout `sys.path` bootstrap into a lazy runtime helper. |
| `. .venv/bin/activate && ruff check tools/inventory_ast_reporter.py tests/test_inventory.py` | pass | Scoped lint on the newly added files passed cleanly. |
| `git diff --check` | pass | No whitespace or patch-format issues in the scoped changes. |

## Dependencies and safety

- New/changed dependencies and review:
  - none; the reporter and its test use only the Python standard library
- Native/build/runtime requirements:
  - validated in the existing project `.venv` on Python `3.14.6`
  - relies on `ast.unparse`, so the practical floor is Python `3.9+` (the repo already targets Python `3.11+`)
  - no build step, service change, or packaging change introduced
- Unsafe/FFI locations and invariants:
  - none touched

## Risks/blockers

- Known risks:
  - dynamic dispatch (`getattr`, decorator replacement, monkeypatching, runtime aliasing) remains intentionally unresolved by this slice
  - future source growth will naturally change counts; the test intentionally checks broad invariants instead of pinning exact totals
  - the normalized unique-callee form is suitable for planning, but the controller should confirm it is the exact representation desired before wiring it into the markdown gate
  - the generated report is intentionally a planning artifact: the next P0.6 step still needs to consume it into `plans/INVENTORY.md` or an equivalent integration gate
- Blocker requiring integration decision:
  - choose the canonical unique-callee normalization for the later `plans/INVENTORY.md` update slice
- Suggested next lane/API change:
  - consume this JSON report in the next Phase 0.6 slice to update or verify `plans/INVENTORY.md` call-edge accounting, with explicit handling notes for dynamic-lookup exceptions

## Review notes

- Diff inspected for out-of-scope paths: yes
- Production runtime untouched by tests: yes
- No skipped/weakened checks: yes
- Rebase required before merge: no