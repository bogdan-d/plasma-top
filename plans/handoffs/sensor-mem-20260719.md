# Handoff: `SENSOR-MEM` / `sensor-mem-20260719`

## Contract

- Objective: Port the memory-owned pieces of `src/sensors.py` with deterministic proc roots and clock-driven history updates.
- Integration base SHA: `b1578e6a658259370c27c373d6d3434a5be0b34d`.
- Branch/worktree: `rust-migration-base-bootstrap` @
  `/var/mnt/xdata/code/_self/plasma-stats`.
- Owned paths: `rust/src/sensors/{mod,memory}.rs` and memory fixture/tests.
- Shared paths reviewed by integration owner: `plans/{INVENTORY,STATUS}.md`.
- Dependencies verified integrated: CONFIG defaults/history lengths and FIXTURES'
  deterministic proc-root + clock conventions.

## Result

- Status: `handoff`.
- Commits: final integration commit created after handoff drafting; see branch history.
- Changed files:
  - `rust/src/sensors/mod.rs` — registers the new memory sensor module.
  - `rust/src/sensors/memory.rs` — deterministic `/proc/meminfo` RAM/swap readers,
    psutil-style available-memory fallback, total-memory helper, and focused tests.
  - `plans/INVENTORY.md` — marks the Python memory callables resolved and records
    the Rust `memory.rs` file/callable inventory.
  - `plans/STATUS.md` — promotes `SENSOR-MEM` to verified and updates the Wave 3 milestone.
  - `plans/handoffs/sensor-mem-20260719.md` — this evidence file.
- Behavior implemented/preserved:
  - RAM usage from `/proc/meminfo` with `used = total - available`, matching
    `psutil.virtual_memory()` semantics instead of the old `free + cached` shortcut;
  - `MemAvailable:` direct path, plus procps-style fallback when `MemAvailable`
    is missing or zero, including `zoneinfo` low-watermark handling;
  - Python-matching clamps for negative available memory and container-broken
    `available > total` cases (`free` fallback);
  - half-even rounded GiB tooltip values (`round()` parity) and one-decimal-percent
    truncation parity for the visible integer percentages;
  - bounded shared memory history keyed off `display.history_interval` and the
    longest configured spark/braille/graphs consumer;
  - deterministic Rust counterpart to `_mem_total_bytes` for later PROCESS lane reuse;
  - swap-total-zero → `None` (no swap row), matching current Python behavior.
- Explicitly not implemented:
  - Wave 5 collector wiring into the shared daemon state/readings model;
  - PROCESS-lane reuse of total-RAM bytes for per-process memory percentages.

## Parity evidence

- Current Python symbols/files covered: `src/sensors.py::_mem_total_bytes`,
  `src/sensors.py::_read_mem_usage`, and `src/sensors.py::_read_swap_usage`.
- Oracle fixtures/cases:
  - direct `MemAvailable:` path;
  - missing `MemAvailable:` with `zoneinfo` fallback;
  - zero `MemAvailable:` with fallback;
  - `available > total` clamp back to `free`;
  - fallback to `free + cached` when the extra procps inputs are unavailable;
  - malformed/missing `meminfo`;
  - swap absent (`SwapTotal == 0`) and present;
  - half-even rounding ties for GiB and percentages.
- Exact differences remaining: none in lane scope.
- Inventory entries proposed resolved: `rust/src/sensors/memory.rs` file row and
  the Python callables `_mem_total_bytes`, `_read_mem_usage`, `_read_swap_usage`.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | pass | No formatting drift. |
| `cargo check --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | All targets compile. |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | Current stable toolchain; no warnings. |
| `cargo test --manifest-path rust/Cargo.toml memory --all-targets --all-features` | pass | 12 focused memory tests. |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 253 library + 23 integration tests (276 total). |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps --all-features` | pass | Public memory sensor API documented. |
| `PYTHONPATH=src .venv/bin/python -m pytest tests/test_sensors.py -v` | pass | Python sensor baseline 4/4. |
| `.venv/bin/python` synthetic `psutil.PROCFS_PATH` checks | pass | Direct oracle confirmation for the two procps-fallback fixture cases used by Rust tests. |

## Dependencies and safety

- New/changed dependencies: none; `Cargo.toml` and `Cargo.lock` unchanged.
- Native/build/runtime requirements: none added.
- Unsafe/FFI locations and invariants: none; crate-level `deny(unsafe_code)` remains effective for production code.

## Risks/blockers

- Known risks: PROCESS will still need to decide whether it reuses `read_mem_total_bytes`
  directly or caches total RAM bytes at a higher orchestration layer for its top-process memory percentages.
- Blocker requiring integration decision: none for `SENSOR-NET`/`SENSOR-DISK`.
- Suggested next lane/API change: start `SENSOR-NET`, then let COLLECTOR merge the CPU+memory state slices once the remaining Phase 3 sensor lanes land.

## Review notes

- Diff inspected for out-of-scope paths: yes.
- Production runtime untouched by tests: yes.
- No skipped/weakened checks: yes.
- Rebase required before merge: no.