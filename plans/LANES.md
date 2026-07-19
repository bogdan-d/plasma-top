# Parallel execution lanes

## Rules

- One agent owns one lane at a time in its own branch/worktree.
- Agents edit only owned paths plus their lane handoff file.
- Composition roots (`lib.rs`, `sensors/mod.rs`, `render/mod.rs`, `daemon.rs`,
  aggregate scripts, `Cargo.toml`, `Cargo.lock`) are integration-owner paths
  unless a lane explicitly owns them.
- Shared contract changes are proposed in the handoff, not made opportunistically.
- A lane rebases onto the integration branch before final validation; agents do
  not merge other lanes themselves.
- Discovery/evidence may be shared; implementation paths may not overlap.
- Every lane reports exact commands, results, fixture additions, unresolved
  parity differences, and commit SHA using `handoffs/TEMPLATE.md`.

## Dependency waves

| Wave | Parallel lanes | Starts after |
|---|---|---|
| 0 | `BASE` | none |
| 1 | `SCAFFOLD` | `BASE` |
| 2 | `DOMAIN`, `CONFIG`, `RUNTIME`, `FIXTURES` | frozen scaffold contracts |
| 3 | `RENDER-CORE`, `TRACES`, `SENSOR-CPU`, `SENSOR-MEM`, `SENSOR-NET`, `SENSOR-DISK` | relevant Wave 2 lanes |
| 4 | `FORMATTER`, `CHART`, `PAGES`, `PROCESS`, `POWER`, `GPU`, `HID`, `NOTIFY` | relevant Wave 3 lanes plus integration-owned production boundary freeze where consumed |
| 5 | `COLLECTOR` then `DAEMON-CLI` | all runtime dependencies |
| 6 | `QML-VERIFY`, `PACKAGING` | integrated Rust binary |
| 7 | `HARDWARE-*` validation lanes | packaged/shadow binary |
| 8 | `CUTOVER` stabilization | all mandatory evidence |

`INTEGRATION` runs continuously between waves and is the only owner allowed to
resolve cross-lane overlaps.

## Lane contracts

### BASE — Python oracle and baseline

- **Objective:** make current behavior reproducible and fixtureable.
- **Owns:** Python dev-test manifest/setup additions, oracle tools, parity fixture
  schema/data, baseline reports, inventory generator.
- **Must not change:** production algorithms, defaults, HTML snapshots.
- **Dependencies:** none.
- **Validation:** full Python suite, ruff, vulture, CLI smoke, deterministic oracle
  regeneration.
- **Handoff:** baseline versions/results; fixture schema; known doc/runtime drift.

### SCAFFOLD — Rust crate and shared contracts

- **Objective:** establish crate, lint/build policy, shared domain boundary types.
- **Owns:** `rust/Cargo.toml`, initial `Cargo.lock`, `rust/src/{lib,main,error}.rs`,
  initial domain contracts, test support skeleton.
- **Must not implement:** renderer, sensors, daemon behavior.
- **Dependencies:** `BASE` fixture schema.
- **Validation:** fmt/check/clippy/test/doc; dependency/license review.
- **Freeze:** after handoff, `Cargo.toml`, `Cargo.lock`, and shared types become
  integration-owner paths.

### DOMAIN — metric × form model

- **Objective:** validated tokens, metric metadata, forms, surfaces, placement,
  capability derivation.
- **Owns:** `rust/src/domain/{form,metric,item}.rs`, focused tests.
- **References:** `src/forms.py`, `src/metrics.py`, token/capability parts of
  `src/registry.py`.
- **Dependencies:** `SCAFFOLD` types, Python item oracle.
- **Validation:** exhaustive token corpus, unknown/misplaced matrix, capability
  call set, list-items ordering/output.

### CONFIG — config/assets/geometry

- **Objective:** exact TOML resolution, merge, typed defaults, DMI selection,
  orientation and auto-fit.
