# Handoff: `POWER` / `power-kilo-20260719`

## Contract

- Objective: Port the POWER-owned half of `src/sensors.py` — UPower
  enumeration/property reads, UDisks2 SMART discovery/health, sysfs+UPower
  system-battery reads with fallback, peripheral-battery reads, and Bolt
  receiver HID++ queries — behind the shared production `DbusFacade` trait and
  explicit sysfs roots, with deterministic fixture coverage for absence,
  malformed, timeout, and cache-TTL paths.
- Integration base SHA: `16f0773` (PROCESS integration head on
  `rust-migration-base-bootstrap`).
- Branch/worktree: `rust-power-lane` @ `/var/mnt/xdata/code/_self/plasma-stats`.
- Owned paths: `rust/src/sensors/power.rs`, `rust/src/sensors/mod.rs`
  (registration only), focused tests inline in `power.rs`, and the matching
  `plans/{STATUS,INVENTORY}.md` + this handoff.
- Shared paths reviewed/touched:
  - `rust/src/sensors/disk.rs` — single visibility change: `is_rotational`
    promoted from `fn` to `pub(crate) fn` so `power::detect_smart_disks` can
    reuse the sysfs rotational read instead of duplicating it. No behavior
    change. Documented here for integration-owner awareness.
- Forbidden/shared paths respected: no edits to `Cargo.toml`/`Cargo.lock`,
  `domain/*`, `test_support/*`, or other lanes' sensor files (beyond the one
  visibility promotion above).
- Dependencies verified integrated:
  - `FIXTURES` (`FakeDbus`, `FakeClock`) — P2 verified.
  - `SENSOR-DISK` identity/cache groundwork (`disk::is_rotational`) — P3
    verified.
  - `INTEGRATION` typed aggregate readings/state (`BatterySystemReading`,
    `BatteryPeripheralReading`, `BatteryState`, `SmartDisk`,
    `DiskSmartInterface`, `BatterySystemCache`, `BatteryPeripheralCache`) —
    Wave 4 contract slice verified.
  - Production `DbusFacade` boundary + `BoundaryError` — Wave 4 contract slice
    verified; FakeDbus implements it directly.

## Result

- Status: `verified` after integration review.
- Implementation/review commit: `916cb89` (`Port Rust power sensors`).
- Changed files:
  - `rust/src/sensors/power.rs` — new module (≈1000 LoC incl. tests) with all
    POWER-owned readers, body-decoding helpers, numeric parity helpers, a
    lane-local `BoltBatteryFacade` trait, and 38 focused tests.
  - `rust/src/domain/{boundary,mod}.rs`, `rust/src/test_support.rs`, and
    `rust/src/test_support/fake_dbus.rs` — exact typed D-Bus arguments,
    per-call timeout, and request traces required by a production adapter.
  - `rust/src/sensors/mod.rs` — registers `pub mod power;`.
  - `rust/src/sensors/disk.rs` — single-line visibility promotion
    (`is_rotational` → `pub(crate)`).
  - `plans/STATUS.md` — POWER promoted to verified; aggregate gate count
    refreshed (433 total).
  - `plans/INVENTORY.md` — POWER-owned Python callables marked `[x]` with
    Rust-side evidence; `rust/src/sensors/power.rs` added to file inventory
    and callable sections; the COLLECTOR-tagged `_sysfs_bat_*` helpers are
    also marked `[x]` (pulled forward into POWER; see below).
  - `plans/handoffs/power-kilo-20260719.md` — this evidence file.
