# Handoff: `BASE` / `base-inventory-gate-20260719`

## Contract

- Objective: Integrate the existing AST reporter evidence into the Phase 0.6 markdown inventory gate with the smallest truthful change.
- Integration base SHA: `9b1256c3e4a02339c977a9b57ac76339f8c92d51`
- Branch/worktree: `rust-migration-base-bootstrap` @ `/var/mnt/xdata/code/_self/plasma-stats`
- Owned paths:
  - `tests/test_inventory.py`
  - `plans/INVENTORY.md`
  - `plans/handoffs/base-inventory-gate-20260719.md`
- Forbidden/shared paths:
  - `src/**`
  - `tools/**`
  - `config/**`
  - `style/**`
  - `plans/STATUS.md`
  - `plans/LANES.md`
  - `README.md`
  - `AGENTS.md`
  - `packaging/**`
  - `install.sh`
  - `uninstall.sh`
  - existing tests other than `tests/test_inventory.py`
- Dependencies verified integrated:
  - pre-existing `tools/inventory_ast_reporter.py`

## Result

- Status: `handoff`
- Commits: none (working tree changes only)
- Changed files:
  - `tests/test_inventory.py` — keeps the reporter smoke test and adds an exact markdown gate for call-edge counts
  - `plans/INVENTORY.md` — adds provenance wording and syncs the call-edge table to the current AST report, including newly reported files
  - `plans/handoffs/base-inventory-gate-20260719.md` — this handoff
- Behavior implemented/preserved:
  - `tests/test_inventory.py` now parses the `Call-edge accounting gate` table in `plans/INVENTORY.md`
  - the new gate compares reporter output to the markdown rows for per-file `Call sites` and `Unique syntactic callees`
  - failures are actionable: missing rows, stale rows, and count drift are reported separately with concrete file names and live counts
  - the markdown table now covers the current report set from `src`, `tests`, `tools`, and `pirostats`
- Explicitly not implemented:
  - no auto-regeneration of `plans/INVENTORY.md`
  - no broader callable/disposition inventory edits outside the call-edge table
  - no edits to `src/**`, `tools/**`, or other test files

## Parity evidence

- Current Python symbols/files covered:
  - per-file call-edge counts for the current `tools/inventory_ast_reporter.py` scan set: `src`, `tests`, `tools`, `pirostats`
- Oracle fixtures/cases:
  - `tests/test_inventory.py::test_inventory_ast_reporter_workspace_smoke`
  - `tests/test_inventory.py::test_inventory_call_edge_table_matches_ast_reporter`
- Exact differences remaining:
  - the gate proves the markdown table stays in sync with the reporter, but it does not regenerate the table
  - broader per-symbol callable ledger additions for newer files such as `tests/oracle.py`, `tests/test_oracle.py`, and `tools/inventory_ast_reporter.py` remain out of scope for this slice
- Inventory entries proposed resolved:
  - `Call-edge accounting gate` row coverage/count sync only

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `. .venv/bin/activate && python -m pytest tests/test_inventory.py -v` | pass | `2 passed`; smoke + exact markdown gate both green |
| `. .venv/bin/activate && python -m pytest tests/test_lint.py tests/test_deadcode.py -v` | pass | repo-level lint/dead-code gates still green |
| `git diff --check` | pass | no whitespace or patch-format issues |

## Dependencies and safety

- New/changed dependencies and review:
  - none; the slice only uses the pre-existing reporter plus Python stdlib in the test
- Native/build/runtime requirements:
  - validated in the existing project `.venv` on Linux/Python 3.14.6
- Unsafe/FFI locations and invariants:
  - none touched

## Risks/blockers

- Known risks:
  - call-edge counts will legitimately drift whenever scanned Python files change, so follow-up edits must update the markdown table deliberately
  - the gate is intentionally strict about row set parity; adding/removing scanned files now requires touching `plans/INVENTORY.md`
- Blocker requiring integration decision:
  - none for this slice
- Suggested next lane/API change:
  - continue Phase 0.6 by deciding whether the newer files added to the call-edge table also need explicit per-symbol callable/disposition sections in `plans/INVENTORY.md`

## Review notes

- Diff inspected for out-of-scope paths: yes
- Production runtime untouched by tests: yes
- No skipped/weakened checks: yes
- Rebase required before merge: no
