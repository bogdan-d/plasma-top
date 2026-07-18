# Handoff: `BASE` / `base-baseline-capture-20260719`

## Contract

- Objective: Implement the Phase 0 evidence-capture harness only: cover P0.1/P0.3 and lay down useful ignored live-host evidence capture for P0.7 without changing production Python behavior.
- Integration base SHA: `225b1f62c41e1eb50b6fdcd23f0e959d1eccbd13`
- Branch/worktree: `rust-migration-base-bootstrap` @ `/var/mnt/xdata/code/_self/plasma-stats`
- Owned paths:
  - `.test-artifacts/.gitignore`
  - `scripts/capture-baseline.sh`
  - `plans/handoffs/base-baseline-capture-20260719.md`
- Forbidden/shared paths:
  - `src/**`
  - `tests/**`
  - `config/**`
  - `style/**`
  - `plans/STATUS.md`
  - `plans/LANES.md`
  - `README.md`
  - `AGENTS.md`
  - `packaging/**`
  - `install.sh`
  - `uninstall.sh`
  - root `.gitignore`
- Dependencies verified integrated: prior BASE setup/oracle work already present on this branch; this slice only drives the existing Python entrypoint and optional Qt preview tool.

## Result

- Status: `handoff`
- Commits: none (working tree changes only)
- Changed files:
  - `.test-artifacts/.gitignore` — tracks the artifact root while ignoring all generated capture output beneath it
  - `scripts/capture-baseline.sh` — one-shot Phase 0 capture harness for baseline validation and live-host evidence
  - `plans/handoffs/base-baseline-capture-20260719.md` — this handoff
- Behavior implemented/preserved:
  - added a dedicated ignored artifact root with separate `baseline-validation/` and `live-host-evidence/` subtrees
  - added a bash harness that prefers `.venv/bin/{python,ruff,vulture}` when present and otherwise falls back to PATH tools
  - records host/runtime metadata, full pytest output, ruff output, vulture output, `bash -n` output, CLI smoke outputs, profiling output, HTML render artifacts, and optional Qt screenshots or a skip note
  - preserves production Python code, configuration, styles, packaging, and tests unchanged
- Explicitly not implemented:
  - no production code or test behavior changes
  - no inventory generation or Rust scaffolding
  - no `plans/STATUS.md` integration updates

## Parity evidence

- Current Python symbols/files covered:
  - existing CLI entrypoint `pirostats`
  - existing Qt preview helper `tools/qt_shot.py`
  - existing Phase 0 expectations in `plans/PHASES.md`
- Oracle fixtures/cases: none added in this slice
- Exact differences remaining:
  - optional Qt PNG evidence was not captured on this host because `PyQt6` is not installed in `.venv`; the harness emitted `.test-artifacts/live-host-evidence/qt-shots/qt-shots-skipped.txt` instead of failing
- Inventory entries proposed resolved: none

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `bash -n scripts/capture-baseline.sh` | pass | Requested shell syntax check passed locally before the smoke run. |
| `git diff --check` | pass | No whitespace or patch-format issues in the scoped slice. |
| `./scripts/capture-baseline.sh` | pass | Wrote `.test-artifacts/summary.txt`; all required captures passed, and Qt screenshots were skipped cleanly via `.test-artifacts/live-host-evidence/qt-shots/qt-shots-skipped.txt` because `PyQt6` is unavailable in the validated `.venv`. |
| `git status --short` | pass | Final tracked diff is limited to `.test-artifacts/`, `plans/handoffs/base-baseline-capture-20260719.md`, and `scripts/capture-baseline.sh`; the smoke run created no extra tracked artifact noise. |

## Dependencies and safety

- New/changed dependencies and review:
  - none; the harness only reuses the current Python/Qt tooling already present in the checkout environment
- Native/build/runtime requirements:
  - requires `bash`
  - requires Python via `.venv/bin/python` or `python3`
  - uses `ruff`, `vulture`, and PyQt6 opportunistically and records skips instead of crashing when optional tooling is absent
- Unsafe/FFI locations and invariants: none touched

## Risks/blockers

- Known risks:
  - live-host artifacts such as `probe`, `profiling`, rendered HTML, and screenshots can contain machine-specific or private data; they remain under ignored local artifact storage and should be reviewed/sanitized before sharing
  - if optional tooling is missing, the harness records a skip artifact instead of proving that check on that machine
- Blocker requiring integration decision:
  - none currently
- Suggested next lane/API change:
  - have the integration owner review the artifact layout and summary output, then decide whether any P0.7 redaction rules should be added before sharing evidence bundles externally

## Review notes

- Diff inspected for out-of-scope paths: yes
- Production runtime untouched by tests: yes
- No skipped/weakened checks: no — optional Qt screenshots were skipped intentionally and documented because `PyQt6` is absent on this host
- Rebase required before merge: no