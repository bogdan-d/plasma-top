# Handoff: `NOTIFY` / `notify-codex-20260719`

## Contract

- Objective: land the smallest shared notification boundary and port the full
  Python notification state machine with deterministic parity evidence.
- Integration base SHA: `2a14cdc`.
- Branch/worktree: repository root integration worktree.
- Owned paths: `rust/src/notify.rs`, notification fake/tests, NOTIFY handoff.
- Authorized integration paths: boundary/state contracts, composition/re-export
  roots, `plans/{STATUS,INVENTORY}.md`.
- Dependencies verified: CONFIG, FIXTURES, typed `HardwareSnapshot`,
  `ReadingsSnapshot`, and `DaemonStateSnapshot` were integrated at the base.

## Result

- Status: `verified` after local integration review.
- Commit: integration commit containing this handoff.
- Behavior implemented:
  - typed title/body/icon/urgency/timeout payload and explicit adapter error;
  - production/test-shared `NotificationFacade`;
  - deterministic fake with exact ordered recording and queued failures;
  - daemon-owned notification latches and Python-compatible device-state retention;
  - all ten alert types, edge-only sends, monotonic holds, hysteresis, recovery,
    exclusions, labels, and exact payloads;
  - service failures preserve state transitions, remain observable in an ordered
    report, and never stop later notification checks.
- Explicitly not implemented: desktop-service transport construction and daemon
  wiring (DAEMON-CLI); live desktop notification validation (not performed).

## Parity evidence

- Python symbols resolved: `_send`, `Latch`, `NotifState`, `_sustained`, and
  `check_and_notify`.
- Full `tests/test_notifier.py` behavior mapped: spike rejection, elapsed-time
  hold, one edge per episode, dip reset, zero hold, hysteresis, cooling/rearm,
  disable behavior, and no-hysteresis load clearing.
- Expanded Rust corpus covers CPU/GPU/disk usage/SMART/disk temperature/system
  battery/mouse/keyboard/load/server, exact ordered full payloads, threshold
  boundaries, charging and zero exclusions, independent device latches, removed
  device retention, absent readings/hardware, and facade failure.
- Exact differences remaining: none in state-machine behavior or payloads.

## Validation

| Command | Result | Notes |
|---|---|---|
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features notify` | pass | 16 focused/mapped tests; 14 direct notify tests plus related config/domain contract tests. |
| `cargo fmt --manifest-path rust/Cargo.toml -- --check` | pass | Clean. |
| `cargo check --manifest-path rust/Cargo.toml --all-targets` | pass | No-feature all-target contract compiles. |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | No warnings. |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 451 library + 23 integration = 474 tests. |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps` | pass | No warnings. |
| `.venv/bin/python -m pytest tests/ -q` | pass | 175 passed, 1 optional skip. |
| `.venv/bin/python -m pytest tests/test_inventory.py -q` | pass | 2 passed. |
| `git diff --check` | pass | Clean. |

## Dependencies and safety

- New/changed dependencies: none; Cargo manifests and lockfile unchanged.
- Native/build/runtime additions: none.
- Unsafe/FFI: none; crate-level `#![deny(unsafe_code)]` remains green.
- Tests perform no real desktop notification or D-Bus calls.

## Risks/blockers

- Known risk: production desktop-service transport and live service behavior are
  not validated in this lane; DAEMON-CLI must provide/wire the adapter and log
  returned `NotificationReport` failures.
- Blocker requiring integration decision: none.
- Suggested next lane: `COLLECTOR`; all Phase 4 backend lanes are verified.

## Review notes

- Diff inspected for out-of-scope paths: yes.
- Production runtime untouched by tests: yes.
- No skipped/weakened checks: yes.
- Rebase required: no.
