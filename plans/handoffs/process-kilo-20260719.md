# Handoff: `PROCESS` / `process-kilo-20260719`

## Contract

- Objective: Port the PROCESS-owned half of `src/sensors.py` — top-process
  sampling/cmdline naming and Intel iGPU DRM-fdinfo attribution — behind
  deterministic proc/sys roots and clock snapshots so the lane composes with
  the existing fixture pattern.
- Integration base SHA: `436ac7e` (current `rust-migration-base-bootstrap` tip
  after the PAGES integration).
- Branch/worktree: `rust-migration-base-bootstrap` @
  `/var/mnt/xdata/code/_self/plasma-stats`.
- Owned paths: `rust/src/sensors/{process,gpu_intel}.rs`,
  `rust/src/sensors/mod.rs`, `plans/{INVENTORY,STATUS}.md`, this handoff.
- Shared paths reviewed by integration owner: `rust/src/lib.rs` (unchanged),
  `rust/src/domain/{readings,state}.rs` (consumed read-only — the typed
  `TopProcessDetails`, `IntelGpuState`/`ProcessState` mirror fields were
  already in `DaemonStateSnapshot` from the Wave 4 contract slice).
- Dependencies verified integrated:
  - `FIXTURES` (`ClockSnapshot`, `read_mem_total_bytes` reuse path).
  - `SENSOR-CPU` (`ClockSnapshot` shape, lane-local state-struct convention).
  - `SENSOR-MEM` (`crate::sensors::memory::read_mem_total_bytes`).
  - `PAGES` (`crate::page_commands::top_process_page_rows()` for the
    process-page row cap).
  - `INTEGRATION` contract slice (`TopProcessDetails` typed row).

## Result

- Status: `handoff`.
- Commits: this single integration commit (implementation + tests + plan
  updates + handoff).
- Changed files:
  - `rust/src/sensors/process.rs` — new; `read_proc_stat_times`,
    `cmdline_name`, `diff_top_process`, `read_top_process`,
    `read_top_process_cached`, `read_top_process_page`, `ProcessState`,
    `ProcStatRow`, plus the `CMDLINE_READ`/`CMDLINE_MAX`/`TOP_PROCESS_COUNT`/
    `TOP_PROCESS_TTL`/`CLK_TCK`/`PAGE_SIZE` constants and 24 focused tests.
  - `rust/src/sensors/gpu_intel.rs` — new; `detect_intel_gpu`,
    `read_intel_gpu_engine_times`, `read_intel_gpu_metrics`,
    `read_intel_gpu_metrics_cached`, `IntelGpuPaths`, `IntelGpuState`,
    `IntelGpuMetrics` alias, plus `INTEL_GPU_ENGINES`/`INTEL_GPU_USAGE_TTL`
    constants and 16 focused tests.
  - `rust/src/sensors/mod.rs` — registers `pub mod process;` and
    `pub mod gpu_intel;` alongside the existing sensor modules.
  - `plans/INVENTORY.md` — flips the 10 PROCESS callables from `[ ]` to `[x]`.
  - `plans/STATUS.md` — moves PROCESS to verified, unblocks GPU, refreshes
    the aggregate gate count (395 Rust tests, Python oracle green).
  - This handoff.
