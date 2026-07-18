# Integration status

Only integration owner edits this file. Lane agents write under `handoffs/`.

## Baseline

- Planning base: `fa2e093` (`Add Rust skills and rules for safe coding practices`).
- Working tree was clean at that commit before this `plans/` handoff package was
  created. No pre-existing user changes or exclusions need preservation.
- Expected planning delta: only `plans/` until these documents are committed.
- Current local validation blockers: pytest, ruff, vulture, and psutil unavailable
  in observed shell environment.

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
| BASE | 0 | ready | — | — | — | test environment unavailable |
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

1. Reproducible Python test environment not established.
2. Runtime dependency documentation disagrees about required `psutil`.
3. No CI currently verifies baseline checks.