- **Owns:** `rust/src/config/**`, config fixtures/tests.
- **References:** `src/config.py`, `config/*.toml`, `style/icons.toml`, `lang/*`.
- **Dependencies:** `DOMAIN` item parser can be stubbed through frozen API until
  integrated; `FIXTURES` filesystem boundary.
- **Validation:** port every `tests/test_config.py` behavior, randomized deep
  merge/geometry differential tests, malformed and permission failures.

### RUNTIME — paths, atomic writes, page state

- **Objective:** exact runtime layout, atomic replacement, counter locking.
- **Owns:** `rust/src/runtime/**`, runtime/page integration tests.
- **References:** `src/runtime.py`, `src/pagestate.py`.
- **Dependencies:** `SCAFFOLD` error/path types, `FIXTURES` temp roots.
- **Validation:** path fallback, write atomicity/readers, process concurrency,
  malformed files, permissions, cleanup.

### FIXTURES — deterministic boundary framework

- **Objective:** common fake clock, filesystem roots, command/D-Bus responses,
  fixture deserialization and oracle invocation.
- **Owns:** `rust/src/test_support/**` or test-only equivalent,
  `rust/tests/fixtures/**`, parity runner scripts.
- **Must not own:** production adapter semantics.
- **Dependencies:** `BASE` schema, `SCAFFOLD` shared result types.
- **Validation:** fixtures round-trip; no host `/proc`, `/sys`, D-Bus, or commands
  accessed in deterministic tests.

### RENDER-CORE — render model and mono serializer

- **Objective:** cells/rows/blocks, HTML escaping, widths, grouping, all five
  layout plans, horizontal inline output.
- **Owns:** `rust/src/render/{model,mono}.rs`, focused tests.
- **References:** `src/render_model.py`, `src/mono_render.py`, `docs/LAYOUT.md`.
- **Dependencies:** `DOMAIN` identifiers; may use frozen mock identifiers until
  integration.
- **Validation:** port all render-model/mono tests, property tests for width/right
  edges, no-table invariant, byte differential corpus.

### TRACES — bars/sparks/braille

- **Objective:** exact percentage visuals and standalone/combo rows.
- **Owns:** `rust/src/render/traces.rs`, trace fixtures/tests.
- **References:** `src/traces.py` and trace-related formatter tests.
- **Dependencies:** `RENDER-CORE` API, config size types.
- **Validation:** exhaustive values including `None`, boundaries, history lengths,
  both surfaces/orientations; byte parity.

### SENSOR-CPU — CPU/load/uptime/core histories

- **Objective:** `/proc/stat`, uptime/load, CPU frequency/turbo and histories.
- **Owns:** `rust/src/sensors/cpu.rs`, CPU fixtures/tests.
- **References:** corresponding `src/sensors.py` symbols in `INVENTORY.md`.
- **Dependencies:** `FIXTURES` reader/clock; frozen state/readings types.
- **Validation:** first/delta/reset/overflow/malformed/per-core/history cadence and
  capability-zero-call cases.

### SENSOR-MEM — memory/swap

- **Objective:** preserve psutil-equivalent total/available/percent semantics and
  memory history.
- **Owns:** `rust/src/sensors/memory.rs`, meminfo fixtures/tests.
- **Dependencies:** `FIXTURES`, frozen readings/state.
- **Validation:** Linux meminfo variants, zero/missing fields, rounding, bounded
  history, Python differential values.

### SENSOR-NET — route, wifi, identity, rates

- **Objective:** active route/device/IP/SSID/signal and per-interface rates.
- **Owns:** `rust/src/sensors/network.rs`, command/sysfs fixtures/tests.
- **Dependencies:** `FIXTURES` command runner/clock.
- **Validation:** `ip`/`iw` argv and parsing, command absence/timeout/error,
  interface switching resets, dBm conversion, TTL behavior.

