# Handoff: `BASE` / `base-inventory-detail-20260719`

## Contract

- Objective: Extend `plans/INVENTORY.md` so the newer Phase 0 oracle/inventory tooling files are represented in both the file inventory and callable inventory, not just in the call-edge accounting table.
- Integration base SHA: `9842a94faf5f02414ef280fa3f549cfcd4fe603d`
- Branch/worktree: `rust-migration-base-bootstrap` @ `/var/mnt/xdata/code/_self/plasma-stats`
- Owned paths:
  - `plans/INVENTORY.md`
  - `plans/handoffs/base-inventory-detail-20260719.md`
- Forbidden/shared paths:
  - `src/**`
  - `tests/**`
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
  - `.gitignore` files
- Dependencies verified integrated:
  - pre-existing `tests/oracle.py`
  - pre-existing `tests/test_oracle.py`
  - pre-existing `tests/test_inventory.py`
  - pre-existing `tools/inventory_ast_reporter.py`

## Result

- Status: `handoff`
- Commits: none (working tree changes only)
- Changed files:
  - `plans/INVENTORY.md` — adds missing file-inventory rows plus callable inventory sections for the Phase 0 oracle/inventory evidence files using current AST-derived line numbers
  - `plans/handoffs/base-inventory-detail-20260719.md` — this handoff
- Behavior implemented/preserved:
  - file inventory rows now explicitly cover `tests/oracle.py`, `tests/test_oracle.py`, `tests/test_inventory.py`, and `tools/inventory_ast_reporter.py`
  - callable inventory now includes those same files with the current callable names, kinds, and line numbers from the live AST reporter output
  - the added entries are scoped to `BASE/INTEGRATION` evidence/tooling work, not product runtime lanes
  - the existing call-edge accounting table was left unchanged
- Explicitly not implemented:
  - no edits to `src/**`, `tests/**`, `tools/**`, or any forbidden path
  - no changes to call-edge counts or row set in the accounting gate table
  - no new generator or automation beyond the existing reporter/test flow

## Parity evidence

- Current Python symbols/files covered:
  - `tests/oracle.py` file ledger row + 14 callable rows
  - `tests/test_oracle.py` file ledger row + 1 callable row
  - `tests/test_inventory.py` file ledger row + 6 callable rows
  - `tools/inventory_ast_reporter.py` file ledger row + 25 callable rows
- Oracle fixtures/cases:
  - `tests/oracle.py::load_fixture`
  - `tests/oracle.py::render_component`
  - `tests/oracle.py::render_fixture`
  - `tests/test_oracle.py::test_oracle_fixture_matches_existing_goldens`
  - `tests/test_inventory.py::test_inventory_ast_reporter_workspace_smoke`
  - `tests/test_inventory.py::test_inventory_call_edge_table_matches_ast_reporter`
- Exact differences remaining:
  - these entries are still planning/disposition evidence; they do not yet prove Rust replacements exist
  - future callable drift in the listed files will require deliberate `plans/INVENTORY.md` updates to keep the ledger truthful
- Inventory entries proposed resolved:
  - missing file-inventory coverage for the four Phase 0 oracle/inventory files
  - missing callable-inventory coverage for the same four files

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `. .venv/bin/activate && python -m pytest tests/test_inventory.py -v` | pass | `2 passed in 0.17s`; inventory reporter smoke + call-edge markdown gate remain green |
| `git diff --check` | pass | no whitespace or patch-format issues after the documentation-only slice |

## Dependencies and safety

- New/changed dependencies and review:
  - none; documentation-only slice using the pre-existing AST reporter for line-number truth
- Native/build/runtime requirements:
  - validated in the existing repo `.venv` on Linux/Python 3.14.6
- Unsafe/FFI locations and invariants:
  - none touched

## Risks/blockers

- Known risks:
  - the new callable tables are intentionally exact, so future edits to these tooling files can stale the ledger unless follow-up slices refresh line numbers/names
- Blocker requiring integration decision:
  - none for this slice
- Suggested next lane/API change:
  - continue Phase 0 inventory closure by mapping these new BASE/INTEGRATION entries to specific retained checks or Rust replacement evidence as those slices land

## Review notes

- Diff inspected for out-of-scope paths: yes
- Production runtime untouched by tests: yes
- No skipped/weakened checks: yes
- Rebase required before merge: no
