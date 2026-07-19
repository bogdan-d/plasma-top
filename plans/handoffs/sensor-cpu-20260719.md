# Handoff: `SENSOR-CPU` / `sensor-cpu-20260719`

## Contract

- Objective: Port the CPU-owned discovery and reading pieces of `src/sensors.py`
  with deterministic proc/sys roots and clock-driven history updates.
- Integration base SHA: `2524ce069df67e365bd0b76cbba5ed94b0b28e61`.
- Branch/worktree: `rust-migration-base-bootstrap` @
  `/var/mnt/xdata/code/_self/plasma-stats`.
- Owned paths: `rust/src/sensors/{mod,cpu}.rs` and CPU fixture/tests.
- Shared paths reviewed by integration owner: `rust/src/lib.rs`,
  `plans/{INVENTORY,STATUS}.md`.
- Dependencies verified integrated: CONFIG defaults/length knobs and FIXTURES'
  deterministic clock + fixture-root conventions.

## Result

- Status: `handoff`.
- Commits: none; working-tree implementation.
- Changed files:
  - `rust/src/sensors/mod.rs` — sensor module registration.
  - `rust/src/sensors/cpu.rs` — CPU discovery, aggregate/per-core usage
    readers, uptime/loadavg, frequency fallback, turbo read, and focused tests.
  - `rust/src/lib.rs` — exports the new `sensors` module.
  - `rust/tests/fixtures/proc/{uptime,loadavg,cpuinfo}` — shared procfs
    fixture inputs for uptime/load/frequency fallback tests.
  - `rust/tests/fixtures/sys/devices/system/cpu/{cpu0/cpufreq/scaling_cur_freq,intel_pstate/no_turbo}`
    — shared sysfs fixture inputs for frequency/turbo tests.
  - `plans/INVENTORY.md` and `plans/STATUS.md` — verified evidence ledger.
- Behavior implemented/preserved:
  - CPU temperature discovery via hwmon with manual override precedence;
  - `cpu0/scaling_cur_freq` discovery and turbo/boost support detection;
  - aggregate CPU usage diff from `/proc/stat`, capped at 99 like Python;
  - per-core CPU usage and per-core history reset when the core count changes;
  - shared history cadence driven by monotonic time, not sleeps;
  - uptime from `/proc/uptime` and load average from `/proc/loadavg`;
  - CPU frequency sysfs fast path with `/proc/cpuinfo` fallback;
  - `intel_pstate/no_turbo` inversion and `cpufreq/boost` fallback semantics.

## Parity evidence

- Current Python symbols/files covered: the CPU-owned callables in
  `src/sensors.py` — `_find_cpu_temp`, `_find_cpu_freq_path`,
  `_detect_cpu_turbo_supported`, `_read_cpu_usage`, `_read_cpu_cores`,
  `_read_uptime`, `_read_load_avg`, `_read_cpu_freq`, and `_read_cpu_turbo`.
- Existing assertions mapped: no pre-existing Python CPU sensor unit file exists;
  focused Rust tests cover the formula/fixture matrix and the existing Python
  `tests/test_sensors.py` baseline remains green unchanged.
- Exact corpus: shared proc/sys fixture files for `stat`, `uptime`, `loadavg`,
  `cpuinfo`, `scaling_cur_freq`, and `no_turbo`, plus temp-tree cases for core
  count changes, malformed files, override precedence, and fallback branches.
- Additional boundaries: first sample, delta sample, reset/overflow-like
  counter rollback, skipped history until cadence elapses, graph-history trim,
  malformed proc/sys files, and turbo malformed-content behavior.
- Inventory entries resolved: all `SENSOR-CPU` production callables currently
  assigned in `src/sensors.py`.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo fmt --all -- --check` | pass | No formatting drift. |
| `cargo check --all-targets --all-features` | pass | All targets compile. |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass | Current stable toolchain; no warnings. |
| `cargo test cpu --all-targets --all-features` | pass | 17 focused CPU tests. |
| `cargo test --all-targets --all-features` | pass | 241 library + 23 integration tests. |
| `cargo doc --no-deps --all-features` | pass | Public CPU sensor API documented. |
| `PYTHONPATH=src .venv/bin/python -m pytest tests/test_sensors.py -v` | pass | Python sensor baseline 4/4. |

## Dependencies and safety

- New/changed dependencies: none; `Cargo.toml` and `Cargo.lock` unchanged.
- Native/build/runtime requirements: none added.
- Unsafe/FFI locations and invariants: none; crate-level `deny(unsafe_code)`
  remains effective for production code.

## Risks/blockers

- Known risk: this lane stops at deterministic CPU discovery/readers. Wiring the
  results into the full `collect` pipeline and shared daemon state remains Wave 5
  COLLECTOR work, and top-process/Intel GPU paths remain owned by PROCESS.
- Blocker requiring integration decision: none for `SENSOR-MEM`/`SENSOR-NET`/
  `SENSOR-DISK`.
- Suggested next lane/API change: start `SENSOR-MEM` next, or begin shaping the
  shared readings/state structs once two or more sensor lanes need to merge.

## Review notes

- Diff inspected for out-of-scope paths: yes.
- Production runtime untouched by tests: yes.
- No skipped/weakened checks: yes.
- Rebase required before merge: no.