### SENSOR-DISK — mounts, usage, I/O, hwmon, disk identity

- **Objective:** mount filtering, statvfs usage, block topology, rates, hwmon
  temperature/fan discovery, SMART identity/cache domain.
- **Owns:** `rust/src/sensors/{disk,hwmon}.rs`, disk/sysfs fixtures/tests.
- **Dependencies:** `FIXTURES` reader/clock; D-Bus SMART execution remains POWER.
- **Validation:** all mount tests, partition stacks, NVMe namespaces, rotational
  flags, missing/permission/malformed files, rate/cache boundary cases.

### FORMATTER — registry and full formatter

- **Objective:** cells, item dispatch, hardware gates, formatter methods,
  canonical width, main panel/tooltip.
- **Owns:** `rust/src/render/{registry,cells,formatter}.rs` and formatter tests.
- **References:** `src/items.py`, render half of `src/registry.py`,
  `src/formatter.py`.
- **Dependencies:** `DOMAIN`, `CONFIG`, `RENDER-CORE`, `TRACES`, and the
  integration-owned typed aggregate hardware/readings render contract.
- **Validation:** every item alone, every irregular layout, existing formatter
  suite, H/V/tooltip goldens, canonical-width guard.

### CHART — graphs PNG

- **Objective:** identical chart dimensions, grid, fill, line, overlay, labels,
  RGBA pixels, and deterministic encoding policy.
- **Owns:** `rust/src/render/chart.rs`, chart fixtures/tests.
- **References:** `src/chart.py`, graph formatter behavior.
- **Dependencies:** fixture schema only; FORMATTER integration later.
- **Validation:** pixel hashes, decode/CRC, empty/single/constant/max series,
  overlay, width/height boundaries; encoded byte comparison where stable.

### PAGES — page registry, command bodies, pager

- **Objective:** page ordering, `ss`/fastfetch command handling, connection
  formatting, page shell/title/pager/click action.
- **Owns:** `rust/src/{page_commands}.rs` and page-focused render file/tests.
- **References:** `src/pages.py`.
- **Dependencies:** `DOMAIN`, `RENDER-CORE`, `FIXTURES`, and the
  integration-owned production command-runner trait implemented by the fake.
- **Validation:** every page and command outcome; ANSI/PTY/cache/ellipsize/service
  resolution; byte parity.

### PROCESS — top processes and Intel process attribution

- **Objective:** process snapshots/deltas/cmdline naming and DRM fdinfo scans.
- **Owns:** `rust/src/sensors/{process,gpu_intel}.rs`, fixtures/tests.
- **References:** process/Intel symbols in `src/sensors.py`.
- **Dependencies:** `FIXTURES`, CPU clock/state types.
- **Validation:** PID reuse/disappearance, malformed stat/cmdline/fdinfo,
  denominator/clamp, TTL vs page sample independence, permission failures.

### POWER — D-Bus, batteries, UDisks SMART

- **Objective:** UPower/UDisks2 discovery/property reads, battery caches and SMART
  calls.
- **Owns:** `rust/src/sensors/power.rs`, D-Bus facade and fixtures/tests.
- **References:** D-Bus/battery/SMART symbols in `src/sensors.py`.
- **Dependencies:** `FIXTURES`, SENSOR-DISK identity types, and the
  integration-owned production D-Bus facade contract implemented by the fake.
- **Validation:** decoded success shapes plus bus/service/object/property absence,
  malformed variants, timeout/error, cache TTL, system/peripheral semantics.

### GPU — NVIDIA and remaining GPU orchestration

- **Objective:** PCI detection, NVML reads, `nvidia-smi` fallback, clamps/cache,
  vendor-selected graph histories.
- **Owns:** `rust/src/sensors/gpu_nvidia.rs` and GPU orchestration tests; coordinate
  with PROCESS owner for `gpu_intel.rs` API without editing it.
