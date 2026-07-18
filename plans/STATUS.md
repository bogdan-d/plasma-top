# Integration status

Only integration owner edits this file. Lane agents write under `handoffs/`.

## Baseline

- Planning base: `fa2e093` (`Add Rust skills and rules for safe coding practices`).
- Working tree was clean at that commit before this `plans/` handoff package was
  created. No pre-existing user changes or exclusions need preservation.
- Expected planning delta: only `plans/` until these documents are committed.
- Local baseline validation now succeeds in an isolated `.venv` created from
  `requirements-dev.txt`: pytest, ruff, vulture, and `psutil` were installed and
  verified against the current Python implementation.

## Decisions

| ID | Decision | Status |
|---|---|---|
| D001 | Replace Python backend as one Rust binary; keep QML/assets | accepted plan assumption |
| D002 | Keep Python as test oracle until Phase 8; no Python/Rust FFI | accepted plan assumption |
| D003 | Synchronous single-crate Rust architecture | accepted plan assumption |
| D004 | Exact observable parity precedes improvements | accepted plan assumption |

## Lane ledger

| Lane | Phase | Status | Owner | Base/commit | Handoff | Verified checks/blocker |
|---|---:|---|---|---|---|---|
| BASE | 0 | active | GitHub Copilot | rust-migration-base-bootstrap | `plans/handoffs/base-copilot-20260719.md`; `plans/handoffs/base-oracle-20260719.md`; `plans/handoffs/base-baseline-capture-20260719.md`; `plans/handoffs/base-inventory-20260719.md`; `plans/handoffs/base-inventory-gate-20260719.md` | P0.2 verified (`.venv`, pytest, ruff, vulture, CLI smokes); starter P0.4/P0.5 render oracle verified; P0.1/P0.3 baseline-capture harness verified on current host; P0.6 AST inventory generator and markdown gate verified; remaining BASE work is broader P0.7 multi-host/live-feature coverage and any residual inventory/detail closure |
| SCAFFOLD | 1 | blocked | — | — | — | waits BASE |
| DOMAIN | 2 | blocked | — | — | — | waits SCAFFOLD |
| CONFIG | 2 | blocked | — | — | — | waits SCAFFOLD/FIXTURES |
| RUNTIME | 2 | blocked | — | — | — | waits SCAFFOLD/FIXTURES |
| FIXTURES | 2 | blocked | — | — | — | waits BASE/SCAFFOLD |
| RENDER-CORE | 3 | blocked | — | — | — | waits DOMAIN |
| TRACES | 3 | blocked | — | — | — | waits RENDER-CORE/CONFIG |
| SENSOR-CPU | 3 | blocked | — | — | — | waits FIXTURES |
| SENSOR-MEM | 3 | blocked | — | — | — | waits FIXTURES |
| SENSOR-NET | 3 | blocked | — | — | — | waits FIXTURES |
| SENSOR-DISK | 3 | blocked | — | — | — | waits FIXTURES |
| FORMATTER | 4 | blocked | — | — | — | waits render foundations |
| CHART | 4 | blocked | — | — | — | waits FIXTURES |
| PAGES | 4 | blocked | — | — | — | waits RENDER-CORE/FIXTURES |
| PROCESS | 4 | blocked | — | — | — | waits SENSOR-CPU/FIXTURES |
| POWER | 4 | blocked | — | — | — | waits SENSOR-DISK/FIXTURES |
| GPU | 4 | blocked | — | — | — | waits PROCESS/FIXTURES |
| HID | 4 | blocked | — | — | — | waits SCAFFOLD/FIXTURES |
| NOTIFY | 4 | blocked | — | — | — | waits CONFIG/FIXTURES |
| COLLECTOR | 5 | blocked | — | — | — | waits all sensor lanes |
| DAEMON-CLI | 5 | blocked | — | — | — | waits backend integration |
| QML-VERIFY | 6 | blocked | — | — | — | waits DAEMON-CLI |
| PACKAGING | 6 | blocked | — | — | — | waits DAEMON-CLI |
| HARDWARE-* | 7 | blocked | — | — | — | waits signed candidate |
| CUTOVER | 8 | blocked | — | — | — | waits all gates |

## Accepted deviations

None. Add rows with user approval, affected contract/tests, and rollback.

## Current blockers

1. No CI currently verifies baseline checks.
2. Phase 0 still lacks broader multi-host live-hardware evidence for P0.7 (current-host capture exists; GPU/battery/HID/Qt-shot coverage remains incomplete).
3. Phase 0 still lacks final BASE closure review for any newly added files that may need explicit markdown inventory/disposition detail beyond the call-edge gate.
