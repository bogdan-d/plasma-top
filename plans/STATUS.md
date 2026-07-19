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
| BASE | 0 | active | GitHub Copilot | rust-migration-base-bootstrap | `plans/handoffs/base-copilot-20260719.md`; `plans/handoffs/base-oracle-20260719.md`; `plans/handoffs/base-baseline-capture-20260719.md`; `plans/handoffs/base-inventory-20260719.md`; `plans/handoffs/base-inventory-gate-20260719.md`; `plans/handoffs/base-inventory-detail-20260719.md`; `plans/handoffs/base-ci-20260719.md`; `plans/handoffs/base-p0.7-local-capture-20260719.md`; `plans/handoffs/base-p0.7-pages-20260719.md`; `plans/handoffs/base-p0.7-amd-host-20260719.md` | P0.2 verified (`.venv`, pytest, ruff, vulture, CLI smokes); starter P0.4/P0.5 render oracle verified; P0.1/P0.3 baseline-capture harness verified on current host; P0.6 AST inventory generator, markdown gate, and explicit ledger coverage for oracle/inventory tooling verified; baseline CI workflow added; current-host P0.7 evidence refreshed 2026-07-19 with profiling, probe, panel/tooltip/deep-dive HTML, Qt screenshots, and explicit hardware inventory: AMD Strix Halo iGPU (`1002:1586`, `amdgpu`), no Intel/NVIDIA GPU, no system battery, and no supported mouse/keyboard battery detected; remaining BASE work is external-hardware/multi-host coverage |
| SCAFFOLD | 1 | verified | — | rust-migration-base-bootstrap | `plans/handoffs/scaffold-20260719.md` | P1.1 verified (`rust/` crate, `Cargo.lock`, `rust-toolchain.toml` stable + clippy + rustfmt, MSRV 1.85); P1.2 verified (`lib.rs`/`main.rs` shells, strict lint attrs denying `unsafe_code`/`unwrap_used`/`expect_used`/`todo`/`unimplemented`, `test-support` cargo feature with `test_support.rs` skeleton owned by the `FIXTURES` lane); P1.3 verified (frozen `Form`/`Shape`/`Surface`/`SurfaceSet`/`Metric`/`MetricSpec`/`Capability`/`ItemToken` contracts plus boundary stubs for command/D-Bus/clock/filesystem/hardware/readings/state); P1.4 verified (`rust/DEPENDENCIES.md` baseline row + per-dep policy); P1.5 verified (`.github/workflows/baseline.yml` `rust-scaffold` job mirrors ARCHITECTURE.md gate: fmt/check/clippy/test/doc with `--all-features` and committed-`Cargo.lock` check); Gate P1 green locally — fmt/check/clippy(`-D warnings`)/test(26)/doc all pass; pre-existing `cargo fmt --check` diffs in form/item tests caught and fixed in-tree; freeze in effect (`Cargo.toml`/`Cargo.lock`/shared types now integration-owner paths); Phase 2 lanes unblocked |
| DOMAIN | 2 | verified | — | rust-migration-base-bootstrap | `plans/handoffs/domain-20260719.md` | P2 verified: `rust/src/domain/registry.rs` (702 LoC, 25 tests) ports the token+capability half of `src/registry.py` (`parse`/`unknown_item_names`/`misplaced_items`/`needed_capabilities`/`NOTIFY_CAPABILITY_MAP`/`graphs_page_capabilities`) plus `SEPARATOR_ITEMS` from `src/render_model.py` and `list_items`/`placement_for` for `pirostats list-items` parity; 51-row corpus matches the live Python output byte-for-byte; 51×2 misplaced matrix walks every metric × form × surface; std-only (no Cargo.toml/Cargo.lock delta); gates green — fmt/check/clippy(`-D warnings`)/test(51)/doc/Cargo.lock all pass; Python oracle (`test_items.py` + `test_oracle.py`) still 15/15; one purely-additive `domain/mod.rs` edit ratified (`pub mod registry;` + re-exports); CONFIG lane may now drop its local unknown/misplaced helpers and use DOMAIN's |
| CONFIG | 2 | verified | — | rust-migration-base-bootstrap | `plans/handoffs/config-20260719.md` | P2 verified (re-run after worktree-cleanup loss): `rust/src/config/{mod,merge,geometry,assets}.rs` (3420 LoC) port `src/config.py` (885 lines) in full — typed `Config` tree (14 leaf structs with `serde(default)` mirroring Python's `_from_dict`), `load_config` pipeline, `deep_merge_tables`, machine DMI detect, appletsrc/geom geometry pipeline with auto-fit, `apply_canonical_width`, asset roots with `PIROSTATS_CODE_ROOT` env override; ports all 50 `tests/test_config.py` cases + 3 integration tests against shipped `config/config.toml`; **`domain::registry::{unknown_item_names, misplaced_items, SEPARATOR_ITEMS}` consumed directly** (no duplicates, since this re-run branched from `cc3f71a` which already had DOMAIN integrated); adds `serde = { version = "1", features = ["derive"] }` (zero new transitive crates — `serde_core`/`serde_derive` already vendored via `toml`); production code stays `unsafe`-free and `unwrap`/`expect`-free (test mods use locally-scoped `#![allow(...)]`); gates green — fmt/clippy(`-D warnings`)/test(217 total)/doc/Cargo.lock in sync; Python oracle (`test_config.py`) still 50/50; cross-lane proposals deferred: `Error::Config` variant promotion, `PIROSTATS_CODE_ROOT` env name as contract, typed icons/labels accessors for Wave 4 FORMATTER |
| RUNTIME | 2 | verified | — | rust-migration-base-bootstrap | `plans/handoffs/runtime-20260719.md` | P2 verified: `rust/src/runtime/{mod,atomic,page}.rs` (400 LoC) port `src/runtime.py` (lazy path accessors, `ensure_dirs`) and `src/pagestate.py` (read/set/npages/step_page with flock); `write_atomic` primitive for Wave 5 daemon; 30 new tests (10 unit + 5 atomic + 11 page + 4 paths) including 32-thread concurrency test (`step_page_never_drops_a_notch_under_concurrency`, deterministic only under flock); `nix = "0.29"` (`fs`+`user`+`process`, MIT, GPL-compat) added under `[dependencies]` — first non-std dep; transitive: `libc`+`bitflags`+`cfg-if`+`cfg_aliases`(build); production code stays `unsafe`-free (`#![deny(unsafe_code)]` in effect everywhere); 9 `unsafe` blocks in tests only (env-mutation `set_var`/`remove_var` gated by per-binary `ENV_GUARD: Mutex<()>` with `// SAFETY:` comments); gates green — fmt/clippy(`-D warnings`)/test(81 total)/doc/Cargo.lock in sync; cross-lane proposals deferred to Wave 5 (`Error::Runtime` variant, `cli::PageDirection`↔`runtime::page::PageDirection` bridge, `FilesystemRoots` integration); HTML-publication tmp-name deviation from Python (PID-qualified vs fixed) flagged for Wave 5 applet-nameFilter verification |
| FIXTURES | 2 | verified | — | rust-migration-base-bootstrap | `plans/handoffs/fixtures-20260719.md` | P2 verified: `rust/src/test_support.rs` rewritten as module root + new `rust/src/test_support/{fixture_root,fake_clock,fake_command_runner,fake_dbus,fixture_loader}.rs` (1628 LoC) with concrete fakes and fixture loaders; 8 sample fixtures under `rust/tests/fixtures/{proc,sys,oracle,cmd,dbus}/` mirroring BASE schema; `rust/tests/parity_runner.sh` stub exits 77 until Wave 4 FORMATTER; 63 unit tests + 1 doctest in isolation (37 new beyond P1's 26), all gates green on integration (118 total tests after merge with DOMAIN+RUNTIME); `toml = "1"` added as a HARD dep (FIXTURES proposed `0.8` optional/feature-gated; unified to `1` hard because CONFIG lane in the same wave needs it in production — `test-support` feature keeps gating the module, not the parser); 25 public methods + 4 Default impls + 2 traits (`CommandRunner`/`DbusFacade`) + 2 enums (`FixtureError`/`RuntimeError`); cross-lane proposals deferred: promote `RuntimeError`/`FixtureError` into `error::Error` when production adapters land (Wave 4/5), move `CommandRunner`/`DbusFacade` traits to `domain::boundary` when production implementations land |
| RENDER-CORE | 3 | verified | Codex | rust-migration-base-bootstrap | `plans/handoffs/render-core-20260719.md` | P3 render foundation verified: `rust/src/render/{model,mono}.rs` ports cells/rows/blocks, thresholds, grouping, horizontal inline output, and all five table-free mono plans; 21 focused Rust tests include a fixed byte-identical Python corpus and 80-case right-edge sweep; full all-feature Rust gates green (238 tests total), focused Python oracle 90/90, ruff/vulture green; no dependency or lockfile change |
| TRACES | 3 | verified | GitHub Copilot | rust-migration-base-bootstrap | `plans/handoffs/traces-20260719.md` | P3 traces verified: `rust/src/render/traces.rs` ports `src/traces.py`'s bar/column/spark/braille encodings plus standalone/combo rows, including Python-matching half-even bar layout width and tooltip label/history wiring; 12 focused Rust tests pin fixed Python byte corpora and row structures, full all-feature Rust gates green (250 tests total), and Python formatter oracle remains green (`tests/test_formatter.py` 58/58); no dependency or lockfile change |
| SENSOR-CPU | 3 | ready | — | — | — | FIXTURES verified; ready for owner assignment |
| SENSOR-MEM | 3 | ready | — | — | — | FIXTURES verified; ready for owner assignment |
| SENSOR-NET | 3 | ready | — | — | — | FIXTURES verified; ready for owner assignment |
| SENSOR-DISK | 3 | ready | — | — | — | FIXTURES verified; ready for owner assignment |
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

## Milestones

- **Wave 2 complete** (commit `eeb80bd`): all four Phase 2 lanes (DOMAIN, CONFIG, RUNTIME, FIXTURES) verified and integrated. Gate P2 green — Rust 217 tests pass, Python 175 passed + 1 skipped. Wave 3 `RENDER-CORE` and SENSOR-CPU/MEM/NET/DISK lanes are ready; `TRACES` starts after the `RENDER-CORE` API is integrated.
- **Wave 3 rendering lanes complete**: `RENDER-CORE` and `TRACES` are verified. Gate P3 remains open pending `SENSOR-CPU`/`SENSOR-MEM`/`SENSOR-NET`/`SENSOR-DISK`.

## Accepted deviations

None. Add rows with user approval, affected contract/tests, and rollback.

## Current blockers

1. Phase 0 still lacks broader multi-host live-hardware evidence for P0.7. The current host has only an unsupported AMD Strix Halo iGPU (`1002:1586`, `amdgpu`), no Intel/NVIDIA GPU, no system battery, and no supported Bolt mouse/keyboard battery detected. Intel, NVIDIA NVML/fallback, UPower battery/peripheral, and Bolt/HID live paths require other hosts/devices; fixture coverage remains mandatory.
