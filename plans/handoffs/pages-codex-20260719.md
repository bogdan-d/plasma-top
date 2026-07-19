# Handoff: `PAGES` / `Codex recovery`

## Contract

- Objective: port page registry, command bodies/cache, connections formatting,
  title/pager/default click, and CPU-core/process/graphs tooltip page HTML.
- Integration base SHA: `e1f27a3`.
- Branch/worktree: `rust-migration-base-bootstrap` / repository root.
- Owned paths: `rust/src/page_commands.rs`, `rust/src/render/pages.rs`, focused
  tests in those modules.
- Forbidden/shared paths: composition roots and frozen boundary/fixture APIs
  integration-owner only.
- Dependencies verified integrated: DOMAIN, CONFIG, FIXTURES, RENDER-CORE,
  TRACES, FORMATTER, CHART.

## Result

- Status: `handoff`.
- Commits: integration commit created after verification.
- Changed files: page modules above; integration-owned module exports,
  `domain/boundary.rs`, command fake/re-export, network timeout assertions,
  `plans/{STATUS,INVENTORY}.md`.
- Behavior implemented/preserved: full/deep-dive registry; unknown-id skip;
  `ss -4tlnp`; fastfetch argv + `script -qec` PTY fallback; 5-second page
  command timeout; ANSI cleanup; stdout/stderr/no-output/error handling;
  30-second fastfetch cache; connection process/service resolution and aligned
  HTML; title/pager/default click; exact CPU-core/process page HTML; graphs page
  CPU/memory/NVIDIA-or-Intel/network composition.
- Explicitly not implemented: detached click launch and daemon page dispatch
  (DAEMON-CLI); process collection (PROCESS); production subprocess adapter
  (COLLECTOR/DAEMON-CLI).

## Parity evidence

- Current Python symbols/files covered: all symbols in `src/pages.py`; page-only
  `PanelFormatter` methods `_wrap_tooltip`, `format_page`, `format_cpu_cores`,
  `format_top_process`, `_graph_val`, `_gpu_graph`, `format_graphs`.
- Oracle fixtures/cases: fixed Python bytes for title/pager, command text shell,
  connections HTML+width, CPU-core HTML, process HTML; CHART's decoded-pixel
  corpus underlies graphs-page image composition.
- Exact differences remaining: none in deterministic page text/HTML tested here.
  PNG compression parity retains CHART's already-verified decoded-pixel policy.
- Inventory entries proposed resolved: `src/pages.py` callable family and page
  formatter methods; corresponding Rust callable sections added.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo fmt --manifest-path rust/Cargo.toml -- --check` | pass | no diff |
| `cargo check --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | all targets |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | no warnings |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 333 lib + 23 integration tests |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps` | pass | no warnings |
| `.venv/bin/python -m pytest tests/test_formatter.py tests/test_golden_render.py -q` | pass | 61 passed |

## Dependencies and safety

- New/changed dependencies and review: none.
- Native/build/runtime requirements: unchanged.
- Unsafe/FFI locations and invariants: none; crate-level `deny(unsafe_code)`
  remains effective.
- Shared contract repair: `CommandRunner::run` now carries `Duration`; fake
  traces program/argv/timeout and can queue adapter failures. Existing network
  fake call sites assert Python's 3-second timeout; pages assert 5 seconds.

## Risks/blockers

- Known risks: production PATH lookup/subprocess execution and detached click
  launch arrive with DAEMON-CLI; fixture tests spawn no child processes.
- Blocker requiring integration decision: none.
- Suggested next lane/API change: PROCESS, POWER, or HID remain ready.

## Review notes

- Diff inspected for out-of-scope paths: yes; shared edits are integration-owned
  timeout-contract repair only.
- Production runtime untouched by tests: yes.
- No skipped/weakened checks: yes.
- Rebase required before merge: no.
