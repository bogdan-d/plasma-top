# Handoff: `BASE` / `base-p0.7-local-capture-20260719`

## Contract

- Objective: Refresh the current-host Phase 0 live-evidence bundle after enabling Qt screenshot capture in the validated `.venv`, without changing product code.
- Integration base SHA: `78986cfb1b7e60e5742e2b621093787c2cefce74`
- Branch/worktree: `rust-migration-base-bootstrap` @ `/var/mnt/xdata/code/_self/plasma-stats`
- Owned paths:
  - `plans/handoffs/base-p0.7-local-capture-20260719.md`
  - `plans/STATUS.md`
- Forbidden/shared paths:
  - `src/**`
  - `tests/**`
  - `tools/**`
  - `config/**`
  - `style/**`
  - `plans/LANES.md`
  - `plans/INVENTORY.md`
  - `README.md`
  - `AGENTS.md`
  - `packaging/**`
  - `install.sh`
  - `uninstall.sh`
  - `.gitignore` files
- Dependencies verified integrated:
  - `scripts/capture-baseline.sh`
  - `.test-artifacts/.gitignore`
  - existing `.venv` baseline environment

## Result

- Status: `handoff`
- Commits: none (working tree changes only)
- Changed files:
  - `plans/handoffs/base-p0.7-local-capture-20260719.md` — this evidence handoff
  - `plans/STATUS.md` — narrows the remaining Phase 0 blocker wording
- Behavior implemented/preserved:
  - installed `PyQt6` into the verified `.venv`
  - reran `./scripts/capture-baseline.sh`
  - refreshed the ignored `.test-artifacts/` bundle with successful Qt screenshots for tooltip, horizontal panel, and vertical panel
  - preserved product/runtime code unchanged
- Explicitly not implemented:
  - no additional hardware support (no NVIDIA/NVML, UPower/gi, or HID enablement)
  - no multi-host evidence capture
  - no Rust scaffolding in this slice

## Parity evidence

- Current Python symbols/files covered:
  - `scripts/capture-baseline.sh`
  - `tools/qt_shot.py`
  - current-host `./pirostats probe`, `./pirostats profiling`, and render outputs
- Oracle fixtures/cases: none (live-evidence refresh only)
- Exact differences remaining:
  - current-host evidence now includes Qt PNG renders, but this machine still lacks NVIDIA/NVML, `gi`/PyGObject, and HID coverage
  - broader Phase 0.7 multi-host evidence remains an external hardware availability issue rather than a local tooling issue
- Inventory entries proposed resolved: none

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `. .venv/bin/activate && python -m pip install PyQt6` | pass | Added local Qt rendering support to the validated environment without changing repo files. |
| `./scripts/capture-baseline.sh` | pass | Refreshed `.test-artifacts/summary.txt` with `required_failures=0` and successful Qt capture entries. |
| `ls .test-artifacts/live-host-evidence/qt-shots` | pass | Confirmed `tooltip.png`, `panel-horizontal.png`, and `panel-vertical.png` plus their logs are present. |
| `python -m pytest tests/test_lint.py tests/test_deadcode.py -v` | pass | Existing repo-level lint/dead-code gates remained green before the evidence refresh. |

## Dependencies and safety

- New/changed dependencies and review:
  - local environment only: `PyQt6==6.11.0` installed into `.venv`
- Native/build/runtime requirements:
  - requires a working Qt offscreen stack in the current Python environment for screenshot capture
  - evidence remains local under ignored `.test-artifacts/`
- Unsafe/FFI locations and invariants:
  - none touched

## Risks/blockers

- Known risks:
  - `.test-artifacts/` contains machine-specific local evidence and should be reviewed/redacted before external sharing
  - the improved local evidence does not change the absence of non-local hardware variants
- Blocker requiring integration decision:
  - none for this slice
- Suggested next lane/API change:
  - proceed to Rust Phase 1 scaffold work while treating the remaining Phase 0.7 gap as external-hardware follow-up, unless additional local host evidence opportunities are discovered

## Review notes

- Diff inspected for out-of-scope paths: yes
- Production runtime untouched by tests: yes
- No skipped/weakened checks: yes
- Rebase required before merge: no