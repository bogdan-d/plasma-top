# Handoff: `COLLECTOR` / `collector-codex-20260719`

## Contract

- Objective: compose capability-driven hardware discovery, peripheral rescans,
  and per-poll sensor collection in Python-compatible order.
- Integration base SHA: `5c7f3f3`.
- Branch/worktree: `rust-migration-base-bootstrap` at repository root.
- Owned paths: `rust/src/sensors/mod.rs`, collector tests, and the production
  NVML construction deferred by the GPU lane.
- Shared paths reviewed by integration owner: `rust/Cargo.toml`,
  `rust/Cargo.lock`, `rust/DEPENDENCIES.md`, and the `NvmlFacade` adapter in
  `rust/src/sensors/gpu_nvidia.rs`.
- Dependencies verified integrated: all Phase 2–4 backend lanes.

## Result

- Status: `handoff`.
- Commits: this commit.
- Changed files: collector composition/tests, NVIDIA facade object-safety and
  production adapter, dependency manifest/lock/review ledger.
- Behavior implemented/preserved: static discovery; dynamic peripheral/network
  rescan; capability derivation from items, notifications, and graph pages;
  always-on CPU/memory baselines; exact collection section order; slow-first-
  paint suppression; stateful rates/caches/histories; hardware adoption on
  route changes; failure isolation; optional lazy NVML with `nvidia-smi`
  fallback; profiling timings.
- Explicitly not implemented: production command and D-Bus facade construction,
  daemon scheduling, logging, and CLI wiring (DAEMON-CLI lane).

## Parity evidence

- Current Python symbols/files covered: `src/sensors.py:timed_section`,
  `discover_hardware`, `rescan_peripherals`, `needs_periph_rescan`, `collect`,
  `_find_peripherals`, `_is_wireless`, `_detect_has_backlight`,
  `_read_brightness`, `_pynvml_handle_get`, `_read_count_file`, and
  `_read_server_file`; remaining collector-tagged helpers were already owned by
  their verified sensor modules.
- Oracle fixtures/cases: 47 collector tests with isolated proc/sys trees and
  command/D-Bus/NVML/Bolt fakes cover individual and combined capabilities,
  empty demand, ordered calls, no duplicate/unrequested calls, cache cadence,
  skip-slow, service/command/file failures, histories, and rescans.
- Exact differences remaining: none in deterministic collector behavior.
  Live NVIDIA/Intel/UPower/Bolt validation remains Phase 7 hardware evidence.
- Inventory entries proposed resolved: the COLLECTOR-tagged `src/sensors.py`
  callables listed above; production `NvmlFacade` construction.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo fmt --manifest-path rust/Cargo.toml -- --check` | pass | clean |
| `cargo check --manifest-path rust/Cargo.toml --all-targets` | pass | default feature set |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | includes `nvml` + test support |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 498 library + 23 integration tests |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps` | pass | warning-free |
| `.venv/bin/python -m pytest tests/ -q` | pass | 175 passed, 1 optional skip |
| `.venv/bin/ruff check .` | pass | clean |
| `.venv/bin/vulture src/ tests/ pirostats tests/vulture_whitelist.py --min-confidence 60` | pass | clean |

## Dependencies and safety

- New dependency: optional `nvml-wrapper 0.11` behind additive feature `nvml`;
  full license/native/transitive/packaging review is in `rust/DEPENDENCIES.md`.
- Native/build/runtime requirements: no native build or link dependency; NVML
  is loaded at runtime and absence falls back to `nvidia-smi`.
- Unsafe/FFI: no project unsafe code; `nvml-wrapper` contains the dynamic native
  boundary behind its safe API.

## Risks/blockers

- Known risks: current AMD-only host cannot provide live NVML, Intel DRM,
  UPower battery, or Bolt evidence; all paths have deterministic fakes.
- Blocker requiring integration decision: none.
- Suggested next lane/API change: DAEMON-CLI should provide production command
  and D-Bus facades, construct `ProductionNvml` when feature-enabled, and own
  `CollectorState` across polls.

## Review notes

- Diff inspected for out-of-scope paths: yes; shared integration edits listed.
- Production runtime untouched by tests: yes.
- No skipped/weakened checks: yes.
- Rebase required before merge: no.
