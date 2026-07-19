# Handoff: `RENDER-CORE` / `render-core-20260719`

## Contract

- Objective: Port the Python render model and five-plan table-free monospace
  serializer with exact deterministic HTML compatibility.
- Integration base SHA: `3f84ce9f2b3ad5d6da823c630c845f0164101421`.
- Branch/worktree: `rust-migration-base-bootstrap` @
  `/var/mnt/xdata/code/_self/plasma-stats`.
- Owned paths: `rust/src/render/{mod,model,mono}.rs` and focused tests.
- Shared paths reviewed by integration owner: `rust/src/lib.rs`,
  `plans/{INVENTORY,STATUS}.md`.
- Dependencies verified integrated: DOMAIN identifiers and CONFIG contracts.

## Result

- Status: `handoff`.
- Commits: none; working-tree implementation.
- Changed files:
  - `rust/src/render/model.rs` — typed alignment/separator states, cells, rows,
    blocks, CSS identities, thresholds, grouping, builders, visible width, and
    horizontal inline serialization.
  - `rust/src/render/mono.rs` — column measurement, all five layout plans,
    global right edge, explicit separator rules, and table-free serialization.
  - `rust/src/render/mod.rs` and `rust/src/lib.rs` — composition/re-exports.
  - `rust/src/domain/registry.rs` — removes two redundant test-only
    `.into_iter()` calls required by current stable Clippy.
  - `plans/INVENTORY.md` and `plans/STATUS.md` — verified evidence ledger.
- Behavior implemented/preserved:
  - `left`, `rightval`, `centermid`, `twopair`, and `titlerule` plans;
  - per-block column widths and one surface-wide right edge;
  - structural padding, minimum inline widths, small-font layout widths;
  - role-shape grouping and explicit small/big separator semantics;
  - HTML tag stripping/entity width, nested spans, and no `<table>` output.
- Explicitly not implemented: traces, formatter registry, complete page HTML,
  or sensor APIs.

## Parity evidence

- Current Python symbols/files covered: every callable/class in
  `src/render_model.py` and `src/mono_render.py`.
- Existing assertions mapped: all cases in `tests/test_render_model.py` and
  `tests/test_mono_render.py`; formatter helper assertions remain green.
- Exact corpus: one fixed Python-produced HTML byte string exercises every
  layout plan, explicit separator, nested trace span, layout-width override,
  and a 24-column floor.
- Additional boundaries: 80 label/value-width combinations prove shared right
  edges; threshold boundaries, Unicode glyphs, numeric entities, empty extras,
  and leading/trailing separators are covered.
- Inventory entries resolved: all RENDER-CORE production callables and preserved
  Python test callables.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo fmt --all -- --check` | pass | No formatting drift. |
| `cargo check --all-targets --all-features` | pass | All targets compile. |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass | Current stable toolchain; no warnings. |
| `cargo test --all-targets --all-features` | pass | 215 library + 23 integration tests; 21 focused render tests. |
| `cargo doc --no-deps --all-features` | pass | Public render API documented. |
| `.venv/bin/python -m pytest tests/test_render_model.py tests/test_mono_render.py tests/test_formatter.py -v` | pass | 90/90 Python oracle assertions. |
| `.venv/bin/ruff check .` | pass | Python lint remains green. |
| `.venv/bin/vulture src/ tests/ pirostats tests/vulture_whitelist.py --min-confidence 60` | pass | Python dead-code gate remains green. |

## Dependencies and safety

- New/changed dependencies: none; `Cargo.toml` and `Cargo.lock` unchanged.
- Native/build/runtime requirements: none added.
- Unsafe/FFI locations and invariants: none; crate-level `deny(unsafe_code)`
  remains effective for production code.

## Risks/blockers

- Known risk: complete formatter/golden parity remains Wave 4 work; this lane
  proves primitives rather than full pages.
- Blocker requiring integration decision: none for TRACES.
- Suggested next lane/API change: implement `rust/src/render/traces.rs` against
  the verified model API. Sensor lanes still need shared readings/state types.

## Review notes

- Diff inspected for out-of-scope paths: yes.
- Production runtime untouched by tests: yes.
- No skipped/weakened checks: yes.
- Rebase required before merge: no.
