# Handoff: `SENSOR-DISK` / `sensor-disk-20260719`

## Contract

- Objective: Port the disk-owned mount/usage/rate/hwmon/identity pieces of
  `src/sensors.py` with deterministic proc/sys roots and monotonic cache/rate
  timing, while leaving UDisks SMART execution itself to `POWER`.
- Integration base SHA: `31e9186900194b4bad0a6aac9d28bead8cf44522`.
- Branch/worktree: `rust-migration-base-bootstrap` @
  `/var/mnt/xdata/code/_self/plasma-stats`.
- Owned paths: `rust/src/sensors/{mod,disk,hwmon}.rs` and disk/sysfs fixture
  tests.
- Shared paths reviewed by integration owner: `plans/{INVENTORY,STATUS}.md`.
- Dependencies verified integrated: `CONFIG` mount/default types,
  `FIXTURES` clock conventions, and the Wave 3 sensor module shape already used
  by `SENSOR-CPU`/`MEM`/`NET`.

## Result

- Status: `handoff`.
- Commits: final integration commit created after handoff drafting; see branch history.
- Changed files:
  - `rust/src/sensors/mod.rs` — registers the new disk and hwmon sensor modules.
  - `rust/src/sensors/hwmon.rs` — deterministic hwmon directory/spec/int helpers
    for disk-owned sensors with focused tests.
  - `rust/src/sensors/disk.rs` — mount resolution, disk usage, root-disk
    topology, hwmon disk/fan discovery with TTL caches, whole-disk identity,
    and `/proc/diskstats` byte-rate readers with focused tests.
  - `plans/INVENTORY.md` — marks the Python disk callables resolved and adds
    Rust file/callable inventory rows.
  - `plans/STATUS.md` — promotes `SENSOR-DISK` to verified, closes Gate P3, and
    marks newly unblocked downstream lanes ready.
  - `plans/handoffs/sensor-disk-20260719.md` — this evidence file.
- Behavior implemented/preserved:
  - explicit `[disks].mounts` passthrough and `auto_roots` filtering with `/`
    always first;
  - mount-table escape decoding for paths containing spaces;
  - disk temperature discovery via manual overrides first, then `nvme` and
    `drivetemp` hwmon autodetect with Python-matching NVMe/SCSI labels;
  - numbered manual fan discovery with the same first-missing-slot stop
    behavior as Python;
  - 30-second label-keyed caches for `hd_temp` and `fan_speed`;
  - mountpoint → device basename → whole-disk topology walk for byte-rate
    sampling, including mapper fallback when no single parent disk exists;
  - supported whole-disk identity enumeration (`nvme*`, `sd*`, `hd*`) with
    kernel rotational-flag classification for later SMART/power work;
  - df/psutil-style `statvfs` percent semantics plus Python-style half-even GiB
    rounding for disk usage;
  - `/proc/diskstats` read/write byte-rate diffs with first-sample,
    device-switch, zero-`dt`, and counter-rollback suppression.
- Explicitly not implemented:
  - UDisks2 SMART D-Bus calls and cache TTL selection (`POWER` lane);
  - Wave 5 COLLECTOR wiring into shared hardware/readings state.

## Parity evidence

- Current Python symbols/files covered: `src/sensors.py::_read_hd_temp_cached`,
  `_read_fan_speed_cached`, `_hwmon_find`, `_find_hd_temps`,
  `_resolve_nvme_namespace`, `_hwmon_device_label`, `_find_fans`,
  `_resolve_mount_device`, `_whole_disk_of`, `_detect_disk_io_device`,
  `_is_rotational`, `_detect_disks`, `_resolve_mounts`, `_read_disk_usage`, and
  `_read_disk_io`.
- Oracle fixtures/cases:
  - all four existing Python mount-resolution cases from `tests/test_sensors.py`;
  - escaped mount-path decoding for `/proc/mounts` parity;
  - manual-overrides-precede-autodetect for disk temperatures;
  - NVMe namespace and SCSI/drivetemp label derivation through resolved sysfs
    topology;
  - numbered-fan early-stop semantics;
  - 30-second temp/fan TTL caching;
  - mountpoint → whole-disk resolution for partition-backed roots and mapper
    fallback;
  - supported whole-disk detection with rotational flags;
  - first-sample, zero-`dt`, device-switch, and rollback suppression for disk
    byte rates;
  - df/psutil-style visible-percent math and half-even GiB rounding for disk
    usage.
- Exact differences remaining: none in lane scope.
- Inventory entries proposed resolved: both new Rust files plus the Python
  disk-owned callables listed above.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo test --manifest-path rust/Cargo.toml sensors::disk --all-targets --all-features` | pass | 17 focused `disk.rs` tests. |
| `cargo test --manifest-path rust/Cargo.toml sensors::hwmon --all-targets --all-features` | pass | 3 focused `hwmon.rs` helper tests. |
| `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | pass | Rustfmt clean after formatting. |
| `cargo check --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | All targets compile. |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | Current stable toolchain; no warnings. |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 284 library + 23 integration tests (307 total). |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps --all-features` | pass | Public disk sensor API documented. |
| `PYTHONPATH=src .venv/bin/python -m pytest tests/test_sensors.py -v` | pass | Python sensor baseline 4/4. |

## Dependencies and safety

- New/changed dependencies: none; `Cargo.toml` and `Cargo.lock` unchanged.
- Native/build/runtime requirements: none added.
- Unsafe/FFI locations and invariants: none; crate-level `deny(unsafe_code)` remains effective for production code.

## Risks/blockers

- Known risks: `detect_disks` intentionally exposes a sysfs-backed identity view
  (label/kind/rotational) only; the later `POWER` lane still owns the exact
  UDisks2 SMART call path and any TTL split that depends on that facade.
- Blocker requiring integration decision: none.
- Suggested next lane/API change: start `PROCESS`, `POWER`, or `FORMATTER`; all
  of them are now unblocked, while `GPU` remains correctly blocked on `PROCESS`.

## Review notes

- Diff inspected for out-of-scope paths: yes.
- Production runtime untouched by tests: yes.
- No skipped/weakened checks: yes.
- Rebase required before merge: no.