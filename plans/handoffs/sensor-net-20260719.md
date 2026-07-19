# Handoff: `SENSOR-NET` / `sensor-net-20260719`

## Contract

- Objective: Port the network-owned route/wifi/identity/rate pieces of
  `src/sensors.py` with deterministic sysfs roots, command injection, and
  clock-driven caching/history updates.
- Integration base SHA: `15343acfbe0089a55fcc49a55e6dcce52e614dde`.
- Branch/worktree: `rust-migration-base-bootstrap` @
  `/var/mnt/xdata/code/_self/plasma-stats`.
- Owned paths: `rust/src/sensors/{mod,network}.rs` and network command/sysfs
  fixture tests.
- Shared paths reviewed by integration owner: `plans/{INVENTORY,STATUS}.md`.
- Dependencies verified integrated: CONFIG history/graphs defaults and
  FIXTURES' deterministic `FakeClock`/`FakeCommandRunner` conventions.

## Result

- Status: `handoff`.
- Commits: final integration commit created after handoff drafting; see branch history.
- Changed files:
  - `rust/src/sensors/mod.rs` — registers the new network sensor module.
  - `rust/src/sensors/network.rs` — deterministic `ip`/`iw` route+wifi readers,
    sysfs byte-rate diffing, TTL caching, and graph-history helpers with focused tests.
  - `plans/INVENTORY.md` — marks the Python network callables resolved and adds
    the Rust `network.rs` file/callable inventory rows.
  - `plans/STATUS.md` — promotes `SENSOR-NET` to verified and narrows the Wave 3 blocker.
  - `plans/handoffs/sensor-net-20260719.md` — this evidence file.
- Behavior implemented/preserved:
  - default-route device detection via `ip route get 8.8.8.8` with fallback to
    `ip route show default`;
  - wifi-hardware discovery via `/sys/class/net/*/wireless`;
  - shared route/IP discovery from one `ip` call, plus wireless-only
    `iw dev <if> link` parsing for SSID and signal;
  - Python-matching dBm-to-percent conversion with `[-100, -50]` clamping;
  - 10-second network-info TTL caching keyed off monotonic time;
  - per-interface tx/rx byte-rate diffs from sysfs statistics, with first-sample,
    interface-switch, zero-`dt`, and counter-rollback suppression to avoid
    spurious spikes;
  - graph-page-gated bounded up/down history with Python-matching zero-fill for
    a missing side when the other side is present.
- Explicitly not implemented:
  - Wave 5 COLLECTOR wiring into the shared daemon `collect` path and hardware state;
  - live-host command adapter ownership (`CommandRunner` promotion remains deferred
    to the future production adapter / integration pass).

## Parity evidence

- Current Python symbols/files covered: `src/sensors.py::_token_after`,
  `_detect_net_device`, `_dbm_to_pct`, `_read_net_info`, `_read_net_info_cached`,
  `_detect_has_wifi`, `_sample_net_history`, and `_read_net_speed`.
- Oracle fixtures/cases:
  - `ip route get` success on wireless and wired routes;
  - fallback to `ip route show default` after a failed `route get`;
  - wireless sysfs presence detection;
  - `iw dev <if> link` SSID/signal parsing and wired-device short-circuit;
  - TTL cache hit vs refresh at the 10-second boundary;
  - first-sample net rates, interface switch reset, zero-`dt`, and counter rollback;
  - graph-history sampling cadence, trim-to-length, and graphs-disabled no-op.
- Exact differences remaining: none in lane scope.
- Inventory entries proposed resolved: `rust/src/sensors/network.rs` file row and
  the Python callables listed above.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | pass | No formatting drift. |
| `cargo check --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | All targets compile. |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | Current stable toolchain; no warnings. |
| `cargo test --manifest-path rust/Cargo.toml sensors::network --all-targets --all-features` | pass | 11 focused SENSOR-NET tests. |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 264 library + 23 integration tests (287 total). |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps --all-features` | pass | Public network sensor API documented. |
| `PYTHONPATH=src .venv/bin/python -m pytest tests/test_sensors.py -v` | pass | Python sensor baseline 4/4. |

## Dependencies and safety

- New/changed dependencies: none; `Cargo.toml` and `Cargo.lock` unchanged.
- Native/build/runtime requirements: none added.
- Unsafe/FFI locations and invariants: none; crate-level `deny(unsafe_code)` remains effective for production code.

## Risks/blockers

- Known risks: the live command runner still needs to move from the test-only
  fixture boundary into the future production adapter during Wave 5 composition.
- Blocker requiring integration decision: none for `SENSOR-DISK`.
- Suggested next lane/API change: start `SENSOR-DISK`, then let COLLECTOR merge
  the CPU/memory/network state slices once the remaining Phase 3 sensor lane lands.

## Review notes

- Diff inspected for out-of-scope paths: yes.
- Production runtime untouched by tests: yes.
- No skipped/weakened checks: yes.
- Rebase required before merge: no.