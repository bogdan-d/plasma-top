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
| BASE | 0 | active | GitHub Copilot | rust-migration-base-bootstrap | `plans/handoffs/base-copilot-20260719.md`; `plans/handoffs/base-oracle-20260719.md`; `plans/handoffs/base-baseline-capture-20260719.md`; `plans/handoffs/base-inventory-20260719.md`; `plans/handoffs/base-inventory-gate-20260719.md`; `plans/handoffs/base-inventory-detail-20260719.md`; `plans/handoffs/base-ci-20260719.md`; `plans/handoffs/base-p0.7-local-capture-20260719.md`; `plans/handoffs/base-p0.7-pages-20260719.md` | P0.2 verified (`.venv`, pytest, ruff, vulture, CLI smokes); starter P0.4/P0.5 render oracle verified; P0.1/P0.3 baseline-capture harness verified on current host; P0.6 AST inventory generator, markdown gate, and explicit ledger coverage for oracle/inventory tooling verified; baseline CI workflow added; current-host P0.7 evidence now includes profiling, probe, panel/tooltip render artifacts, Qt screenshots, and deep-dive tooltip page HTML/PNG capture for `processes`, `cpu_cores`, `connections`, `fastfetch`, and `graphs`; remaining BASE work is external-hardware/multi-host coverage |
| SCAFFOLD | 1 | verified | — | rust-migration-base-bootstrap | `plans/handoffs/scaffold-20260719.md` | P1.1 verified (`rust/` crate, `Cargo.lock`, `rust-toolchain.toml` stable + clippy + rustfmt, MSRV 1.85); P1.2 verified (`lib.rs`/`main.rs` shells, strict lint attrs denying `unsafe_code`/`unwrap_used`/`expect_used`/`todo`/`unimplemented`, `test-support` cargo feature with `test_support.rs` skeleton owned by the `FIXTURES` lane); P1.3 verified (frozen `Form`/`Shape`/`Surface`/`SurfaceSet`/`Metric`/`MetricSpec`/`Capability`/`ItemToken` contracts plus boundary stubs for command/D-Bus/clock/filesystem/hardware/readings/state); P1.4 verified (`rust/DEPENDENCIES.md` baseline row + per-dep policy); P1.5 verified (`.github/workflows/baseline.yml` `rust-scaffold` job mirrors ARCHITECTURE.md gate: fmt/check/clippy/test/doc with `--all-features` and committed-`Cargo.lock` check); Gate P1 green locally — fmt/check/clippy(`-D warnings`)/test(26)/doc all pass; pre-existing `cargo fmt --check` diffs in form/item tests caught and fixed in-tree; freeze in effect (`Cargo.toml`/`Cargo.lock`/shared types now integration-owner paths); Phase 2 lanes unblocked |
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

1. Phase 0 still lacks broader multi-host live-hardware evidence for P0.7 (current-host capture now includes panel/tooltip plus deep-dive page HTML/Qt evidence; GPU/battery/HID/NVIDIA coverage remains machine-limited and needs other hosts/hardware).