- **Dependencies:** `FIXTURES`, PROCESS Intel API.
- **Validation:** no GPU, NVML success/init/read failure, fallback argv/CSV/errors,
  cache selection/TTL, clamp, vendor preference, history cadence.

### HID — Bolt battery protocol

- **Objective:** hidraw discovery and Logitech feature/query protocol.
- **Owns:** `rust/src/sensors/hid.rs`, binary protocol fixtures/tests.
- **References:** `src/bolt_battery.py`.
- **Dependencies:** reviewed HID dependency and frozen battery value type.
- **Validation:** packet bytes, timeout, short/mismatched response, feature absent,
  device absent, name decoding, battery conversion; safe-wrapper review.

### NOTIFY — notification state machine

- **Objective:** exact latch/hysteresis/hold and emitted notification payloads.
- **Owns:** `rust/src/notify.rs`, notifier tests/fake facade.
- **References:** `src/notifier.py`, `tests/test_notifier.py`.
- **Dependencies:** CONFIG, the typed aggregate readings/hardware contract, fake
  clock, and the production notification facade shared with test support.
- **Validation:** port full notifier suite plus every notification type, facade
  failure, per-device state cleanup/retention behavior.

### COLLECTOR — discovery and collect composition

- **Objective:** exact capability-driven call ordering and state mutation.
- **Owns:** `rust/src/sensors/mod.rs`, collector integration tests.
- **Dependencies:** all sensor lanes, DOMAIN, CONFIG.
- **Validation:** call-trace comparison against Python for every capability set,
  skip-slow behavior, rescan, combined histories, failure isolation.

### DAEMON-CLI — lifecycle, diagnostics, process entry

- **Objective:** compose daemon and all CLI commands.
- **Owns:** `rust/src/{daemon,cli,diagnostics,main}.rs`; integration tests.
- **References:** root `pirostats`, `src/daemon.py`.
- **Dependencies:** all backend lanes.
- **Validation:** full CLI matrix, fake-clock daemon lifecycle, reload, theme/CSS,
  publication, page wake, signals, profile output, Python differential runs.

### QML-VERIFY — applet contract validation

- **Objective:** prove unchanged applet works; edit QML only for a confirmed Rust
  incompatibility approved by integration owner.
- **Owns:** QML test evidence and, only if approved, narrowly scoped
  `plasmoid/package/**` changes.
- **Dependencies:** integrated Rust daemon.
- **Validation:** Qt shots and live Plasma interaction matrix in `TESTING.md`.

### PACKAGING — install/systemd/AUR

- **Objective:** locked native build, upgrade/uninstall, asset resolution.
- **Owns:** `install.sh`, `uninstall.sh`, `service/`, `packaging/aur/`, package
  documentation/tests.
- **Dependencies:** release Rust binary and dependency inventory.
- **Validation:** shell checks, package build, file manifest, disposable upgrade
  from Python, rollback, uninstall preserving user config.

### HARDWARE-* — live validation only

Separate non-editing lanes: `HARDWARE-INTEL`, `HARDWARE-NVIDIA`,
`HARDWARE-POWER`, `HARDWARE-DISK`, `HARDWARE-NET`, `HARDWARE-DESKTOP`.

- **Objective:** run approved probe/shadow/soak matrix on available machines.
- **Owns:** one handoff/evidence file only; no implementation edits.
- **Dependencies:** signed integration candidate.
- **Validation:** commands from `TESTING.md`, sanitized result bundle.

### INTEGRATION — sole cross-lane owner

- **Objective:** verify handoffs, merge in dependency order, resolve APIs, run
  aggregate gates, update `STATUS.md` and inventory evidence.
- **Owns:** composition roots, dependency files after scaffold freeze, aggregate
  scripts, `plans/STATUS.md`, cross-lane fixes.
- **Must not:** accept unverifiable claims, hide failures, or let lane agents
  self-certify integration.
