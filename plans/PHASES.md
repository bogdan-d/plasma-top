# Migration phases

Each phase ends at a stable rollback point. A phase gate is verified by the
integration owner, not accepted solely from an agent report.

## Phase 0 — Freeze and measure Python behavior

**Objective:** create a reproducible oracle before Rust behavior exists.

### Steps

- **P0.1** Confirm the recorded base commit and clean-tree state, then record
  host/runtime versions, active config, and hardware capabilities.
- **P0.2** Create reproducible Python dev/test setup; resolve the unconditional
  `psutil` dependency and document pytest/ruff/vulture installation without
  changing runtime behavior.
- **P0.3** Run full Python suite, ruff, vulture, CLI smoke commands, shell syntax,
  and Qt screenshot tools. Save outputs under ignored test-artifact storage.
- **P0.4** Add shared deterministic fixture schema for `Config`, `HardwareInfo`,
  `Readings`, `DaemonState`, command results, D-Bus replies, `/proc`, and `/sys`.
- **P0.5** Add Python oracle tool that consumes fixtures and emits deterministic
  render/config/sensor-domain results. It must not alter production imports.
- **P0.6** Generate callable/call-edge inventory and map every symbol to a parity
  test or explicit non-port disposition in `INVENTORY.md`.
- **P0.7** Capture live probes/profiles from available Intel/NVIDIA/battery/disk/
  network machine variants; redact unstable/private data.

### Gate P0

- Python checks pass from documented clean setup.
- Existing golden snapshots pass unchanged.
- Oracle fixtures regenerate deterministically.
- Every current Python callable appears in inventory.
- Baseline performance and live-hardware evidence recorded.

**Rollback:** remove harness-only additions; production remains Python.

## Phase 1 — Rust scaffold and contract freeze

**Objective:** establish build, types, test commands, and lane boundaries.

### Steps

- **P1.1** Create single crate under `rust/`, commit `Cargo.lock`, set supported
  stable toolchain/MSRV based on target distro.
- **P1.2** Add `lib.rs`/`main.rs` shells, strict lint policy, fixture loader, and
  test support. No production cutover.
- **P1.3** Define frozen domain/API contracts needed by parallel lanes: metric,
  form, surface, capability, readings, hardware, state, clock, command result,
  filesystem roots, and D-Bus facade outputs.
- **P1.4** Add dependency review entries and license compatibility check.
- **P1.5** Add Rust checks to aggregate validation while Python remains primary.

### Gate P1

- Empty/scaffold crate passes fmt/check/clippy/test/doc.
- Shared contracts compile and have constructor/default/invariant tests.
- Parallel lane file ownership is frozen in `LANES.md`.

**Rollback:** delete `rust/`; Python unaffected.

## Phase 2 — Parallel foundations

**Objective:** port independent pure/boundary foundations.

Parallel lanes:

- **P2-DOM:** forms, metrics, token validation, placement, capabilities.
- **P2-CFG:** TOML resolution, merge, machine detection, geometry, item filtering.
- **P2-RTP:** runtime paths, atomic writes, page state/flock.
- **P2-FIX:** fixture filesystem, fake clock, command runner, D-Bus fake support.

### Required parity

- exhaustive token list equals `pirostats list-items`
- all config tests ported plus Python/Rust differential corpus
- runtime/page concurrency stress catches lost updates
- geometry/auto-fit formulas match boundary cases and randomized inputs
- every boundary failure maps to expected Rust error/degradation

### Gate P2

All four lane handoffs integrated; aggregate Python+Rust checks pass. Shared API
changes after this gate require integration-owner review and dependent-lane rerun.

## Phase 3 — Parallel rendering and baseline sensors

**Objective:** produce exact deterministic output and core Linux readings.

Parallel lanes:

- **P3-RCORE:** cells, blocks, threshold classes, grouping, five mono plans.
- **P3-TRACE:** bar/column/spark/braille encodings and row combinations.
- **P3-SCPU:** CPU, per-core CPU, uptime, load, histories.
- **P3-SMEM:** memory/swap and bounded memory history.
- **P3-SNET:** route/device identity, wifi, network rates.
- **P3-SDISK:** mounts, usage, topology, I/O rates, hwmon temps/fans, SMART model.

### Gate P3

- Ported pure tests pass.
- All renderer primitives are byte-identical against oracle fixtures.
- Sensor parser/formula fixtures pass success and failure matrices.
- No real host I/O occurs in fixture tests.
- Capability gating proves unrequested sensors make zero adapter calls.

## Phase 4 — Formatter, pages, hardware adapters, notifications

**Objective:** complete domain behavior above foundations.

Integration prerequisite: replace the scaffold-only aggregate hardware/readings
snapshots with the typed render/collector contract and promote command/D-Bus
traits from feature-gated test support into production boundaries. Lanes that do
not consume those shared APIs may start while this contract slice lands.

Parallel lanes:

- **P4-FMT:** item registry rendering, formatter, canonical width, main tooltip.
- **P4-CHART:** graph pixel generation and deterministic PNG handling.
- **P4-PAGE:** processes/connections/fastfetch/page HTML and command cache.
- **P4-PROC:** top-process sampling/cmdline and Intel DRM process attribution.
- **P4-POWER:** UPower/UDisks2, system/peripheral battery, disk SMART calls.
- **P4-GPU:** Intel/NVIDIA metrics, NVML/fallback, histories.
- **P4-HID:** Bolt/HID query protocol.
- **P4-NOTIFY:** latch transitions and notification facade.

