# Handoff: `FORMATTER` / `20260719`

## Contract

- Objective: port the main item-registry rendering path (`src/items.py`, formatter-owned `src/registry.py`, and main panel/tooltip portions of `src/formatter.py`) to Rust with byte-identical panel H/V and tooltip goldens, hardware gates, section collapse, and canonical-width coverage.
- Integration base SHA: `fa2e093`
- Branch/worktree: `rust-migration-base-bootstrap`
- Owned paths:
  - `rust/src/render/{cells,registry,formatter}.rs`
  - `plans/handoffs/formatter-20260719.md`
  - integration-owner verification updates in `plans/STATUS.md`
- Forbidden/shared paths:
  - `rust/Cargo.toml`, `rust/Cargo.lock`
  - Phase-5+ composition roots and non-formatter sensor/page/chart/notifier modules
- Dependencies verified integrated:
  - `DOMAIN`, `CONFIG`, `RENDER-CORE`, `TRACES`, and the typed aggregate `domain::{readings,state}` contract slice

## Result

- Status: `handoff`
- Commits: working tree prepared for a single formatter-lane commit after integration-owner verification updates
- Changed files:
  - `rust/src/render/mod.rs`
  - `rust/src/render/cells.rs`
  - `rust/src/render/registry.rs`
  - `rust/src/render/formatter.rs`
- Behavior implemented/preserved:
  - formatter-owned hardware gates matching `src/metrics.py`
  - token-to-render-form resolution for formatter dispatch, including BAR→COLUMN CSS token switching by orientation
  - main panel H/V and tooltip rendering, including section collapse, title/title-rule rows, explicit separator normalization, horizontal inline item gaps, and canonical tooltip width derivation
  - regular item rows, historied CPU/memory forms, paired disk/fan/SMART rows, dual-rate rows, battery rows, string/SSID/device joins, load/uptime/top-process item rows, and value/threshold formatting helpers
  - Rust golden coverage against `tests/golden/{panel_h,panel_v,tooltip}.html`
- Explicitly not implemented:
  - deep-dive page rendering (`PAGES` lane)
  - graph PNG generation and graphs page formatting (`CHART` lane)
  - collector/daemon/CLI integration of the new formatter (`COLLECTOR` / `DAEMON-CLI` lanes)

## Parity evidence

- Current Python symbols/files covered:
  - main panel/tooltip behavior from `src/formatter.py`
  - formatter-owned dispatch from `src/registry.py`
  - reusable row/cell composition from `src/items.py`
- Oracle fixtures/cases:
  - focused Rust formatter tests for helper behavior, section collapse, canonical-width guard, and shipped HTML goldens
  - existing Python oracle checks retained and re-run from `.venv` for `tests/test_formatter.py` and `tests/test_golden_render.py`
- Exact differences remaining: none in the main panel/tooltip formatter scope
- Inventory entries proposed resolved:
  - `rust/src/render/cells.rs`
  - `rust/src/render/registry.rs`
  - `rust/src/render/formatter.rs`
  - formatter/golden Python oracle coverage mapped to Rust formatter tests

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo test --manifest-path rust/Cargo.toml render::formatter::tests --lib` | pass | 5 focused formatter tests, including panel H/V + tooltip goldens |
| `cargo fmt --manifest-path rust/Cargo.toml -- --check` | pass | full Rust gate |
| `cargo check --manifest-path rust/Cargo.toml --all-targets` | pass | full Rust gate |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | full Rust gate |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 302 Rust tests total after formatter integration |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps` | pass | full Rust gate |
| `.venv/bin/python -m pytest tests/test_formatter.py tests/test_golden_render.py -v` | pass | 61 Python oracle tests passed |

## Dependencies and safety

- New/changed dependencies and review: none
- Native/build/runtime requirements: none beyond existing Rust/toolchain and the repo-local `.venv` for Python oracle reruns
- Unsafe/FFI locations and invariants: none added; production formatter code remains `unsafe`-free

## Risks/blockers

- Known risks:
  - `format_page`/graphs/process-page presentation remains intentionally deferred to `PAGES`/`CHART`; later lanes must preserve the now-verified main-tooltip canonical-width contract when integrating those pages.
- Blocker requiring integration decision: none
- Suggested next lane/API change:
  - `CHART` or `PAGES` can start next; both can build on `PanelFormatter::canonical_width` and the now-verified item-formatting helpers without changing formatter-owned output.

## Review notes

- Diff inspected for out-of-scope paths: yes
- Production runtime untouched by tests: yes
- No skipped/weakened checks: yes
- Rebase required before merge: no