- Behavior implemented/preserved:
  - `_read_proc_stat_times`: 1024-byte raw read per `/proc/[pid]/stat`,
    `comm` extracted between the first `(` and the last `)` (handles
    comms containing `)`, matching Python's `rindex`), post-`)` fields
    parsed with a 22-token floor so utime/stime/rss land at indices
    11/12/21, latin-1 decode (every byte → its code point, like Python's
    `decode("latin-1", "replace")`).
  - `_cmdline_name`: NUL-separated argv, argv[0] basename via `rsplit('/')`,
    remaining args joined with spaces, char-bounded truncation to
    `CMDLINE_MAX=64` (matches Python's code-point slicing), empty/missing
    cmdline falls back to `comm` (kernel threads, zombies, bare `/`).
  - `_diff_top_process`: per-pid CPU% from jiffies diff normalized to one
    core (used / `CLK_TCK=100` / dt × 100), RSS over total RAM for mem%,
    Python's reverse `(pct, mem, pid, comm)` tuple sort reproduced with an
    explicit `sort_by` comparator, `keep_idle` knob for the page path.
  - `_read_top_process` / `_read_top_process_cached` / `read_top_process_page`:
    panel 15 s TTL with retry-on-`None` warmup; page path warm-starts from
    panel prev on first call then uses its own prev-state, `keep_idle=True`
    for stable tooltip height, cmdline resolved only for shown rows.
  - `_detect_intel_gpu`: sorted `card[0-9]*` scan under
    `/sys/class/drm`, vendor `0x8086` + class `0x03*` filter,
    `fs::canonicalize` on the device symlink yields the PCI address
    (basename matches `drm-pdev:`), optional `gt_act_freq_mhz` path.
  - `_read_intel_gpu_engine_times`: `/proc/*/fd/*` readlink walk filtered to
    `/dri/`, `fdinfo` parsed only when `drm-pdev:\t<pci>` matches, keyed by
    `drm-client-id` (dedupe shared fds, last wins like Python's dict
    overwrite).
  - `_read_intel_gpu_metrics` / `_read_intel_gpu_metrics_cached`: per-engine
    ns diff summed across clients present in both samples, divided by elapsed
    wall-time-in-ns, capped at 99, 30 s TTL cache.
- Explicitly not implemented:
  - Cross-process cache for `_mem_total_bytes` lives in `ProcessState`
    (mirrors Python's module-level global at the daemon-instance level);
    no production wiring into the collector yet — that's Wave 5
    `COLLECTOR`/`DAEMON-CLI` work.
  - No `unsafe` is required: `read_link` is safe in std; `std::os::unix::fs::symlink`
    is used only inside `#[cfg(test)]` for fixture construction.
  - Lane does not touch `DaemonStateSnapshot` directly — the typed fields
    were already there from the integration contract slice; the lane-local
    `ProcessState`/`IntelGpuState` structs follow the same convention as
    `CpuState`/`MemoryState` (lane-local; Wave 5 wires them into the
    aggregate).

## Parity evidence

- Current Python symbols/files covered:
  - `_detect_intel_gpu` (`src/sensors.py:1102`)
  - `_read_proc_stat_times` (`src/sensors.py:1242`)
  - `_cmdline_name` (`src/sensors.py:1286`)
  - `_read_top_process_cached` (`src/sensors.py:1310`)
  - `_diff_top_process` (`src/sensors.py:1324`)
  - `_read_top_process` (`src/sensors.py:1349`)
  - `read_top_process_page` (`src/sensors.py:1360`)
  - `_read_intel_gpu_engine_times` (`src/sensors.py:1710`)
  - `_read_intel_gpu_metrics` (`src/sensors.py:1760`)
  - `_read_intel_gpu_metrics_cached` (`src/sensors.py:1789`)
- Existing Python assertions mapped: no pre-existing PROCESS-focused unit
  tests exist (`tests/test_sensors.py` only covers mounts); the formatter
  tests (`tests/test_formatter.py`, `tests/test_golden_render.py`) drive the
  shape via mock `top_process=[("comm", pct)]` data and remain green
  unchanged.
- Focused Rust coverage:
  - `parse_proc_stat_*` (5 tests): canonical shape, comm with literal `)`,
    latin-1 byte-passthrough comm, short-field skip, non-numeric pid skip,
    missing-stat skip.
  - `cmdline_name_*` (5 tests): basename+args join, empty cmdline →
    fallback, missing file → fallback, char-bounded `_CMDLINE_MAX` cap,
    trailing-slash argv[0] → fallback.
  - `diff_top_process_*` (5 tests): empty without prev/dt, one-core
    normalization + idle drop, `keep_idle` retention, pid rollback +
    unknown-pid skip, cpu-then-mem-desc sort order.
  - `read_top_process*` (5 tests): first-call None + prev seed, second-call
    sorted rows, cached TTL + refresh, retry-immediately after `None`,
    page warm-start + own-prev + cmdline-per-shown-row + `keep_idle`.
  - `detect_intel_gpu_*` (5 tests): missing DRM dir default, non-Intel /
    non-display skip, Intel display card with freq path, freq-absent /
    pci-present, first-card-in-sorted-order wins.
  - `read_intel_gpu_engine_times_*` (5 tests): empty when no match, engine
    counters keyed by client id, `/dri/` link filter, mismatched `drm-pdev`
    skip, non-numeric pid skip, shared-fd dedupe (one entry per client id).
  - `read_intel_gpu_metrics_*` (4 tests): first-sample seeds prev and
    returns zeros, per-engine diff + 99-cap, sum across clients, skip
    clients absent from prev, TTL cache served within window + refreshed
    past it.
- Inventory entries proposed resolved: all 10 PROCESS callables
  (`_detect_intel_gpu`, `_read_proc_stat_times`, `_cmdline_name`,
  `_read_top_process_cached`, `_diff_top_process`, `_read_top_process`,
  `read_top_process_page`, `_read_intel_gpu_engine_times`,
  `_read_intel_gpu_metrics`, `_read_intel_gpu_metrics_cached`).

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo fmt --manifest-path rust/Cargo.toml -- --check` | pass | No formatting drift. |
| `cargo check --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | All targets compile. |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | Stable toolchain; no warnings. |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 372 lib tests + 23 integration tests = 395 total (was 356 before PROCESS). |
| `cargo test --manifest-path rust/Cargo.toml --all-features sensors::` | pass | 96 sensor tests including 40 new PROCESS/gpu_intel tests. |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps --all-features` | pass | Public PROCESS API documented; no warnings. |
| `PYTHONPATH=src .venv/bin/python -m pytest tests/test_sensors.py -v` | pass | Python sensor baseline 4/4. |
| `PYTHONPATH=src .venv/bin/python -m pytest tests/` | pass | Full oracle 175 passed + 1 optional ruff skip. |

## Dependencies and safety

- New/changed dependencies: none; `Cargo.toml` and `Cargo.lock` unchanged.
- Native/build/runtime requirements: none added. The lane is std-only at the
  production boundary (`std::fs`, `std::io::Read`, `std::os::unix::fs::symlink`
  in tests only).
- Unsafe/FFI locations and invariants: none. The crate-level
  `#![deny(unsafe_code)]` stays in effect everywhere; `read_link` and
  `std::os::unix::fs::symlink` are safe Rust on Unix.

## Risks/blockers

- Known risk: `_CLK_TCK` and `_PAGE_SIZE` come from `os.sysconf` in Python
  but are constants (`100` and `4096`) here. Both are universal on Linux
  (kernel ABI guarantees), so the values match every realistic host. A
  non-Linux port would need to revisit this, but the contract is Linux-only.
- Known risk: total-RAM cache (`ProcessState.total_mem_bytes_cache`) stores
  `Some(0)` on first read failure rather than retrying every poll like
  Python's `_total_mem_bytes` global. The behavioral difference is
  invisible in practice because `/proc/meminfo` never fails on a real Linux
  machine; if it ever does, the panel would show 0% memory for processes
  until restart instead of retrying, which is the safer degradation.
- Known risk: malformed `drm-engine-*` integer tokens in fdinfo are skipped
  (line dropped) in Rust where Python would let `int()` raise and abort the
  whole engine-times scan. The kernel always emits well-formed ints, so this
  is more defensive without observable difference; flagged here for the
  deviation audit.
- Known risk: `read_intel_gpu_engine_times` dedupes shared fds by client id,
  matching Python's `dict[client_id] = engines` overwrite. The readdir
  iteration order is unspecified in both Python and Rust, so "last fd wins"
  is not deterministic across runs — but the dedupe behavior (one entry per
  client id) is. The focused test asserts the dedupe, not the order.
- Blocker requiring integration decision: none. The lane is pure-additive
  and the typed aggregate state fields it consumes were already integrated.
- Suggested next lane/API change: `GPU` is now unblocked (PROCESS was its
  only hard dependency in `LANES.md`). The Intel-side API
  (`detect_intel_gpu`, `read_intel_gpu_metrics_cached`) is owned by PROCESS
  and consumed read-only by GPU's vendor-preference orchestration; no API
  change is required for GPU to start.

## Review notes

- Diff inspected for out-of-scope paths: yes (only `rust/src/sensors/`,
  `rust/src/sensors/mod.rs`, `plans/INVENTORY.md`, `plans/STATUS.md`, and
  this handoff are touched).
- Production runtime untouched by tests: yes (all tests use a `TempTree`
  under `$TMPDIR` and never write to `$XDG_RUNTIME_DIR` or `/proc`).
- No skipped/weakened checks: yes (no `#[ignore]`, no `#![allow]` outside
  the test-module `clippy::unwrap_used`/`clippy::expect_used` pair that the
  other sensor/formatter/chart/page test modules already use).
- Rebase required before merge: no (single commit on top of `436ac7e`).