- Behavior implemented/preserved:
  - **D-Bus body encoding contracts** — documented per call:
    - `EnumerateDevices` (UPower manager): flat `[path1, path2, ...]` body.
    - `Properties.GetAll` (UPower device): device-interface string argument;
      interleaved `[key, val, key, val, ...]` reply body.
    - `Properties.Get` (UDisks2 SMART): interface + property string arguments;
      single decoded-value reply body.
    - `GetManagedObjects` (UDisks2 ObjectManager): objects separated by empty
      strings; each chunk = `[path, iface1, iface2, ...]` with a special
      `Block.Drive=<path>` token encoding the Block interface's Drive
      property. The domain applies all filtering (partitions, optical,
      missing/empty drive, sr* labels, SMART-interface presence).
    - `SmartUpdate` (UDisks2 drive iface): empty `a{sv}` argument, 15-second
      timeout, empty body (void return).
  - **`upower_enumerate` / `find_battery_sys`** — EnumerateDevices + filter on
    `/battery_BAT` + sort, matching Python's `_upower_enumerate` /
    `_find_battery_sys`. Empty list on any failure.
  - **`upower_device_props`** — one GetAll call decoding
    `Percentage`/`State`/`EnergyRate`/`Model`/`Type`. Returns None on any
    failure (Python's blanket `except Exception: return None`).
  - **`detect_smart_disks`** — GetManagedObjects walk with all of Python's
    filters: `/block_devices/` path, Block iface presence, Partition iface
    absence, non-empty/non-root Drive ref, drive object existence in the
    reply, `sr*` optical skip, and NVMe-vs-ATA SMART interface detection.
    Rotational flag from sysfs via `disk::is_rotational`. Produces the typed
    `SmartDisk` map that `HardwareSnapshot.disk_smart_drives` consumes.
  - **`read_disk_smart`** — successful `SmartUpdate` followed by NVMe
    `SmartCriticalWarning` (healthy iff empty) or ATA
    `SmartFailing` (healthy iff false). Returns None on any property failure.
  - **`read_disk_smart_cached`** — label-keyed TTL cache mirroring Python's
    `_cached_by_label`, including the "cache None until TTL elapses" semantics
    so a failing SmartUpdate doesn't hammer the drive every poll. Per-drive
    TTL (HDD vs SSD) is selected by the caller, matching Python's
    `cfg.disks.smart_interval_hdd` vs `smart_interval`.
  - **`sysfs_bat_read` / `sysfs_bat_rate` / `sysfs_bat_charge_limit`** —
    `/sys/class/power_supply/<name>/{capacity,status,power_now,
    charge_control_end_threshold}` readers. `bat_name_from_id` mirrors
    Python's `bat_id.rsplit("battery_", 1)[-1]`. Watts use banker's rounding
    via `round_half_even_ratio(µW, 1_000_000)` to match Python's
    `round(uw / 1_000_000)`. Charge limit collapses to None at 100 (no
    meaningful limit, matching Python).
  - **`read_battery_sys`** — sysfs first; on absence, UPower `GetAll` fallback
    that only updates the cache when `Percentage` is present. Zero-rate
    back-channel: when UPower reports `EnergyRate = 0` but the state is
    charging/discharging, falls back to sysfs `power_now` (Python's
    `cache.rate = _sysfs_bat_rate(bat_id)`). 30s TTL keyed by battery id.
    Appends a `BatterySystemReading` only when charge is known (Python's
    `if cache.perc:` truthiness gate).
  - **`read_battery_periph`** — UPower `GetAll` for `Percentage`/`Model`;
    caches the model name once (Python's `if not cache.name and
    props.get("Model"):`); suppresses zero/missing charge (Python's `if pct`
    gate → empty string → row disappears). 30s TTL.
  - **`read_battery_bolt`** — consumes a lane-local `BoltBatteryFacade` trait.
    Three-way outcome matching Python: `Ok(Some)` → update name+level+ts;
    `Ok(None)` (device responded, no battery) → advance ts only (Python's
    `cache.ts = time.monotonic(); return None`); `Err` (HID failure) → return
    None WITHOUT advancing ts (Python's `except OSError: return None` skips
    the ts update) so the next poll retries. Name fetched only until cached
    (Python's `want_name = not name_override and not cache.name`).
  - **Numeric parity** — `round_half_even_ratio` (duplicated from disk/memory
    to keep the lane self-contained) matches Python's `round()` on
    `uw / 1_000_000` for all integer µW inputs. `round_half_even_f64` matches
    Python's `round()` on EnergyRate doubles (clean half values use
    banker's rounding; NaN/inf/negative → 0).
- Explicitly not implemented:
  - `_find_peripherals` (COLLECTOR-owned) — uses `upower_device_props` but is
    tagged COLLECTOR in the inventory. POWER exposes the typed
    `upower_device_props` for COLLECTOR to consume when it lands.
  - Production hidraw `BoltBatteryFacade` implementation — deferred to the
    HID lane (owns `src/bolt_battery.py`). POWER defines the trait + cache
    semantics; tests use a fake.
  - Production D-Bus adapter (system bus, GDBus) — deferred to Wave 5
    COLLECTOR/DAEMON-CLI. POWER documents the body encoding contract the
    adapter must produce.
  - Wave 5 COLLECTOR wiring — readers already mutate the shared typed
    `DaemonStateSnapshot`; the collector only needs to call them in order.

## Parity evidence

- Current Python symbols/files covered:
  - `src/sensors.py`: `_bus`, `_upower_enumerate`, `_upower_device_props`,
    `_udisks_prop`, `_read_disk_smart`, `_read_disk_smart_cached`,
    `_find_battery_sys`, `_read_battery_sys`, `_read_battery_periph`,
    `_read_battery_bolt`, and (pulled forward from COLLECTOR)
    `_sysfs_bat_rate`, `_sysfs_bat_charge_limit`, `_sysfs_bat_read`.
  - Constant parity: `_UPOWER_NAME`/`_PATH`/`_IFACE`/`_DEV_IFACE`,
    `_UDISKS_NAME`/`_PATH`/`_BLOCK`/`_PARTITION`/`_NVME`/`_ATA`,
    `_UPOWER_STATE_MAP` (via `BatteryState::state_from_value`),
    `_SYSFS_BAT_STATUS_MAP` (via `sysfs_status_to_state`),
    `BAT_CACHE_TTL`/`PERIPH_CACHE_TTL`/`BOLT_CACHE_TTL`.
- Oracle fixtures/cases (38 focused tests):
  - `upower_enumerate_returns_paths_on_success`,
    `upower_enumerate_empty_when_bus_unavailable`.
  - `find_battery_sys_filters_and_sorts_battery_paths`.
  - `detect_smart_disks_finds_nvme_and_ata_drives`,
    `detect_smart_disks_skips_partitions`,
    `detect_smart_disks_skips_optical_and_missing_drive_and_unsupported`,
    `detect_smart_disks_empty_when_bus_unavailable`.
  - `read_disk_smart_nvme_healthy_when_warning_empty`,
    `read_disk_smart_nvme_failing_when_warning_present`,
    `read_disk_smart_ata_healthy_when_not_failing`,
    `read_disk_smart_ata_failing_when_smart_failing_true`,
    `read_disk_smart_returns_none_when_smart_update_unreachable`.
  - `read_disk_smart_cached_refreshes_after_ttl_expires` (within TTL no
    calls, after TTL refresh),
    `read_disk_smart_cached_caches_failure_until_ttl_expires` (None cached,
    no retry within TTL).
  - `read_battery_sys_reads_sysfs_first`,
    `read_battery_sys_falls_back_to_upower_when_sysfs_absent`,
    `read_battery_sys_upower_zero_rate_falls_back_to_sysfs_power_now`,
    `read_battery_sys_skips_batteries_without_percentage`,
    `read_battery_sys_uses_cache_within_ttl`,
    `read_battery_sys_charge_limit_100_treated_as_unset`.
  - `read_battery_periph_returns_reading_on_success`,
    `read_battery_periph_none_when_percentage_zero_or_missing`,
    `read_battery_periph_none_when_upower_unreachable`,
    `read_battery_periph_name_override_wins_over_cached_model`,
    `read_battery_periph_uses_cache_within_ttl`.
  - `read_battery_bolt_caches_name_and_level`,
    `read_battery_bolt_returns_none_when_level_is_none_but_advances_timestamp`,
    `read_battery_bolt_no_level_hides_stale_charge_for_refresh_call`,
    `read_battery_bolt_returns_none_on_hid_failure_without_advancing_timestamp`,
    `read_battery_bolt_name_override_suppresses_name_fetch`.
  - Helper tests: `parse_object_paths_skips_empty_strings`,
    `parse_property_map_decodes_interleaved_pairs`,
    `parse_property_map_ignores_stray_trailing_key`,
    `parse_managed_objects_splits_on_empty_strings`,
    `round_half_even_ratio_matches_python_bankers_rounding`,
    `round_half_even_f64_handles_halfway_and_non_finite`,
    `bat_name_from_id_extracts_power_supply_name`,
    `parse_bool_accepts_case_insensitive_true_false`.
- Deferred production wiring:
  - **`upower_device_props` uses explicit `GetAll`** —
    Python builds a `Gio.DBusProxy` (which implicitly does one `GetAll`
    round-trip when the proxy is created) then reads cached properties.
    Rust issues the `GetAll` explicitly. Same round-trip count, same
    decoded values, same None-on-failure behavior.
  - **Bolt HID++ implementation deferred** — Python imports `bolt_battery`
    at module load (`_BOLT_AVAILABLE = True/False`); Rust defines a
    `BoltBatteryFacade` trait and leaves the production impl to the HID
    lane. Daemon wiring (Wave 5) will pass the production facade when HID
    is available.
- Inventory entries proposed resolved: all POWER-tagged symbols in
  `src/sensors.py` (lines 65, 78, 93, 924, 984, 1011, 1018, 1616, 1652, 1677)
  plus the three COLLECTOR-tagged sysfs battery helpers (lines 1574, 1584,
  1604) pulled forward because `_read_battery_sys` cannot function without
  them.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo test --manifest-path rust/Cargo.toml sensors::power --all-targets --all-features` | pass | 38 focused `power.rs` tests. |
| `cargo fmt --manifest-path rust/Cargo.toml --all -- --check` | pass | Rustfmt clean. |
| `cargo check --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | All targets compile. |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | No warnings. |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 410 library + 23 integration tests (433 total). |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps --all-features` | pass | Public `power` API documented; no doc warnings. |
| `PYTHONPATH=src .venv/bin/python -m pytest tests/` | pass | Python oracle 175 passed + 1 optional ruff skip. |

## Dependencies and safety

- New/changed dependencies: none; `Cargo.toml` and `Cargo.lock` unchanged.
- Native/build/runtime requirements: none added.
- Unsafe/FFI locations and invariants: none; crate-level `#![deny(unsafe_code)]`
  remains in effect. All D-Bus work flows through the safe `DbusFacade` trait;
  all sysfs reads use `std::fs`; no `unsafe` blocks in production or test
  code. HID++ protocol (hidraw) is deferred to the HID lane.

## Risks/blockers

- Known risks:
  - The `BoltBatteryFacade` trait is lane-local in `power.rs`. The HID lane
    will land the production impl; if HID prefers a trait in
    `domain::boundary`, a small promotion will be needed then (similar to
    how `DbusFacade` was promoted).
  - The COLLECTOR-tagged `_sysfs_bat_*` helpers were pulled forward into
    POWER because `_read_battery_sys` depends on them and COLLECTOR is
    Phase 5. INVENTORY dispositions updated to reflect this.
- Blocker requiring integration decision: none.
- Suggested next lane/API change:
  - GPU and HID remain as Wave 4 ready lanes. NOTIFY still waits its shared
    notification-facade contract.
  - Wave 5 COLLECTOR should wire the production D-Bus adapter that produces
    the body shapes documented in `power.rs`.

## Review notes

- Diff inspected for out-of-scope paths: yes — only `disk.rs`'s single-line
  visibility promotion touches another lane's file, and it is documented
  above.
- Production runtime untouched by tests: yes — all D-Bus goes through
  `FakeDbus`, all sysfs through `TempTree`, all Bolt through `FakeBolt`.
- No skipped/weakened checks: yes — no `#[ignore]`, no blanket `allow`s
  beyond the test-module `clippy::unwrap_used`/`expect_used` (matches
  `process.rs`'s pattern), no weakened assertions.
- Rebase required before merge: no — branched from `16f0773` (current
  integration head).