### Gate P4

- Panel H/V and main tooltip snapshots byte-match Python.
- Every configured item renders alone and canonical width covers it.
- Every page passes deterministic body/shell parity.
- Chart pixels match exactly; encoded bytes match or approved normalized
  comparison proves identical decoded image.
- D-Bus/NVML/HID fixtures cover absent, malformed, timeout/error, and success.
- Notification transitions and emitted payloads match Python.

## Phase 5 — Collector, daemon, diagnostics, CLI

**Objective:** compose complete Rust backend without changing QML/systemd.

Mostly serial integration after Phase 4.

### Steps

- **P5.1** Compose discovery/rescan and demand-driven `collect` in exact order.
- **P5.2** Port CSS/theme detection and reload behavior.
- **P5.3** Port first paint, poll loop, page-change wakeup, boot-watch logging,
  signal cleanup, and last-good config reload.
- **P5.4** Port `render`, `probe`, `profiling`, `list-items`, `page`, `click`, and
  help/error behavior.
- **P5.5** Add isolated runtime-dir daemon integration tests with fake clock and
  deterministic adapters.
- **P5.6** Run Python and Rust CLIs against identical fixtures and normalize only
  explicitly volatile fields/timings.

### Gate P5

- Every CLI invocation and error case covered.
- Daemon lifecycle tests prove ordering, atomicity, reload, cleanup, page latency,
  no duplicate/not-requested calls, and no busy loop.
- Differential output has zero unexplained differences.
- Rust daemon does not write production runtime during tests.

## Phase 6 — Applet and packaging integration

**Objective:** prove existing QML and install surfaces work with Rust unchanged.

### Steps

- **P6.1** Run a QML test instance and Rust daemon under disposable XDG/runtime
  roots; systemd is optional and `/usr/bin/pirostats` must not be replaced.
- **P6.2** Run `tools/qt_shot.py` and live Plasma screenshot matrix for panel H/V,
  tooltip pages, pinning, desktop dark/light, overlay, and geometry changes.
- **P6.3** Verify watcher behavior, lazy tooltip reads, wheel concurrency, command
  nonces, page latency, and no extra runtime-root events.
- **P6.4** Update `install.sh`, uninstall, systemd/package assets, and AUR recipe
  for locked native build; preserve user files.
- **P6.5** Test install, upgrade from Python package, restart, and uninstall in a
  disposable VM/container/session appropriate for Plasma.

### Gate P6

- QML needs no behavior workaround; any unavoidable edit has separate evidence.
- Screenshot/runtime interaction matrix passes.
- Package owns expected files and has complete dependency/license metadata.
- Rollback package restores Python version without user config loss.

## Phase 7 — Shadow and hardware matrix

**Objective:** validate real I/O across supported hardware before cutover.

### Steps

- **P7.1** Run Python and Rust probes near-simultaneously; compare formulas with
  per-metric tolerance and first-sample rules.
- **P7.2** Run Rust daemon against isolated runtime for long sessions: idle,
  suspend/resume, network switches, disk hotplug, theme/config edits, page use.
- **P7.3** Cover available hardware rows: no-GPU, Intel GPU, NVIDIA NVML,
  NVIDIA fallback, battery/UPower, UDisks SMART, HID, wifi/ethernet.
- **P7.4** Compare startup, RSS, warm poll work, slow-page timing, process/fork
  behavior, and Plasma CPU. Treat regressions as blockers unless accepted.
- **P7.5** Soak malformed/missing service conditions and verify bounded logs.

### Gate P7

Mandatory hardware/fixture matrix complete, no crash/leak/busy-loop, no material
performance regression, and all deviations explicitly resolved.

## Phase 8 — Cutover and stabilization

**Objective:** make Rust authoritative, then remove Python safely.

### Steps

- **P8.1** Switch package/service `/usr/bin/pirostats` to Rust binary with a tagged
  rollback point.
- **P8.2** Observe one release/stabilization window while retaining Python oracle
  source and tests but not running Python in production.
- **P8.3** Fix parity defects in Rust; do not patch QML to hide backend errors.
- **P8.4** Remove Python runtime source/dependencies only after acceptance.
- **P8.5** Promote/adapt Rust tests, refresh docs, archive oracle fixtures and
  migration plans as historical evidence.

### P8.2 acceptance

The stabilization window closes only when the integration owner records one
live installed Plasma session covering service restart, panel and tooltip
refresh, click, wheel paging, pinning, config/style hot reload, and clean
shutdown. Evidence must include the installed mode and environment, relevant
journal excerpts, defects found and resolved, and rollback status. D005/D006
remain explicit exceptions rather than silent skips. User approval closes P8.2;
there is no fixed calendar duration independent of that evidence.

### Final gate

- Clean install/upgrade/uninstall tested.
- Rust aggregate checks and applet integration checks pass.
- All `INVENTORY.md` entries resolved with evidence.
- No temporary compatibility process, dead Python path, or dual source of truth.
- README/DESIGN/PERFORMANCE accurately describe Rust implementation.
