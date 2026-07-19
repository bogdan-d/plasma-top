# Source and callable inventory

This ledger prevents accidental omission. Boxes are closed only by the integration owner after reviewing a lane handoff and rerunning cited evidence. `Disposition` describes final migration handling, not current status.

Agent tooling (`.agents/`, `scripts/`, `skills-lock.json`) and this `plans/` directory are excluded from product migration inventory. They are not shipped PiroStats behavior.

Evidence codes: **U** unit, **D** Python/Rust differential, **F** fault injection, **I** integration/process, **L** live hardware, **P** preserve existing assertion, **E0–E5** exactness levels from `TESTING.md`.

## File inventory

| Done | Current file | Disposition | Lane | Required verification |
|---|---|---|---|---|
| [x] | `.gitignore` | update only for generated Rust artifacts | `SCAFFOLD/QML-VERIFY` | ignore audit (Phase 1: `rust/target/` added; QML-VERIFY re-audits at Gate 6) |
| [ ] | `CLAUDE.md` | update after cutover | `CUTOVER` | link/content audit |
| [ ] | `LICENSE` | preserve | `PACKAGING` | license/package audit |
| [ ] | `NOTICE` | preserve | `PACKAGING` | license/package audit |
| [ ] | `README.md` | update after cutover | `CUTOVER` | link/content audit |
| [ ] | `config/config.toml` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [ ] | `config/machines.toml` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [ ] | `docs/DESIGN.md` | update after cutover | `CUTOVER` | link/content audit |
| [ ] | `docs/ITEMS.md` | update after cutover | `CUTOVER` | link/content audit |
| [ ] | `docs/LAYOUT.md` | update after cutover | `CUTOVER` | link/content audit |
| [ ] | `docs/PERFORMANCE.md` | update after cutover | `CUTOVER` | link/content audit |
| [ ] | `install.sh` | modify for native binary | `PACKAGING` | package/install/upgrade/uninstall |
| [ ] | `lang/en.toml` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [ ] | `packaging/aur/PKGBUILD` | modify for native binary | `PACKAGING` | package/install/upgrade/uninstall |
| [ ] | `packaging/aur/pirostats.install` | modify for native binary | `PACKAGING` | package/install/upgrade/uninstall |
| [ ] | `pirostats` | replace | `DAEMON-CLI` | CLI process matrix |
| [ ] | `plasmoid/.gitignore` | update only for generated Rust artifacts | `SCAFFOLD/QML-VERIFY` | ignore audit |
| [ ] | `plasmoid/LICENSE` | preserve/review | `INTEGRATION` | file-specific inspection |
| [ ] | `plasmoid/package/contents/config/config.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/config/main.xml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/icons/pirostats.svg` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/config/ConfigAppearance.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/CheckBox.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/ColorField.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/ComboBox.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/FontFamily.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/FormKCM.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/Heading.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/SpinBox.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/TextAlign.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/TextField.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/TextFormat.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/libconfig/VertAlign.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/contents/ui/main.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `plasmoid/package/metadata.json` | preserve; edit only if approved | `QML-VERIFY` | T6 visual/interaction + package manifest |
| [ ] | `ruff.toml` | retain until Python removal | `BASE/CUTOVER` | ruff gate |
| [x] | `rust-toolchain.toml` | new; pin stable Rust + clippy/rustfmt components | `SCAFFOLD` | P1.1 toolchain shell + toolchain present in CI |
| [x] | `rust/Cargo.lock` | new; committed per parity plan | `SCAFFOLD` | P1.1 lockfile present + `cargo fetch --locked` no-op |
| [x] | `rust/Cargo.toml` | new; single crate metadata, `test-support` feature | `SCAFFOLD` | P1.1 + P1.2 feature gate; integration-owner path after freeze |
| [x] | `rust/DEPENDENCIES.md` | new; per-dep review ledger | `SCAFFOLD` | P1.4 baseline row + policy fields for future lanes |
| [x] | `rust/rustfmt.toml` | new; rustfmt policy | `SCAFFOLD` | P1.1 fmt gate green |
| [x] | `rust/src/lib.rs` | new; composition root, crate lint attrs | `SCAFFOLD` | P1.2 lint/fmt/clippy/test/doc green; integration-owner path after freeze |
| [x] | `rust/src/main.rs` | new; thin binary entry | `SCAFFOLD` | P1.2 delegating shell; deny `unsafe_code` |
| [x] | `rust/src/error.rs` | new; top-level user-facing error context | `SCAFFOLD` | P1.3 typed `Error` enum + `Result` alias |
| [x] | `rust/src/cli.rs` | new; scaffold-only CLI contract for `daemon`/`render`/`probe`/`profiling`/`list-items`/`page`/`click` | `SCAFFOLD` | P1.3 command names/choices mirror Python `src/daemon.py` |
| [x] | `rust/src/domain/mod.rs` | new; domain composition map | `SCAFFOLD` | P1.3 frozen re-exports |
| [x] | `rust/src/domain/form.rs` | new; `Form`/`Shape`/`Surface`/`SurfaceSet` contracts | `SCAFFOLD` | P1.3 mirrors `src/forms.py`; invariant tests |
| [x] | `rust/src/domain/metric.rs` | new; `Metric`/`MetricSpec`/`Capability` contracts | `SCAFFOLD` | P1.3 mirrors `src/metrics.py` + capability map |
| [x] | `rust/src/domain/item.rs` | new; validated `metric[:form]` `ItemToken` | `SCAFFOLD` | P1.3 token rules mirror `src/registry.py` |
| [x] | `rust/src/domain/registry.rs` | new; token/capability derivation layer (`parse`/`unknown_item_names`/`misplaced_items`/`needed_capabilities`/`SEPARATOR_ITEMS`/`list_items`) | `DOMAIN` | P2 mirrors token+capability half of `src/registry.py`; 51-row `list-items` corpus + 51×2 misplaced matrix |
| [x] | `rust/src/domain/boundary.rs` | new; production boundary contracts (`CommandRunner`/`DbusFacade`/`BoundaryError`) plus command/D-Bus payloads, clock, and filesystem roots | `INTEGRATION` | P4 contract slice: promoted traits out of feature-gated `test_support`; fixture fakes implement the production traits |
| [x] | `rust/src/domain/readings.rs` | new; typed aggregate hardware/readings contracts (`HardwareSnapshot`, `ReadingsSnapshot`, batteries, load, process rows, SMART identity) | `INTEGRATION` | P4 contract slice: replaces placeholder capability sets with formatter/collector-ready typed models |
| [x] | `rust/src/domain/state.rs` | new; typed aggregate mutable daemon/cache state (`DaemonStateSnapshot`, caches, timed values, rate state, GPU cache) | `INTEGRATION` | P4 contract slice: replaces placeholder state with typed cross-poll mutation contract |
| [x] | `rust/src/render/mod.rs` | new; render composition and public API | `RENDER-CORE` | P3 module registration + documented re-exports |
| [x] | `rust/src/render/model.rs` | new; cells/rows/blocks, thresholds, grouping, inline HTML | `RENDER-CORE` | P3 unit tests + fixed Python byte corpus + no-table invariant |
| [x] | `rust/src/render/mono.rs` | new; five-plan table-free monospace serializer | `RENDER-CORE` | P3 unit/width sweep + fixed Python byte corpus covering every plan |
| [x] | `rust/src/render/traces.rs` | new; bar/column/spark/braille encodings + standalone/combo rows | `TRACES` | P3 ports `src/traces.py`; 12 focused tests + fixed Python byte corpus + combo-row structure parity |
| [x] | `rust/src/render/cells.rs` | new; formatter shared helpers for labels/ellipsis/disk text/separator normalization | `FORMATTER` | P4 helper parity via Rust formatter suite + shipped goldens |
| [x] | `rust/src/render/registry.rs` | new; formatter-side token resolution, CSS form tokens, trace-metric mapping, and hardware gates | `FORMATTER` | P4 gate parity via Rust formatter suite + shipped goldens |
| [x] | `rust/src/render/formatter.rs` | new; main panel/tooltip formatter, item dispatch, canonical width, and formatter-owned irregular rows | `FORMATTER` | P4 byte-identical panel H/V + tooltip goldens, canonical-width guard, and mapped Python formatter oracle |
| [x] | `rust/src/render/chart.rs` | new; deterministic tooltip graph PNG rasterizer (grid/labels/fill/line/overlay) and PNG encode/decode test corpus | `CHART` | P4 decoded-pixel parity against `src/chart.py` for empty/overlay/single/constant corpora + PNG chunk/CRC round-trip |
| [x] | `rust/src/sensors/mod.rs` | new; sensor composition map | `SENSOR-CPU` | P3 module registration for incremental sensor lanes |
| [x] | `rust/src/sensors/cpu.rs` | new; CPU discovery, `/proc/stat` diffs, uptime/loadavg, cpufreq/turbo, and per-core histories | `SENSOR-CPU` | P3 ports CPU-owned pieces of `src/sensors.py`; 17 focused tests cover first/delta/reset/malformed/history/discovery/fallback |
| [x] | `rust/src/sensors/memory.rs` | new; `/proc/meminfo` memory/swap readers, total-memory helper, and bounded memory history | `SENSOR-MEM` | P3 ports memory-owned pieces of `src/sensors.py`; 12 focused tests cover direct/fallback/zero/clamp/malformed/history/swap/rounding |
| [x] | `rust/src/sensors/network.rs` | new; route/device detection, wifi identity/signal, sysfs byte rates, and bounded network history | `SENSOR-NET` | P3 ports network-owned pieces of `src/sensors.py`; 11 focused tests cover `ip` fallback, wired/wireless paths, TTL caching, interface-switch/counter-reset rate resets, and graph-history trimming |
| [x] | `rust/src/sensors/hwmon.rs` | new; shared hwmon directory/spec/int helpers for disk-owned sensor paths | `SENSOR-DISK` | P3 ports the disk lane's generic hwmon helpers; 3 focused tests cover substring matching, manual spec resolution, and parse failures |
| [x] | `rust/src/sensors/disk.rs` | new; mount resolution, statvfs usage, block-device identity/topology, hwmon disk/fan caches, and `/proc/diskstats` byte rates | `SENSOR-DISK` | P3 ports disk-owned pieces of `src/sensors.py`; 17 focused tests cover mount filters, NVMe/SCSI labels, partition stacks, TTL caching, rate resets, and df-style usage math |
| [x] | `rust/src/runtime/mod.rs` | new; runtime path resolution (`runtime_dir`/`state_dir`/accessors) + `ensure_dirs` | `RUNTIME` | P2 ports `src/runtime.py`; lazy per-call path resolution for testability |
| [x] | `rust/src/runtime/atomic.rs` | new; `write_atomic` primitive (PID-unique tmp + rename-over) | `RUNTIME` | P2 ports `src/daemon.py:_write_atomic` shape; atomicity + tmp-cleanup tests |
| [x] | `rust/src/runtime/page.rs` | new; page counter (`read_page`/`set_page`/`npages`/`step_page`/`PageDirection`) with flock | `RUNTIME` | P2 ports `src/pagestate.py`; 32-thread concurrency test proves no lost updates |
| [x] | `rust/src/config/mod.rs` | new; typed `Config` tree + sub-structs + `load_config` + `apply_canonical_width` + drop guardrails | `CONFIG` | P2 ports `src/config.py` lines 76–376 + 719–735 + 772–848 + 863–885; `domain::registry` consumed directly (no duplicate unknown/misplaced helpers) |
| [x] | `rust/src/config/merge.rs` | new; TOML merge pipeline (`deep_merge_tables`/`resolve_items`/`parse_surface`/`load_toml_at`/`load_machines`) | `CONFIG` | P2 ports `src/config.py` lines 30–67 + 424–456 + 738–769 |
| [x] | `rust/src/config/geometry.rs` | new; `PanelGeometry` + DMI machine detect + appletsrc vertical detect + geom live/cache + auto-fit | `CONFIG` | P2 ports `src/config.py` lines 380–401 + 471–716; every disk-touch fn has `_at`/`_text`/`_with_dmi` test seam |
| [x] | `rust/src/config/assets.rs` | new; asset root resolution (`code_root`/`xdg_dir`/`home_dir`/`shipped_*`) with `PIROSTATS_CODE_ROOT` env override | `CONFIG` | P2 replaces Python's `__file__`-relative resolution with `CARGO_MANIFEST_DIR/..` + env override for packaged installs |
| [x] | `rust/tests/config_default_load.rs` | new; integration test loading shipped `config/config.toml` end-to-end | `CONFIG` | P2 asserts typed fields, threshold vectors, horizontal override, no unknown/misplaced items |
| [x] | `rust/tests/runtime_paths.rs` | new; integration tests for path resolution + `XDG_RUNTIME_DIR` fallback | `RUNTIME` | env mutation serialized via `ENV_GUARD: Mutex<()>` |
| [x] | `rust/tests/runtime_atomic.rs` | new; integration tests for atomic writes | `RUNTIME` | success/failure/cleanup matrix |
| [x] | `rust/tests/runtime_page.rs` | new; integration tests for page counter + concurrency | `RUNTIME` | 32-thread stress + permission-failure path |
| [x] | `rust/src/test_support.rs` | rewritten as module root for new-style `test_support/` directory | `FIXTURES` | P2 re-exports concrete fakes; `lib.rs` `pub mod test_support;` line unchanged |
| [x] | `rust/src/test_support/fixture_root.rs` | new; virtual FS root (`proc`/`sys`/`run` subtrees, `from_env`, `join`) | `FIXTURES` | P2 no host boundaries touched |
| [x] | `rust/src/test_support/fake_clock.rs` | new; deterministic clock (`at`/`advance`/`tick`/`set_advance_step`) | `FIXTURES` | P2 saturating overflow invariants |
| [x] | `rust/src/test_support/fake_command_runner.rs` | new; argv-keyed FIFO replies + `CommandRunner` trait + call trace | `FIXTURES` | P2 distinct-argv isolation + exhausted-queue error |
| [x] | `rust/src/test_support/fake_dbus.rs` | new; signature-keyed D-Bus replies + `DbusFacade` trait + call trace | `FIXTURES` | P2 FIFO order + empty-queue error |
| [x] | `rust/src/test_support/fixture_loader.rs` | new; `load_text`/`load_bytes`/`load_oracle_fixture` + `OracleFixtureRaw` untyped view | `FIXTURES` | P2 typed deserialization deferred to Wave 3/4 |
| [x] | `rust/tests/fixtures/**` | new; 8 sample fixtures (proc/sys text, oracle TOML, cmd JSON, dbus TOML) | `FIXTURES` | P2 mirrors BASE schema; consumed by loader tests |
| [x] | `rust/tests/parity_runner.sh` | new; Python/Rust parity diff stub | `FIXTURES` | P2 exits 77 (skip) until Wave 4 FORMATTER lands `render` |
| [ ] | `screenshots/desktop-black-text.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [ ] | `screenshots/desktop-white-text.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [ ] | `screenshots/graphs.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [ ] | `screenshots/panel-horizontal.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [ ] | `screenshots/panel-vertical.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [ ] | `screenshots/process.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [ ] | `service/pirostats.service` | modify for native binary | `PACKAGING` | package/install/upgrade/uninstall |
| [ ] | `src/__init__.py` | port then remove | `CUTOVER` | symbol + differential parity |
| [ ] | `src/bolt_battery.py` | port then remove | `HID` | symbol + differential parity |
| [x] | `src/chart.py` | port then remove | `CHART` | symbol + differential parity via Rust chart pixel corpus |
| [ ] | `src/config.py` | port then remove | `CONFIG` | symbol + differential parity |
| [ ] | `src/daemon.py` | port then remove | `DAEMON-CLI` | symbol + differential parity |
| [ ] | `src/formatter.py` | port then remove | `FORMATTER` | symbol + differential parity |
| [ ] | `src/forms.py` | port then remove | `DOMAIN` | symbol + differential parity |
| [ ] | `src/items.py` | port then remove | `FORMATTER` | symbol + differential parity |
| [ ] | `src/metrics.py` | port then remove | `DOMAIN` | symbol + differential parity |
| [ ] | `src/mono_render.py` | port then remove | `RENDER-CORE` | symbol + differential parity |
| [ ] | `src/notifier.py` | port then remove | `NOTIFY` | symbol + differential parity |
| [ ] | `src/pages.py` | port then remove | `PAGES` | symbol + differential parity |
| [ ] | `src/pagestate.py` | port then remove | `RUNTIME` | symbol + differential parity |
| [ ] | `src/registry.py` | port then remove | `DOMAIN/FORMATTER` | symbol + differential parity |
| [ ] | `src/render_model.py` | port then remove | `RENDER-CORE` | symbol + differential parity |
| [ ] | `src/runtime.py` | port then remove | `RUNTIME` | symbol + differential parity |
| [ ] | `src/sensors.py` | port then remove | `SENSOR-*/COLLECTOR` | symbol + differential parity |
| [ ] | `src/traces.py` | port then remove | `TRACES` | symbol + differential parity |
| [ ] | `src/units.py` | port then remove | `DOMAIN` | symbol + differential parity |
| [ ] | `style/icons.toml` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [ ] | `style/style-dark.css` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [ ] | `style/style-light.css` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [ ] | `style/style-overlay.css` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [ ] | `tests/conftest.py` | retain then port/archive | `BASE/INTEGRATION` | existing assertion mapped to Rust |
| [ ] | `tests/golden/panel_h.html` | preserve oracle | `FORMATTER` | byte snapshot |
| [ ] | `tests/golden/panel_v.html` | preserve oracle | `FORMATTER` | byte snapshot |
| [ ] | `tests/golden/tooltip.html` | preserve oracle | `FORMATTER` | byte snapshot |
| [ ] | `tests/oracle.py` | retain then port/archive | `BASE/INTEGRATION` | oracle fixture/render parity mapped to Rust |
| [ ] | `tests/test_config.py` | retain then port/archive | `BASE/CONFIG` | existing assertion mapped to Rust |
| [ ] | `tests/test_deadcode.py` | retain then port/archive | `BASE/INTEGRATION` | existing assertion mapped to Rust |
| [x] | `tests/test_formatter.py` | retain oracle; mapped to Rust formatter suite | `BASE/FORMATTER` | existing assertion preserved in Python and mapped to Rust formatter coverage |
| [x] | `tests/test_golden_render.py` | retain oracle; mapped to Rust formatter goldens | `BASE/FORMATTER` | existing assertion preserved in Python and mapped to Rust panel/tooltip golden coverage |
| [ ] | `tests/test_inventory.py` | retain then port/archive | `BASE/INTEGRATION` | inventory gate + reporter smoke |
| [ ] | `tests/test_items.py` | retain then port/archive | `BASE/DOMAIN` | existing assertion mapped to Rust |
| [ ] | `tests/test_lint.py` | retain then port/archive | `BASE/INTEGRATION` | existing assertion mapped to Rust |
| [ ] | `tests/test_mono_render.py` | retain then port/archive | `BASE/RENDER-CORE` | existing assertion mapped to Rust |
| [ ] | `tests/test_notifier.py` | retain then port/archive | `BASE/NOTIFY` | existing assertion mapped to Rust |
| [ ] | `tests/test_oracle.py` | retain then port/archive | `BASE/INTEGRATION` | oracle fixture/render parity mapped to Rust |
| [ ] | `tests/test_render_model.py` | retain then port/archive | `BASE/RENDER-CORE` | existing assertion mapped to Rust |
| [x] | `tests/test_sensors.py` | retain then port/archive | `BASE/SENSOR-DISK` | existing mount-resolution assertions mapped to Rust disk tests; Python baseline still runs 4/4 |
| [ ] | `tests/vulture_whitelist.py` | retain then port/archive | `BASE/INTEGRATION` | existing assertion mapped to Rust |
| [ ] | `tools/demo_shot.py` | preserve/update invocation | `BASE/QML-VERIFY` | tool smoke + target parity |
| [ ] | `tools/inventory_ast_reporter.py` | preserve/update invocation | `BASE/INTEGRATION` | tool smoke + exact inventory gate |
| [ ] | `tools/manual_tooltip_preview.py` | preserve/update invocation | `BASE/QML-VERIFY` | tool smoke + target parity |
| [ ] | `tools/qt_shot.py` | preserve/update invocation | `BASE/QML-VERIFY` | tool smoke + target parity |
| [ ] | `uninstall.sh` | modify for native binary | `PACKAGING` | package/install/upgrade/uninstall |

## Production Python callable inventory

Every top-level function/class and class method under `src/`, plus the root entry point, is listed. Nested local functions/closures are covered through their enclosing callable branch/call-edge ledger generated in Phase 0.

### `src/__init__.py`

- [ ] No declared callable: verify module constants/import/entry behavior and final disposition.

### `src/bolt_battery.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 14 | `_load_hidapi` | function | `HID` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 39 | `_bolt_hidraw` | function | `HID` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 59 | `_xfer` | function | `HID` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 75 | `_get_feature_idx` | function | `HID` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 83 | `_get_battery` | function | `HID` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 93 | `_get_name` | function | `HID` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 109 | `query` | function | `HID` | U/D/F/L: fixture formula, call trace, failures, live where available |

### `src/chart.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 57 | `_encode_png` | function | `CHART` | U/D: mapped to `rust/src/render/chart.rs:encode_png`; Rust tests validate PNG chunk order, CRCs, scanline filter bytes, and decoded round-trip |
| [x] | 76 | `area_chart_png` | function | `CHART` | U/D: mapped to `rust/src/render/chart.rs:area_chart_png`; Rust tests pin Python-oracle decoded-pixel CRCs + sampled RGBA pixels for empty/overlay/single/constant corpora |

### `src/config.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 34 | `default_config_path` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 43 | `resolve_style` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 52 | `user_machines_path` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 60 | `_deep_merge` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 70 | `_from_dict` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 86 | `DisplayConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 98 | `PagesConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 111 | `BarConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 134 | `SparkConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 144 | `BrailleConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 154 | `ColumnConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 188 | `Section` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 195 | `Surface` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 205 | `Surface.has` | method | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 210 | `Surface.item_set` | method | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 215 | `ThresholdConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 251 | `NotifyThresholds` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 272 | `NotificationConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 286 | `SensorOverrides` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 300 | `DiskConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 318 | `BatteryConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 328 | `SystemUpdatesConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 336 | `ServerCheckConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 344 | `Config` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 380 | `detect_machine` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 406 | `_build_section` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 410 | `_load_toml_at` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 424 | `_resolve_items` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 440 | `_parse_surface` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 508 | `PanelGeometry` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 523 | `_parse_kde_ini` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 540 | `_int_or_none` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 553 | `_detect_vertical_from_appletsrc` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 570 | `_parse_geom` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 592 | `_read_geom_file` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 611 | `cache_live_geom` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 629 | `detect_panel_geometry` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 651 | `detect_vertical_layout` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 657 | `_auto_fit_panel` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 719 | `apply_canonical_width` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 738 | `machines_path_for` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 745 | `machine_source_paths` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 755 | `_load_machines` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 772 | `load_config` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 851 | `_drop_items` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 863 | `_drop_unknown_items` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [ ] | 873 | `_drop_misplaced_items` | function | `CONFIG` | U/D: direct + Python differential; boundaries |

### `src/daemon.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 55 | `_css_path_for` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 65 | `_parse_rgb` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 74 | `_window_bg` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 102 | `_plasma_is_light` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 122 | `_read_css_file` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 138 | `_overlay_css_path` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 147 | `_read_css` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 160 | `_strip_html` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 200 | `_mtime` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 207 | `_write_atomic` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 213 | `_render_page` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 237 | `_render_tooltip` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 244 | `_publish_pages` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 252 | `_cleanup` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 263 | `_warmed_readings` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 278 | `run_probe` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 312 | `_tooltip_html_for_render` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 324 | `run_render` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 396 | `_print_timings` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 413 | `_print_cache_state` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 460 | `run_profile` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 610 | `_log_boot_ready` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 628 | `run_daemon` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 824 | `run_list_items` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 853 | `run_page` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 865 | `run_click` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [ ] | 877 | `main` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |

### `src/formatter.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 66 | `_net_fmt` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 75 | `_maxed_readings` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 123 | `_separator_size` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 140 | `_normalize_separators` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 165 | `PanelFormatter` | class | `FORMATTER` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 166 | `PanelFormatter.__init__` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 178 | `PanelFormatter.format_panel` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 219 | `PanelFormatter._wrap_tooltip` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 227 | `PanelFormatter.format_page` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 232 | `PanelFormatter.format_cpu_cores` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 271 | `PanelFormatter.format_top_process` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 328 | `PanelFormatter._graph_val` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 336 | `PanelFormatter._gpu_graph` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 349 | `PanelFormatter.format_graphs` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 423 | `PanelFormatter.canonical_width` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 448 | `PanelFormatter._canonical_sig` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 458 | `PanelFormatter.format_tooltip` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 478 | `PanelFormatter._build_entries` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 534 | `PanelFormatter._available` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 546 | `PanelFormatter._render_item` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 556 | `PanelFormatter._label_cell` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 573 | `PanelFormatter._battery_sys_is_full` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 577 | `PanelFormatter._battery_sys_icon` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 606 | `PanelFormatter._middle_ellipsis` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 620 | `PanelFormatter._disk_label` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 632 | `PanelFormatter._disk_smart_icon` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 638 | `PanelFormatter._disk_smart_class` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 647 | `PanelFormatter._fmt_disk_space` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 668 | `PanelFormatter._hd_label` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 678 | `PanelFormatter._pair_grid` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 728 | `PanelFormatter._disk_smart_pair` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 748 | `PanelFormatter._hd_temp_pair` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 765 | `PanelFormatter._fan_speed_pair` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 778 | `PanelFormatter._string_row` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 790 | `PanelFormatter._wifi_signal` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 802 | `PanelFormatter._net_device_ip` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 813 | `PanelFormatter._wifi_ssid_signal` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 831 | `PanelFormatter._fmt_freq` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 841 | `PanelFormatter._uptime` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 851 | `PanelFormatter._load_avg` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 871 | `PanelFormatter._top_process` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 889 | `PanelFormatter._dual_rate_rows` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 912 | `PanelFormatter._net_speed` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 916 | `PanelFormatter._disk_io` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 920 | `PanelFormatter._battery_sys` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 968 | `PanelFormatter._battery_periph` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 986 | `PanelFormatter._system_updates` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 993 | `PanelFormatter._server_check` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |

### `src/forms.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 25 | `Shape` | class | `DOMAIN` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 34 | `Surface` | class | `DOMAIN` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 47 | `Form` | class | `DOMAIN` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 83 | `form_from_token` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |

### `src/items.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 40 | `row` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 48 | `per` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 63 | `label` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 80 | `value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 111 | `spark` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 123 | `braille` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 137 | `freq_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 146 | `turbo_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 156 | `turbo_icon` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 177 | `disk_label` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 183 | `disk_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 195 | `disk_space` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 222 | `mem_space` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 239 | `fan_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 256 | `gpu_fan_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 272 | `hd_temp_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 291 | `_thr` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |

### `src/metrics.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 29 | `_ALWAYS` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [ ] | 63 | `Metric` | class | `DOMAIN` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 78 | `_m` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [ ] | 141 | `supports` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [ ] | 150 | `item_surfaces` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |

### `src/mono_render.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 40 | `_pad` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 51 | `_cell_width` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 62 | `_span` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 70 | `_is_title_rule` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 77 | `_Plan` | class | `RENDER-CORE` | U/D: defaults, construction, invariants, round-trip |
| [x] | 91 | `_is_two_pair` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 101 | `_col_widths` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 110 | `_render_cols` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 124 | `_plan_row` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 171 | `_emit` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 210 | `global_width_of` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 225 | `render_blocks_monospace` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |

### `src/notifier.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 26 | `_send` | function | `NOTIFY` | U/D: direct + Python differential; boundaries |
| [ ] | 41 | `Latch` | class | `NOTIFY` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 49 | `NotifState` | class | `NOTIFY` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 64 | `_sustained` | function | `NOTIFY` | U/D: direct + Python differential; boundaries |
| [ ] | 95 | `check_and_notify` | function | `NOTIFY` | U/D: direct + Python differential; boundaries |

### `src/pages.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 38 | `Page` | class | `PAGES` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 82 | `build_pages` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 94 | `_run_command` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 141 | `text_to_mono_html` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 149 | `_text_width` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 156 | `_esc` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 160 | `_ellipsize` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 183 | `_proc_name` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 204 | `_service_for_port` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 219 | `_format_connections` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 270 | `page_inner` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 282 | `title_html` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 291 | `pager_html` | function | `PAGES` | U/D: direct + Python differential; boundaries |
| [ ] | 311 | `default_click` | function | `PAGES` | U/D: direct + Python differential; boundaries |

### `src/pagestate.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 19 | `read_page` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |
| [ ] | 28 | `set_page` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |
| [ ] | 37 | `_npages` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |
| [ ] | 44 | `step_page` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |

### `src/registry.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 37 | `_form_token` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [ ] | 56 | `_historied` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 148 | `render` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 164 | `parse` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [ ] | 181 | `render_item` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [ ] | 189 | `item_gate` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [ ] | 206 | `needed_capabilities` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [ ] | 227 | `unknown_item_names` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [ ] | 233 | `misplaced_items` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |

### `src/render_model.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 23 | `Cell` | class | `RENDER-CORE` | U/D: defaults, construction, invariants, round-trip |
| [x] | 58 | `Ident` | class | `RENDER-CORE` | U/D: defaults, construction, invariants, round-trip |
| [x] | 68 | `Ident.css` | method | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 75 | `visible_width` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 83 | `_nbsp` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 88 | `cell_inner` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 104 | `_val_cell` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 110 | `_aux_cell` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 122 | `_fmt_perc` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 140 | `Separator` | class | `RENDER-CORE` | U/D: defaults, construction, invariants, round-trip |
| [x] | 156 | `Block` | class | `RENDER-CORE` | U/D: defaults, construction, invariants, round-trip |
| [x] | 168 | `css_class_from_thresholds` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 178 | `css_class_active` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 183 | `css_class_battery` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 194 | `_cell_role` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 200 | `_row_shape` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 210 | `group_rows_into_blocks` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 247 | `render_two_pair_row` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 257 | `render_three_col_row` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 272 | `_separator_rule_html` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |
| [x] | 284 | `render_row_inline` | function | `RENDER-CORE` | U/D: direct + Python differential; boundaries |

### `src/runtime.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 30 | `_runtime_dir` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |
| [ ] | 63 | `ensure_dirs` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |

### `src/sensors.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 65 | `_bus` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 78 | `_upower_enumerate` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 93 | `_upower_device_props` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 116 | `timed_section` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 131 | `BatterySys` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 140 | `BatteryPeriph` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 146 | `DiskUsage` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 153 | `HardwareInfo` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 192 | `_BatterySysCache` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 200 | `_BatteryPeriphCache` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 206 | `_NetInfoCache` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 215 | `_RateState` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 224 | `DaemonState` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 294 | `Readings` | class | `SCAFFOLD` | U/D: defaults, construction, invariants, round-trip |
| [ ] | 359 | `discover_hardware` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 383 | `rescan_peripherals` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 402 | `needs_periph_rescan` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 420 | `collect` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 606 | `_cached_by_label` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 622 | `_read_hd_temp_cached` | function | `SENSOR-DISK` | U/F: Rust `read_hd_temp_cached` TTL cache mirrors Python label-keyed behavior |
| [x] | 633 | `_read_fan_speed_cached` | function | `SENSOR-DISK` | U/F: Rust `read_fan_speed_cached` TTL cache mirrors Python label-keyed behavior |
| [x] | 640 | `_hwmon_find` | function | `SENSOR-DISK` | U/F: Rust `hwmon::hwmon_dirs_matching` preserves case-insensitive `name` substring discovery |
| [ ] | 656 | `_resolve_sensor` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 666 | `_read_path_millideg` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 676 | `_read_path_int` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 687 | `_find_cpu_temp` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 698 | `_find_cpu_freq_path` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 704 | `_find_hd_temps` | function | `SENSOR-DISK` | U/F: Rust `find_hd_temp_paths` covers override precedence plus NVMe/drivetemp autodetect |
| [x] | 730 | `_resolve_nvme_namespace` | function | `SENSOR-DISK` | U/F: Rust `resolve_nvme_namespace` maps controller labels to first namespace with fallback |
| [x] | 744 | `_hwmon_device_label` | function | `SENSOR-DISK` | U/F: Rust `hwmon_device_label` preserves NVMe and SCSI-backed disk labels |
| [x] | 770 | `_find_fans` | function | `SENSOR-DISK` | U/F: Rust `find_fan_speed_paths` mirrors numbered override discovery and early stop semantics |
| [x] | 785 | `_token_after` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 795 | `_detect_net_device` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 811 | `_is_wireless` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 815 | `_dbm_to_pct` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 821 | `_read_net_info` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 855 | `_read_net_info_cached` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 864 | `_resolve_mount_device` | function | `SENSOR-DISK` | U/F: Rust mount-table parsing resolves mountpoint → device basename including escaped mount paths |
| [x] | 875 | `_whole_disk_of` | function | `SENSOR-DISK` | U/F: Rust `whole_disk_of` preserves partition-parent discovery with mapper fallback |
| [x] | 892 | `_detect_disk_io_device` | function | `SENSOR-DISK` | U/F: Rust `detect_disk_io_device` mirrors mount→whole-disk topology walk |
| [x] | 914 | `_is_rotational` | function | `SENSOR-DISK` | U/F: Rust `is_rotational` preserves kernel queue flag behavior |
| [ ] | 924 | `_udisks_prop` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 940 | `_detect_disks` | function | `SENSOR-DISK` | U/F: Rust `detect_disks` enumerates supported whole disks and preserves rotational classification |
| [ ] | 984 | `_read_disk_smart` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1011 | `_read_disk_smart_cached` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1018 | `_find_battery_sys` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1022 | `_find_peripherals` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1061 | `_detect_cpu_turbo_supported` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1069 | `_detect_has_backlight` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1081 | `_detect_has_wifi` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1091 | `_detect_nvidia` | function | `GPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1102 | `_detect_intel_gpu` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1126 | `_read_cpu_usage` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1168 | `_read_cpu_cores` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1213 | `_read_uptime` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1221 | `_read_load_avg` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1234 | `_mem_total_bytes` | function | `SENSOR-MEM` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1242 | `_read_proc_stat_times` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1286 | `_cmdline_name` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1310 | `_read_top_process_cached` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1324 | `_diff_top_process` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1349 | `_read_top_process` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1360 | `read_top_process_page` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1383 | `_read_mem_usage` | function | `SENSOR-MEM` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1408 | `_sample_gpu_history` | function | `GPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1438 | `_sample_net_history` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1461 | `_read_swap_usage` | function | `SENSOR-MEM` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1468 | `_counter_rate` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1485 | `_read_net_speed` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1496 | `_resolve_mounts` | function | `SENSOR-DISK` | U/F/P: Rust `resolve_mounts` ports all four existing Python mount-resolution assertions |
| [x] | 1518 | `_read_disk_usage` | function | `SENSOR-DISK` | U/F: Rust `read_disk_usage` mirrors df/psutil-style `statvfs` percent plus half-even GiB rounding |
| [x] | 1528 | `_read_disk_io` | function | `SENSOR-DISK` | U/F: Rust `read_disk_io` ports byte-rate diffs with first-sample/device-switch/rollback suppression |
| [x] | 1538 | `_read_cpu_freq` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1548 | `_read_cpu_turbo` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1558 | `_read_brightness` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1574 | `_sysfs_bat_rate` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1584 | `_sysfs_bat_charge_limit` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1604 | `_sysfs_bat_read` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1616 | `_read_battery_sys` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1652 | `_read_battery_periph` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1677 | `_read_battery_bolt` | function | `POWER` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1710 | `_read_intel_gpu_engine_times` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1760 | `_read_intel_gpu_metrics` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1789 | `_read_intel_gpu_metrics_cached` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1802 | `_gpu_cache_ttl` | function | `GPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1809 | `_nvidia_cap` | function | `GPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1818 | `_pynvml_handle_get` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1835 | `_read_nvidia_pynvml` | function | `GPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1859 | `_read_nvidia_smi` | function | `GPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1883 | `_read_nvidia` | function | `GPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1894 | `_read_count_file` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [ ] | 1905 | `_read_server_file` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |

### `src/traces.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 56 | `_surface_cfg` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 64 | `bar_html` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 84 | `column_html` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 102 | `spark_html` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 124 | `_braille_level` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 135 | `_braille_char` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 148 | `braille_html` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 183 | `_bar_layout_width` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 195 | `_standalone` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 204 | `bar_row` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 209 | `column_row` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 213 | `spark_row` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 217 | `braille_row` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 221 | `_bar_history_row` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 245 | `bar_spark_row` | function | `TRACES` | U/D: direct + Python differential; boundaries |
| [x] | 252 | `bar_braille_row` | function | `TRACES` | U/D: direct + Python differential; boundaries |

### `src/units.py`

- [ ] No declared callable: verify module constants/import/entry behavior and final disposition.

### `pirostats`

- [ ] No declared callable: verify module constants/import/entry behavior and final disposition.

## Existing test/tool callable inventory

Existing tests remain oracle evidence until mapped to a passing Rust test or intentionally retained integration check. Fixtures/helpers also require disposition because their assumptions define behavior.

### `tests/conftest.py`

- [ ] No declared callable: preserve/port file-level behavior.

### `tests/oracle.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 33 | `OracleFixture` | class | `BASE/INTEGRATION` | D/I: preserve oracle fixture schema + render parity harness |
| [ ] | 39 | `_runtime_symbols` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 62 | `_maybe_path` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 66 | `_path_map` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 70 | `_load_disk_smart_drives` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 77 | `_load_disk_usage` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 89 | `_load_battery_periph` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 96 | `_load_hardware` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 119 | `_load_readings` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 166 | `load_fixture` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 177 | `deterministic_render_env` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 186 | `render_component` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 204 | `render_fixture` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |
| [ ] | 208 | `main` | function | `BASE/INTEGRATION` | D/I: preserve deterministic oracle fixture/render harness |

### `tests/test_config.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 11 | `test_apply_canonical_width_sets_resolved_width` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 17 | `test_apply_canonical_width_does_not_ratchet` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 24 | `test_apply_canonical_width_floors_at_builtin_minimum` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 30 | `test_apply_canonical_width_ignores_nonpositive` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 39 | `test_deep_merge_override_scalar` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 44 | `test_deep_merge_nested_dicts_merge_recursively` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 51 | `test_deep_merge_does_not_mutate_base` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 57 | `test_deep_merge_dict_replaces_non_dict` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 65 | `test_detect_machine_no_dmi_access_returns_none` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 72 | `test_detect_machine_board_contains_match` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 82 | `test_detect_machine_no_match_returns_none` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 89 | `test_detect_machine_product_contains_match` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 96 | `test_detect_machine_ignores_non_dict_entries` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 105 | `test_resolve_items_plain` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 109 | `test_resolve_items_add_appends_without_dups_preserving_order` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 114 | `test_resolve_items_remove` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 119 | `test_parse_surface_order_drives_sections` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 135 | `test_parse_surface_order_add_appends_section` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 146 | `test_surface_has_and_item_set_empty` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 154 | `test_drop_unknown_items_removes_typos` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 163 | `test_drop_unknown_items_spares_separators` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 175 | `test_drop_misplaced_items_removes_panel_only_from_the_tooltip` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 184 | `test_drop_misplaced_items_removes_tooltip_only_from_the_panel` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 194 | `test_drop_misplaced_items_leaves_a_section_empty_rather_than_absent` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 205 | `test_load_config_missing_path_returns_no_machine` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 211 | `test_load_config_section_schema` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 230 | `test_load_config_machine_items_add` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 251 | `test_load_config_machine_order_add_new_section` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 272 | `test_unknown_item_names_flags_only_unknowns` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 278 | `test_default_config_has_no_unknown_items` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 288 | `test_load_config_warns_on_unknown_item` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 300 | `test_detect_vertical_layout_defaults_vertical_without_appletsrc` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 307 | `test_detect_vertical_layout_reads_panel_edge` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 329 | `_patch_plasma` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 344 | `test_detect_panel_geometry_reads_geom_file` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 352 | `test_detect_panel_geometry_falls_back_to_appletsrc_orientation` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 360 | `test_detect_panel_geometry_ignores_degenerate_geom_file` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 368 | `test_detect_panel_geometry_stale_geom_orientation_uses_appletsrc` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 378 | `test_detect_panel_geometry_defaults_when_unreadable` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 387 | `test_read_geom_falls_back_to_cache_when_live_absent` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 398 | `test_read_geom_prefers_live_over_cache` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 409 | `test_read_geom_none_when_live_absent_and_no_cache` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 416 | `test_cache_live_geom_persists_valid_live` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 427 | `test_cache_live_geom_ignores_degenerate_and_absent` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 443 | `test_auto_fit_panel_derives_knobs_from_geometry` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 460 | `test_auto_fit_bar_height_zero_uses_main_advance` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 471 | `test_auto_fit_horizontal_sizes_column_height` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 481 | `test_auto_fit_noop_when_geometry_unpublished` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 509 | `test_orientation_override_horizontal_picks_column` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 518 | `test_orientation_override_vertical_picks_bar` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [ ] | 527 | `test_column_panel_width_loads` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |

### `tests/test_deadcode.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 24 | `test_no_dead_code` | function | `BASE/INTEGRATION` | P: preserve assertion; map to Rust test |

### `tests/test_formatter.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 9 | `_bare_hw` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 24 | `test_val_cell_no_class_is_plain_val` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 31 | `test_val_cell_with_class_appends_it` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 38 | `test_fmt_perc_panel_caps_at_100_without_percent_sign` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 42 | `test_fmt_perc_tooltip_always_has_percent_sign` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 46 | `test_fmt_perc_below_100_has_percent_sign_either_way` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 53 | `test_net_fmt_zero` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 57 | `test_net_fmt_kilobits` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 61 | `test_net_fmt_megabits` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 67 | `test_disk_label_root_mount` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 71 | `test_disk_label_strips_mnt_prefix` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 75 | `test_disk_label_basename_for_run_media` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 79 | `test_middle_ellipsis_short_string_unchanged` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 83 | `test_middle_ellipsis_keeps_head_and_tail` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 87 | `test_middle_ellipsis_never_exceeds_budget` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 92 | `test_middle_ellipsis_bounds_ssid_to_max` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 97 | `test_net_device_ip_truncates_long_interface` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 105 | `test_string_row_caps_net_device_leaves_ip_raw` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 111 | `_canonical_guard_cfg` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 119 | `test_canonical_width_exceeds_short_content` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 130 | `_guard_full_hw` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 144 | `_guard_readings` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 176 | `_tooltip_tokens` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 191 | `test_canonical_width_covers_every_tooltip_item_guard` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 214 | `test_hd_label_strips_trailing_index` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 218 | `test_hd_label_no_trailing_index` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 222 | `test_hd_label_nvme_namespace_block_device` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 232 | `_fmt` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 236 | `test_std_never_attaches_bar_or_history_for_cpu_usage` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 243 | `test_bar_html_for_empty_when_value_missing` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 249 | `test_bar_html_for_empty_when_width_zero` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 255 | `test_spark_html_for_empty_when_history_missing` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 260 | `test_bar_spark_row_empty_when_only_bar_available` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 269 | `test_bar_spark_row_renders_when_both_available` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 277 | `test_bar_row_and_spark_row_agree_with_bar_spark_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 289 | `_titles` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 298 | `test_available_hw_bound_items_off_on_bare_machine` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 307 | `test_available_unbound_items_always_on` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 315 | `test_available_present_hw_turns_item_on` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 326 | `test_available_battery_periph_via_bolt_config` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 336 | `_surface_cfg` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 349 | `test_empty_section_drops_title_and_separator` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 361 | `test_first_section_has_no_leading_separator` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 369 | `test_panel_has_no_title_rows_and_no_separators` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 380 | `test_hd_temp_row_empty_without_temp` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 389 | `test_top_process_no_padding_to_fixed_count` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 398 | `_hw_disks` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 403 | `test_disk_smart_packs_two_drives_per_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 411 | `test_disk_smart_odd_count_uses_blank_filler` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 421 | `test_disk_smart_single_disk_is_full_width_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 428 | `test_disk_smart_single_result_among_many_is_full_width` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 436 | `test_disk_smart_empty_when_no_results` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 443 | `_hw_hd_temps` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 447 | `test_hd_temp_pair_packs_two_drives_per_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 455 | `test_hd_temp_pair_odd_count_uses_blank_filler` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 464 | `test_hd_temp_pair_single_disk_is_full_width_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 471 | `test_hd_temp_pair_skips_disks_without_temp` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 479 | `test_hd_temp_pair_empty_when_no_temps` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 486 | `_hw_fans` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 490 | `test_fan_speed_pair_two_fans_one_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 499 | `test_fan_speed_pair_odd_count_uses_blank_filler` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 508 | `test_fan_speed_pair_single_fan_is_full_width_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 515 | `test_fan_speed_pair_skips_fans_without_reading` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 523 | `test_fan_speed_pair_empty_when_no_readings` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 528 | `test_disk_smart_empty_when_smart_disabled` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 540 | `_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 543 | `test_normalize_keeps_separator_between_two_rows` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 548 | `test_normalize_drops_leading_and_trailing_separators` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 553 | `test_normalize_collapses_consecutive_keeping_largest` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 558 | `test_normalize_section_edge_separator_becomes_inter_section_gap` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |

### `tests/test_golden_render.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 27 | `_full_hw` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 41 | `_full_readings` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 65 | `_render` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [ ] | 84 | `test_golden_render` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |

### `tests/test_inventory.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 24 | `_run_report` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 39 | `_names` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 43 | `_file_counts` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 54 | `_call_edge_rows` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 82 | `test_inventory_ast_reporter_workspace_smoke` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 135 | `test_inventory_call_edge_table_matches_ast_reporter` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |

### `tests/test_items.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 13 | `_cfg` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 25 | `_where` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 34 | `test_cpu_usage_needs_no_dedicated_sensor` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 39 | `test_item_pulls_its_capability` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 44 | `test_metric_can_need_multiple_capabilities` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 49 | `test_form_does_not_change_the_capability` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 55 | `test_notification_keeps_sensor_alive_without_the_item` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 61 | `test_unknown_token_contributes_nothing` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 65 | `test_gpu_nvidia_metrics_share_one_capability` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 72 | `test_unknown_item_names_flags_bad_metric_and_bad_form` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 80 | `test_value_metrics_live_on_both_surfaces` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 86 | `test_bare_visuals_are_panel_only` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 93 | `test_wide_forms_and_string_metrics_are_tooltip_only` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 100 | `test_misplaced_items_flags_tooltip_only_in_panel` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 108 | `test_misplaced_items_flags_panel_only_in_tooltip` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [ ] | 117 | `test_misplaced_items_ignores_unknown_names` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |

### `tests/test_lint.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 24 | `test_ruff_clean` | function | `BASE/INTEGRATION` | P: preserve assertion; map to Rust test |

### `tests/test_mono_render.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 7 | `_label` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 11 | `_val` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 15 | `_line_widths` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 22 | `test_visible_width_strips_tags_and_decodes_entities` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 30 | `test_plain_blocks_emit_no_table` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 42 | `test_values_share_a_global_right_edge` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 55 | `test_value_sits_at_the_right_edge` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 66 | `test_two_pair_row_splits_into_two_halves` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 77 | `test_separator_small_emits_rule_div` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 86 | `test_separator_big_emits_rule_div` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 94 | `test_no_rule_without_explicit_separator` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 105 | `test_title_is_left_aligned` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 118 | `test_title_rule_is_full_width_bar` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |

### `tests/test_notifier.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 15 | `_Clock` | class | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 17 | `_Clock.__init__` | method | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 20 | `_Clock.__call__` | method | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 23 | `_Clock.advance` | method | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 27 | `_Hw` | class | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 33 | `sent` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 42 | `clock` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 48 | `_cfg` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 58 | `_poll` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 66 | `test_cpu_temp_spike_never_notifies` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 75 | `test_cpu_temp_notifies_once_when_sustained` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 84 | `test_cpu_temp_hysteresis_blocks_rattle` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 96 | `test_cpu_temp_rearms_after_cooling` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 108 | `test_cpu_temp_hold_restarts_on_a_dip` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 118 | `test_cpu_temp_sustain_zero_fires_immediately` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 127 | `test_cpu_temp_notification_off_stays_silent` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 136 | `test_sustained_hold_measures_time_not_polls` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 145 | `test_sustained_fires_once_per_episode` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |
| [ ] | 156 | `test_sustained_without_hysteresis_clears_at_the_trip_point` | function | `BASE/NOTIFY` | P: preserve assertion; map to Rust test |

### `tests/test_oracle.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 15 | `test_oracle_fixture_matches_existing_goldens` | function | `BASE/INTEGRATION` | P/D: preserve oracle golden assertion; map to Rust differential |

### `tests/test_render_model.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 16 | `test_css_class_below_mid_is_good` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 20 | `test_css_class_at_mid_boundary_is_warn` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 24 | `test_css_class_at_high_boundary_is_crit` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 28 | `test_css_class_above_high_is_crit` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 34 | `test_css_class_active_above_threshold` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 38 | `test_css_class_active_at_or_below_threshold` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 44 | `test_css_class_battery_low_charge_is_crit` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 48 | `test_css_class_battery_mid_charge_is_warn` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 52 | `test_css_class_battery_high_charge_is_good` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 58 | `_row` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 62 | `test_consecutive_same_shape_rows_form_one_block` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 68 | `test_separator_splits_into_two_blocks` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 75 | `test_shape_change_splits_without_explicit_separator` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 82 | `test_spanning_row_gets_its_own_block` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 97 | `test_same_cell_count_but_different_roles_splits` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 113 | `test_same_role_pattern_merges_even_with_different_state_classes` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 120 | `test_separator_marks_following_block_with_its_size` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 126 | `test_shape_change_does_not_set_separator_size` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 131 | `test_leading_and_trailing_separators_produce_no_empty_block` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 136 | `test_empty_input_produces_no_blocks` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 142 | `test_render_row_inline_no_table_tags` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 151 | `test_render_row_inline_separates_multi_cell_rows` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |
| [x] | 160 | `test_render_row_inline_reserves_min_width_fixed_footprint` | function | `BASE/RENDER-CORE` | P: preserve assertion; map to Rust test |

### `tests/test_sensors.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 5 | `_Part` | class | `BASE/SENSOR-DISK` | P: fixture shape retained by the four passing Python assertions and mapped Rust mount tests |
| [x] | 7 | `_Part.__init__` | method | `BASE/SENSOR-DISK` | P: fixture construction retained by the four passing Python assertions and mapped Rust mount tests |
| [x] | 13 | `test_resolve_mounts_explicit_list_used_as_is` | function | `BASE/SENSOR-DISK` | P/U: mapped to Rust `resolve_mounts_explicit_list_used_as_is` |
| [x] | 19 | `test_resolve_mounts_auto_filters_to_roots_and_orders` | function | `BASE/SENSOR-DISK` | P/U: mapped to Rust `resolve_mounts_auto_filters_to_roots_and_orders` |
| [x] | 33 | `test_resolve_mounts_auto_root_only_when_nothing_mounted` | function | `BASE/SENSOR-DISK` | P/U: mapped to Rust `resolve_mounts_auto_root_only_when_nothing_under_auto_roots` |
| [x] | 40 | `test_resolve_mounts_auto_ignores_bare_root_dir` | function | `BASE/SENSOR-DISK` | P/U: mapped to Rust `resolve_mounts_auto_ignores_bare_root_dirs` |

### `tests/vulture_whitelist.py`

- [ ] No declared callable: preserve/port file-level behavior.

### `tools/demo_shot.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 34 | `_demo_hw` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |
| [ ] | 51 | `_demo_readings` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |
| [ ] | 76 | `main` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |

### `tools/inventory_ast_reporter.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 39 | `CallContext` | class | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 44 | `_display_path` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 51 | `_read_source` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 56 | `_callee_text` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 63 | `_callee_key` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 79 | `_iter_targets` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 107 | `FileAnalyzer` | class | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 108 | `FileAnalyzer.__init__` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 115 | `FileAnalyzer.analyze` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 140 | `FileAnalyzer._visit_module_stmt` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 149 | `FileAnalyzer._visit_recorded_class` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 164 | `FileAnalyzer._visit_recorded_function` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 178 | `FileAnalyzer._visit_class_header` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 188 | `FileAnalyzer._visit_function_header` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 197 | `FileAnalyzer._record_callable` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 220 | `FileAnalyzer._context` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 227 | `FileAnalyzer._current_context` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 230 | `FileAnalyzer.visit_FunctionDef` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 235 | `FileAnalyzer.visit_AsyncFunctionDef` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 240 | `FileAnalyzer.visit_ClassDef` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 245 | `FileAnalyzer.visit_Call` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 265 | `analyze_file` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 272 | `build_report` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 312 | `build_parser` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [ ] | 337 | `main` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |

### `tools/manual_tooltip_preview.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 35 | `_val` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |
| [ ] | 40 | `build_entries` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |
| [ ] | 75 | `main` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |

### `tools/qt_shot.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 88 | `_qml_for_html` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |
| [ ] | 103 | `main` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |

## QML callable and signal-handler inventory


### `plasmoid/package/contents/config/config.qml`

- [ ] Declarative-only file: T6 load/bind/config visual verification.

### `plasmoid/package/contents/ui/config/ConfigAppearance.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 15 | `onDesktop` — `readonly property bool onDesktop: plasmoid.formFactor === PlasmaCore.Types.Planar` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/CheckBox.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 11 | `onClicked` — `onClicked: plasmoid.configuration[configKey] = !plasmoid.configuration[configKey]` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/ColorField.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 19 | `onTextChanged` — `onTextChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 52 | `onValueChanged` — `onValueChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 90 | `onClicked` — `onClicked: dialogLoader.active = true` | QML-VERIFY T6 event/interaction path |
| [ ] | 137 | `onSelectedColorChanged` — `onSelectedColorChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 142 | `onAccepted` — `onAccepted: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 146 | `onRejected` — `onRejected: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 154 | `Component.onCompleted` — `Component.onCompleted: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/ComboBox.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 21 | `onPopulate` — `onPopulate: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 36 | `onConfigValueChanged` — `onConfigValueChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 58 | `Component.onCompleted` — `Component.onCompleted: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 63 | `onCurrentIndexChanged` — `onCurrentIndexChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 75 | `size` — `function size() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 87 | `findValue` — `function findValue(val) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 96 | `selectValue` — `function selectValue(val) {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/FontFamily.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 17 | `isMonospace` — `function isMonospace(family) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 22 | `onPopulate` — `onPopulate: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/FormKCM.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 24 | `Window.onWindowChanged` — `Window.onWindowChanged: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/Heading.qml`

- [ ] Declarative-only file: T6 load/bind/config visual verification.

### `plasmoid/package/contents/ui/libconfig/SpinBox.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 41 | `onValueRealChanged` — `onValueRealChanged: serializeTimer.start()` | QML-VERIFY T6 event/interaction path |
| [ ] | 90 | `onTriggered` — `onTriggered: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 129 | `onActiveFocusChanged` — `onActiveFocusChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 134 | `selectValue` — `function selectValue() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 143 | `fixMinus` — `function fixMinus(str) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 155 | `fixDecimals` — `function fixDecimals(str) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 162 | `fixText` — `function fixText(str) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 166 | `onTextEdited` — `function onTextEdited() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 197 | `bindContentItem` — `function bindContentItem() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 210 | `onContentItemChanged` — `onContentItemChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 214 | `Component.onCompleted` — `Component.onCompleted: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/TextAlign.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 13 | `setValue` — `function setValue(val) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 20 | `updateChecked` — `function updateChecked() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 28 | `Component.onCompleted` — `Component.onCompleted: updateChecked()` | QML-VERIFY T6 event/interaction path |
| [ ] | 34 | `onClicked` — `onClicked: setValue(Text.AlignLeft)` | QML-VERIFY T6 event/interaction path |
| [ ] | 41 | `onClicked` — `onClicked: setValue(Text.AlignHCenter)` | QML-VERIFY T6 event/interaction path |
| [ ] | 48 | `onClicked` — `onClicked: setValue(Text.AlignRight)` | QML-VERIFY T6 event/interaction path |
| [ ] | 55 | `onClicked` — `onClicked: setValue(Text.AlignJustify)` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/TextField.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 12 | `onConfigValueChanged` — `onConfigValueChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 19 | `onTextChanged` — `onTextChanged: serializeTimer.start()` | QML-VERIFY T6 event/interaction path |
| [ ] | 30 | `onClicked` — `onClicked: textField.text = defaultValue` | QML-VERIFY T6 event/interaction path |
| [ ] | 42 | `onTriggered` — `onTriggered: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/TextFormat.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 30 | `onClicked` — `onClicked: plasmoid.configuration[configKey] = checked` | QML-VERIFY T6 event/interaction path |
| [ ] | 40 | `onClicked` — `onClicked: plasmoid.configuration[configKey] = checked` | QML-VERIFY T6 event/interaction path |
| [ ] | 50 | `onClicked` — `onClicked: plasmoid.configuration[configKey] = checked` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/VertAlign.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 13 | `setValue` — `function setValue(val) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 20 | `updateChecked` — `function updateChecked() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 27 | `Component.onCompleted` — `Component.onCompleted: updateChecked()` | QML-VERIFY T6 event/interaction path |
| [ ] | 33 | `onClicked` — `onClicked: setValue(Text.AlignTop)` | QML-VERIFY T6 event/interaction path |
| [ ] | 40 | `onClicked` — `onClicked: setValue(Text.AlignVCenter)` | QML-VERIFY T6 event/interaction path |
| [ ] | 47 | `onClicked` — `onClicked: setValue(Text.AlignBottom)` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/main.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [ ] | 24 | `onNewData` — `onNewData: (sourceName, data) => {` | QML-VERIFY T6 event/interaction path |
| [ ] | 32 | `exec` — `function exec(cmd) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 52 | `execOnce` — `function execOnce(cmd) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 57 | `performClick` — `function performClick() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 61 | `performMouseWheelUp` — `function performMouseWheelUp() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 65 | `performMouseWheelDown` — `function performMouseWheelDown() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 81 | `wheelStep` — `function wheelStep(delta) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 100 | `onTriggered` — `onTriggered: widget.wheelInGesture = false` | QML-VERIFY T6 event/interaction path |
| [ ] | 149 | `resetState` — `function resetState(state) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 155 | `parseAnsiCode` — `function parseAnsiCode(n, i, tokens, state) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 176 | `formatHexInt` — `function formatHexInt(n) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 185 | `rgbToHex` — `function rgbToHex(r, g, b) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 188 | `parseColorMode` — `function parseColorMode(i, tokens) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 218 | `parseAnsiEscape` — `function parseAnsiEscape(codes, state) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 258 | `desktopRecolor` — `function desktopRecolor(html, color) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 278 | `formatOutputText` — `function formatOutputText(stdout) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 319 | `onExited` — `function onExited(cmd, exitCode, exitStatus, stdout, stderr) {` | QML-VERIFY T6 event/interaction path |
| [ ] | 341 | `runCommand` — `function runCommand() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 346 | `runTooltipCommand` — `function runTooltipCommand() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 376 | `onDataChanged` — `onDataChanged: readDebounce.restart()` | QML-VERIFY T6 event/interaction path |
| [ ] | 377 | `onRowsInserted` — `onRowsInserted: readDebounce.restart()` | QML-VERIFY T6 event/interaction path |
| [ ] | 386 | `onTriggered` — `onTriggered: widget.readOutputs()` | QML-VERIFY T6 event/interaction path |
| [ ] | 389 | `readOutputs` — `function readOutputs() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 399 | `Component.onCompleted` — `Component.onCompleted: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 408 | `Plasmoid.onActivated` — `Plasmoid.onActivated: widget.performClick()` | QML-VERIFY T6 event/interaction path |
| [ ] | 413 | `onExpandedChanged` — `onExpandedChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 479 | `onLinkActivated` — `onLinkActivated: Qt.openUrlExternally(link)` | QML-VERIFY T6 event/interaction path |
| [ ] | 507 | `onIsVerticalChanged` — `onIsVerticalChanged: output.publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [ ] | 522 | `onItemWidthChanged` — `// onItemWidthChanged: console.log('itemWidth', itemWidth, 'implicitWidth', output.implicitWidth, 'contentWidth', output.contentWidth)` | QML-VERIFY T6 event/interaction path |
| [ ] | 537 | `onItemHeightChanged` — `// onItemHeightChanged: console.log('itemHeight', itemHeight, 'implicitHeight', output.implicitHeight, 'contentHeight', output.contentHeight)` | QML-VERIFY T6 event/interaction path |
| [ ] | 550 | `onHoveredChanged` — `onHoveredChanged: {` | QML-VERIFY T6 event/interaction path |
| [ ] | 574 | `onClicked` — `onClicked: (mouse) => {` | QML-VERIFY T6 event/interaction path |
| [ ] | 582 | `onWheel` — `onWheel: (wheel) => {` | QML-VERIFY T6 event/interaction path |
| [ ] | 607 | `onAdvanceWidthChanged` — `onAdvanceWidthChanged: output.publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [ ] | 617 | `onAdvanceWidthChanged` — `onAdvanceWidthChanged: output.publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [ ] | 619 | `publishGeometry` — `function publishGeometry() {` | QML-VERIFY T6 event/interaction path |
| [ ] | 635 | `onWidthChanged` — `onWidthChanged: publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [ ] | 636 | `Component.onCompleted` — `Component.onCompleted: publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [ ] | 643 | `onLinkActivated` — `onLinkActivated: Qt.openUrlExternally(link)` | QML-VERIFY T6 event/interaction path |
| [ ] | 700 | `onDesktop` — `readonly property bool onDesktop: plasmoid.formFactor === PlasmaCore.Types.Planar` | QML-VERIFY T6 event/interaction path |
| [ ] | 705 | `onClicked` — `onClicked: widget.expanded = false   // middle-click again un-pins` | QML-VERIFY T6 event/interaction path |
| [ ] | 713 | `onWheel` — `onWheel: (wheel) => {` | QML-VERIFY T6 event/interaction path |
| [ ] | 774 | `onLinkActivated` — `onLinkActivated: Qt.openUrlExternally(link)` | QML-VERIFY T6 event/interaction path |

## Shell/package callable inventory


### `install.sh`

- [ ] No declared function: shell syntax plus full script scenario test.

### `packaging/aur/PKGBUILD`

- [ ] line 27 `pkgver` — PACKAGING scenario + failure/rollback evidence.
- [ ] line 42 `package` — PACKAGING scenario + failure/rollback evidence.

### `packaging/aur/pirostats.install`

- [ ] line 4 `post_install` — PACKAGING scenario + failure/rollback evidence.
- [ ] line 24 `post_upgrade` — PACKAGING scenario + failure/rollback evidence.
- [ ] line 33 `pre_remove` — PACKAGING scenario + failure/rollback evidence.

### `uninstall.sh`

- [ ] No declared function: shell syntax plus full script scenario test.

## Rust callable inventory

Mirrors the Python ledger for new Rust callables introduced by the migration.
Each entry's `Lane` is the lane that owns the *final* shape; `SCAFFOLD` rows
are contracts that downstream lanes extend. Evidence codes follow the same
legend as the rest of this file (U/D/F/I/L/P + E0–E5).

### `rust/src/lib.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `run` | function | `SCAFFOLD` | U: dispatch tests for `Help`/`Version`/`ScaffoldOnly` |

### `rust/src/error.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `Error` | enum | `SCAFFOLD` | U: variants surface `Cli` + `ScaffoldOnly` |
| [x] | `Result` | alias | `SCAFFOLD` | U: alias used by `run` |

### `rust/src/cli.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `Cli` | struct | `SCAFFOLD` | U: defaults/help/version parse |
| [x] | `Cli::parse` | method | `SCAFFOLD` | U: every command + flag choice; rejects unknown/duplicate/missing |
| [x] | `Command` | enum | `SCAFFOLD` | U: variants match Python CLI contract |
| [x] | `Command::name` | method | `SCAFFOLD` | U: stable command-name strings |
| [x] | `RenderCommand` / `ConfigCommand` / `PageCommand` | structs | `SCAFFOLD` | U: default + override parse |
| [x] | `RenderComponent` / `RenderFormat` / `PanelLayout` / `RenderPage` / `PageDirection` | enums | `SCAFFOLD` | U: choice matrix |
| [x] | `CliError` | enum | `SCAFFOLD` | U: each variant reachable from `parse` |
| [x] | `help_text` | function | `SCAFFOLD` | U: snapshot covers all command names |

### `rust/src/domain/form.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `Form` | enum | `SCAFFOLD`/`DOMAIN` | U: as_str/FromStr round-trip; mirrors `src/forms.py` |
| [x] | `Form::allowed_surfaces` | method | `SCAFFOLD`/`DOMAIN` | U: panel/tooltip gate per `FORM_SURFACES` |
| [x] | `Shape` | enum | `SCAFFOLD`/`DOMAIN` | U: intrinsic shape set |
| [x] | `Surface` | enum | `SCAFFOLD`/`DOMAIN` | U: 3-surface model |
| [x] | `SurfaceSet` | struct | `SCAFFOLD`/`DOMAIN` | U: bitset contains/intersection/empty |
| [x] | `FormParseError` | struct | `SCAFFOLD` | U: error path |

### `rust/src/domain/metric.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `Metric` | enum | `SCAFFOLD`/`DOMAIN` | U: 35 variants + FromStr/as_str |
| [x] | `MetricSpec` | struct | `SCAFFOLD`/`DOMAIN` | U: capabilities/forms/surfaces/intrinsic_shape |
| [x] | `Capability` | enum | `SCAFFOLD`/`DOMAIN` | U: mirrors `src/registry.py` capability set |
| [x] | `Metric::spec`/`supports_form`/`surfaces`/`intrinsic_shape`/`capabilities`/`all` | methods | `SCAFFOLD`/`DOMAIN` | U: per-metric invariants |
| [x] | `MetricParseError` | struct | `SCAFFOLD` | U: unknown token |

### `rust/src/domain/item.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `ItemToken` | struct | `SCAFFOLD`/`DOMAIN` | U: token rules from `src/registry.py` parse |
| [x] | `ItemToken::new`/`metric`/`rendering`/`form`/`effective_surfaces` | methods | `SCAFFOLD`/`DOMAIN` | U: pairing + intersection |
| [x] | `ItemRendering` | enum | `SCAFFOLD`/`DOMAIN` | U: Generic vs Intrinsic |
| [x] | `ItemParseError` | enum | `SCAFFOLD` | U: each error variant reachable |

### `rust/src/domain/registry.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `parse` | function | `DOMAIN` | U: round-trip + separator/invalid rejection; mirrors `src/registry.py:parse` |
| [x] | `unknown_item_names` | function | `DOMAIN` | U: bad metric + bad form flagged; spares separators; mirrors `src/registry.py:unknown_item_names` |
| [x] | `misplaced_items` | function | `DOMAIN` | U: panel-only/tooltip-only matrix; 51×2 exhaustive; mirrors `src/registry.py:misplaced_items` |
| [x] | `needed_capabilities` | function | `DOMAIN` | U: union of metric + notification + graphs-page caps; mirrors `src/registry.py:needed_capabilities` |
| [x] | `notification_capability_map` / `NOTIFY_CAPABILITY_MAP` | function/const | `DOMAIN` | U: 10-key map matches `_NOTIFY_CAPS` |
| [x] | `graphs_page_capabilities` / `GRAPHS_PAGE_CAPABILITIES` | function/const | `DOMAIN` | U: 4-cap special case when `graphs` in pages order |
| [x] | `SEPARATOR_ITEMS` | const | `DOMAIN` | U: keys match `src/render_model.py:SEPARATOR_ITEMS` |
| [x] | `list_items` / `placement_for` | functions | `DOMAIN` | U: 51-row byte-for-byte parity with `pirostats list-items` |

### `rust/src/domain/boundary.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `CommandStatus` / `CommandOutput` | enum/struct | `SCAFFOLD`/`INTEGRATION` | U: shared command payload contract used by production boundaries and fixture fakes |
| [x] | `BusKind` / `DbusOutput` | enum/struct | `SCAFFOLD`/`INTEGRATION` | U: shared D-Bus payload contract used by production boundaries and fixture fakes |
| [x] | `BoundaryError` | enum | `INTEGRATION` | U: promoted shared boundary error contract for command/D-Bus production traits and fixture failures |
| [x] | `CommandRunner` / `DbusFacade` | traits | `INTEGRATION` | U: promoted production boundary traits now implemented by `FakeCommandRunner` and `FakeDbus` |
| [x] | `ClockSnapshot` | struct | `SCAFFOLD`/`FIXTURES` | U: default at zero/UNIX_EPOCH |
| [x] | `FilesystemRoots` | struct | `SCAFFOLD`/`FIXTURES` | U: default + `state_root()` derivation |

### `rust/src/domain/readings.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `BatteryState` | enum | `INTEGRATION` | U: stable token mapping for `"charging"`, `"discharging"`, and `"fully-charged"` |
| [x] | `BatterySystemReading` / `BatteryPeripheralReading` | structs | `INTEGRATION` | U: typed battery aggregates replace preformatted placeholder contracts |
| [x] | `DiskUsageReading` / `LoadAverage` | structs | `INTEGRATION` | U: typed disk-usage and load-average aggregates for formatter/collector lanes |
| [x] | `TopProcessSummary` / `TopProcessDetails` | structs | `INTEGRATION` | U: typed process rows replace tuple-only placeholder contracts |
| [x] | `DiskSmartInterface` / `SmartDisk` | enum/struct | `INTEGRATION` | U: typed SMART identity contract replaces raw tuple placeholders |
| [x] | `HardwareSnapshot` / `ReadingsSnapshot` | structs | `INTEGRATION` | U: default/invariant tests cover typed aggregate replacement for the old placeholder-only capability/metric sets |

### `rust/src/domain/state.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `TimedValue` | struct | `INTEGRATION` | U: empty default captures the shared TTL-cache shape |
| [x] | `BatterySystemCache` / `BatteryPeripheralCache` / `NetworkInfoCache` | structs | `INTEGRATION` | U: typed cached cross-poll state replaces stringly placeholder state |
| [x] | `CounterRateState` / `GpuCache` | structs | `INTEGRATION` | U: typed diff/cache state shared by future collectors |
| [x] | `DaemonStateSnapshot` | struct | `INTEGRATION` | U: default/invariant tests cover typed cross-poll state plus retained page/poll bookkeeping |

### `rust/src/render/model.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `Align` / `Cell` / `Ident` / `SeparatorSize` / `Separator` / `Entry` / `Block` / `Row` | enums/structs/alias | `RENDER-CORE` | U/D: row/cell identity, role, alignment, separator, and grouping corpus mirrors `src/render_model.py` |
| [x] | `visible_width` / `value_cell` / `auxiliary_cell` / `format_percent` | functions | `RENDER-CORE` | U/D: entities, tags, missing values, panel/tooltip percentages, and state-class composition |
| [x] | `css_class_from_thresholds` / `css_class_active` / `css_class_battery` | functions | `RENDER-CORE` | U/D: all threshold boundaries mapped from `tests/test_render_model.py` |
| [x] | `group_rows_into_blocks` | function | `RENDER-CORE` | U/D: shape changes, explicit separators, spanning rows, and empty-edge cases |
| [x] | `render_two_pair_row` / `render_three_col_row` / `render_row_inline` | functions | `RENDER-CORE` | U/D: fixed Python row corpus plus table-free horizontal output |

### `rust/src/render/mono.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `global_width_of` | function | `RENDER-CORE` | U/D: minimum width, full-surface right edge, title-rule exclusion, and layout-width overrides |
| [x] | `render_blocks_monospace` | function | `RENDER-CORE` | U/D: fixed byte corpus covers all five plans; 80-case right-edge sweep; no `<table>` |

### `rust/src/render/cells.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `table_text` / `label_cell` / `regular_label_cell` | functions | `FORMATTER` | U/D: icons/labels/delimiter lookup and panel glyph suppression match `src/items.py` / `src/formatter.py` |
| [x] | `net_fmt` / `middle_ellipsis` / `disk_label` / `hd_label` / `fmt_freq` / `fmt_disk_space` | functions | `FORMATTER` | U/D: helper text formatting matches `tests/test_formatter.py` boundaries and golden HTML |
| [x] | `separator_size` / `normalize_separators` | functions | `FORMATTER` | U/D: explicit separator handling matches `src/formatter.py` and Python oracle tests |

### `rust/src/render/registry.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `ResolvedItem` | struct | `FORMATTER` | U: formatter dispatch carries validated token + resolved CSS form token |
| [x] | `resolve_item` / `form_token` / `trace_metric` | functions | `FORMATTER` | U/D: token→render-form resolution and historied metric mapping match formatter-owned `src/registry.py` behavior |
| [x] | `item_gate` | function | `FORMATTER` | U/D: hardware gates match `src/metrics.py` / `PanelFormatter._available` behavior |

### `rust/src/render/formatter.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `PanelFormatter` | struct | `FORMATTER` | U/D: borrowed config/hardware formatter shell for main panel/tooltip parity |
| [x] | `PanelFormatter::new` / `with_now_unix` / `format_panel` / `format_tooltip` / `canonical_width` | methods | `FORMATTER` | U/D: shipped panel H/V + tooltip goldens, deterministic battery alternation, and canonical-width guard |
| [x] | `PanelFormatter::build_entries` + item-render helper family | methods | `FORMATTER` | U/D: section collapse, titles, separators, regular/irregular rows, paired rows, batteries, dual-rate rows, and formatter-owned dispatch from `src/formatter.py` |

### `rust/src/render/chart.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `RGBA` + palette constants (`GRID`, `LABEL`, `BLUE_*`, `PURPLE_*`, `GREEN_*`, `ORANGE_LINE`, `TEAL_*`, `RED_LINE`) | type alias + consts | `CHART` | U: stable color contract mirrors `src/chart.py`'s baked tooltip graph palette |
| [x] | `AreaChartOptions` | struct | `CHART` | U: defaults mirror `src/chart.py` keyword defaults (`vmax`, colors, grid levels, left_pad, overlay, label_values) |
| [x] | `encode_png` | function | `CHART` | U/D: Rust PNG round-trip test validates scanline filter bytes, chunk order, CRCs, and decoded RGBA reconstruction for the `_encode_png` parity slice |
| [x] | `area_chart_png` | function | `CHART` | U/D: fixed Python decoded-pixel CRC corpus covers empty/overlay/single/constant charts, clipped labels, fill, line AA, overlay, and repeated-call determinism |

### `rust/src/render/traces.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `TraceMetric` | enum | `TRACES` | U/D: CPU/memory config, identity, glyph, label, and history selection |
| [x] | `bar_html` / `column_html` / `spark_html` / `braille_html` | functions | `TRACES` | U/D: fixed Python byte corpus; missing/zero/boundary/history cases |
| [x] | `bar_row` / `column_row` / `spark_row` / `braille_row` | functions | `TRACES` | U/D: standalone row structure and absent-data collapse |
| [x] | `bar_spark_row` / `bar_braille_row` | functions | `TRACES` | U/D: combo structure, presence logic, labels, and Python half-even layout width |

### `rust/src/test_support.rs` (module root) + `rust/src/test_support/*` (submodules)

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `FixtureRoot` | struct | `FIXTURES` | U: default + `join` + `proc`/`sys`/`run` subtrees + `from_env` (`CARGO_MANIFEST_DIR`-resolved) |
| [x] | `FakeClock` | struct | `FIXTURES` | U: `at` + `advance` + `tick` + `set_advance_step`; saturating overflow on monotonic + wall |
| [x] | `FakeCommandRunner` + re-exported `CommandRunner` trait | struct + trait | `FIXTURES`/`INTEGRATION` | U: argv-keyed FIFO `enqueue` + `run` + ordered `call_trace` + `next_call` peek; implements promoted `domain::boundary::CommandRunner` |
| [x] | `FakeDbus` + re-exported `DbusFacade` trait | struct + trait | `FIXTURES`/`INTEGRATION` | U: signature-keyed `(bus,service,path,iface,member)` FIFO + `call_trace`; implements promoted `domain::boundary::DbusFacade` |
| [x] | `FixtureLoader` + `OracleFixtureRaw` | struct + struct | `FIXTURES` | U: `load_text`/`load_bytes`/`load_oracle_fixture` (raw `toml::Value` view); typed deserialization deferred to Wave 3/4 |
| [x] | `FixtureError` / re-exported `BoundaryError` | enums | `FIXTURES`/`INTEGRATION` | U: loader errors stay test-local; fake boundary failures now use the promoted production `BoundaryError` contract |
| [x] | `DbusCall` type alias | alias | `FIXTURES` | U: `(BusKind, String, String, String, String)` for trace slices |

### `rust/src/sensors/cpu.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `CpuPaths` / `CpuState` | structs | `SENSOR-CPU` | U: discovered path bundle plus aggregate/per-core diff and history state |
| [x] | `discover_cpu_paths` / `find_cpu_temp_path` / `find_cpu_freq_path` / `detect_cpu_turbo_supported` | functions | `SENSOR-CPU` | U/F: override precedence, hwmon/sysfs discovery, missing paths, and fallback support |
| [x] | `read_cpu_usage` / `read_cpu_cores` | functions | `SENSOR-CPU` | U/F: first/delta/reset/malformed/core-count/history cases; mirrors Python CPU formulas |
| [x] | `read_uptime_seconds` / `read_load_average` | functions | `SENSOR-CPU` | U/F: proc fixture parsing plus missing/malformed results |
| [x] | `read_cpu_frequency_mhz` / `read_cpu_turbo` | functions | `SENSOR-CPU` | U/F: sysfs fast path, cpuinfo/boost fallback, inversion, and malformed results |

### `rust/src/sensors/hwmon.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `hwmon_dirs_matching` / `resolve_sensor_spec` | functions | `SENSOR-DISK` | U/F: case-insensitive chip match, manual spec resolution, and absent files |
| [x] | `read_path_millidegrees_celsius` / `read_path_int` | functions | `SENSOR-DISK` | U/F: milli-unit conversion, integer parsing, and absent/malformed paths |

### `rust/src/sensors/memory.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `MemoryState` | struct | `SENSOR-MEM` | U: shared memory-history buffer + sample timestamp state |
| [x] | `MemoryUsage` | struct | `SENSOR-MEM` | U: percent + used/total GiB result shape |
| [x] | `read_mem_total_bytes` | function | `SENSOR-MEM` | U: deterministic `MemTotal` reader; Rust counterpart to `src/sensors.py:_mem_total_bytes` |
| [x] | `read_memory_usage` | function | `SENSOR-MEM` | U: direct `MemAvailable` path + procps-style fallback + zero/clamp handling + history cadence; mirrors `src/sensors.py:_read_mem_usage` |
| [x] | `read_swap_usage` | function | `SENSOR-MEM` | U: swap-total-zero absent behavior + one-decimal-percent truncation; mirrors `src/sensors.py:_read_swap_usage` |

### `rust/src/sensors/network.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `NetInfo` | struct | `SENSOR-NET` | U: route device + IP + optional wifi SSID/signal result shape |
| [x] | `NetworkState` | struct | `SENSOR-NET` | U: cached route info, per-interface rate diff state, and graph-history buffers |
| [x] | `NetworkState::net_up_history` / `net_down_history` | methods | `SENSOR-NET` | U: read-only history exposure for future formatter/chart lanes |
| [x] | `detect_net_device` | function | `SENSOR-NET` | U: exact `ip route get` → `ip route show default` fallback and `dev` token parsing; mirrors `src/sensors.py:_detect_net_device` |
| [x] | `detect_has_wifi` | function | `SENSOR-NET` | U: sysfs wireless-interface presence detection; mirrors `src/sensors.py:_detect_has_wifi` |
| [x] | `dbm_to_pct` | function | `SENSOR-NET` | U: linear clamped dBm→percent conversion; mirrors `src/sensors.py:_dbm_to_pct` |
| [x] | `read_net_info` | function | `SENSOR-NET` | U: shared route/IP + wireless-only `iw` parsing and call trace; mirrors `src/sensors.py:_read_net_info` |
| [x] | `read_net_info_cached` | function | `SENSOR-NET` | U: 10-second TTL cache over `read_net_info`; mirrors `src/sensors.py:_read_net_info_cached` |
| [x] | `read_net_speed` | function | `SENSOR-NET` | U: sysfs tx/rx diff against monotonic time, with first-sample/device-switch/counter-reset suppression; mirrors `src/sensors.py:_read_net_speed` |
| [x] | `sample_net_history` | function | `SENSOR-NET` | U: graph-page-gated, cadence-driven bounded up/down history with zero-fill for missing side; mirrors `src/sensors.py:_sample_net_history` |

### `rust/src/sensors/disk.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `DiskKind` / `DiskIdentity` | enum/struct | `SENSOR-DISK` | U: stable disk-identity shape for later POWER/FORMATTER lanes; NVMe vs ATA + rotational flag |
| [x] | `DiskUsage` | struct | `SENSOR-DISK` | U: visible percent + half-even rounded used/total GiB result shape |
| [x] | `DiskState` | struct | `SENSOR-DISK` | U: label-keyed hd-temp/fan caches plus whole-disk byte-rate diff state |
| [x] | `find_hd_temp_paths` | function | `SENSOR-DISK` | U/F: override precedence plus NVMe/drivetemp autodetect with Python-matching labels; mirrors `src/sensors.py:_find_hd_temps` |
| [x] | `find_fan_speed_paths` | function | `SENSOR-DISK` | U/F: numbered fan override discovery with first-missing-slot stop; mirrors `src/sensors.py:_find_fans` |
| [x] | `read_hd_temp_cached` / `read_fan_speed_cached` | functions | `SENSOR-DISK` | U/F: 30-second label-keyed caches over hwmon integer reads; mirrors `src/sensors.py:_read_hd_temp_cached` and `_read_fan_speed_cached` |
| [x] | `resolve_mounts` | function | `SENSOR-DISK` | U/F/P: explicit-list passthrough plus auto-root filtering/order and escaped-path decoding; mirrors `src/sensors.py:_resolve_mounts` |
| [x] | `detect_disk_io_device` | function | `SENSOR-DISK` | U/F: mountpoint → whole-disk topology walk with mapper fallback; mirrors `src/sensors.py:_detect_disk_io_device` |
| [x] | `detect_disks` | function | `SENSOR-DISK` | U/F: supported whole-disk enumeration with rotational classification; mirrors `src/sensors.py:_detect_disks` |
| [x] | `read_disk_usage` | function | `SENSOR-DISK` | U/F: `statvfs`-backed df/psutil percent semantics plus half-even GiB rounding; mirrors `src/sensors.py:_read_disk_usage` |
| [x] | `read_disk_io` | function | `SENSOR-DISK` | U/F: `/proc/diskstats` byte-rate diffs with first-sample/device-switch/rollback suppression; mirrors `src/sensors.py:_read_disk_io` |

### `rust/src/runtime/mod.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `runtime_dir` / `state_dir` | functions | `RUNTIME` | U: `$XDG_RUNTIME_DIR/pirostats` with `/tmp/pirostats-{uid}` fallback; empty-XDG-as-unset; mirrors `src/runtime.py:_runtime_dir` |
| [x] | `panel_file` / `tooltip_file` / `geom_file` / `page_file` / `npages_file` / `lock_file` | functions | `RUNTIME` | U: lazy per-call resolution; mirrors `src/runtime.py` module constants |
| [x] | `ensure_dirs` | function | `RUNTIME` | I: idempotent `create_dir_all`; returns `io::Result<()>` (Python silent) |

### `rust/src/runtime/atomic.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `write_atomic` | function | `RUNTIME` | I: PID-unique tmp + rename-over; tmp cleanup on failure; target preserved on write error; success leaves no tmp |

### `rust/src/runtime/page.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `PageDirection` | enum | `RUNTIME` | U: local `{ Next, Prev }`; `cli::PageDirection` bridge deferred to Wave 5 |
| [x] | `read_page` / `npages` | functions | `RUNTIME` | U: missing-file + garbage defaults (0 / 1); mirrors `src/pagestate.py` |
| [x] | `set_page` | function | `RUNTIME` | I: atomic write via `write_atomic`; PID-unique tmp matches Python's `page.{pid}.tmp` scheme |
| [x] | `step_page` | function | `RUNTIME` | I: flock serialization (`nix::fcntl::Flock::LockExclusive`); early-out when `npages ≤ 1`; `rem_euclid` wrap for negative deltas; **32-thread concurrency test proves no lost updates**; readonly-dir permission failure propagates `Err` |

### `rust/src/config/mod.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `Config` + sub-structs (`DisplayConfig`/`PagesConfig`/`BarConfig`/`SparkConfig`/`BrailleConfig`/`ColumnConfig`/`ThresholdConfig`/`NotifyThresholds`/`NotificationConfig`/`SensorOverrides`/`DiskConfig`/`BatteryConfig`/`SystemUpdatesConfig`/`ServerCheckConfig`) | structs | `CONFIG` | U: every field/default ports `src/config.py:76–376`; `Mounts` enum for `list[str] \| str` |
| [x] | `Section` / `Surface` (config-local) | structs | `CONFIG` | U: section order + items + glyphs; surface item_set + has |
| [x] | `load_config` / `load_config_with_dmi` / `load_config_with_machine` | functions | `CONFIG` | U+I: ports `src/config.py:load_config`; `_with_dmi` test seam replaces Python's monkeypatch pattern |
| [x] | `apply_canonical_width` | function | `CONFIG` | U: non-ratcheting tooltip-width resolver; floors at `TOOLTIP_WIDTH_FLOOR` |
| [x] | `drop_unknown_items` / `drop_misplaced_items` / `drop_items` | functions | `CONFIG` | U: delegates to `domain::registry::{unknown_item_names, misplaced_items}` — NO local duplicates |
| [x] | `ConfigError` | enum | `CONFIG` | U: `Io`/`Toml` variants; `Error::Config` promotion proposed for Wave 5 |
| [x] | constants `TOOLTIP_WIDTH_FLOOR`/`BRAILLE_LENGTH_MULTIPLIER`/`CSS_ADVANCE_RATIO`/`BAR_SAFETY_PX`/`COLUMN_DIGIT_RATIO` | consts | `CONFIG` | U: mirror `src/config.py` underscored variants |

### `rust/src/config/merge.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `deep_merge_tables` | function | `CONFIG` | U: scalar replace + recursive dict merge; mirrors `_deep_merge` |
| [x] | `resolve_items` / `parse_surface` | functions | `CONFIG` | U: section-order-driven parse; `glyphs` surface option survives merge |
| [x] | `load_toml_at` / `load_machines` / `default_config_path` / `resolve_style` / `user_machines_path` / `machines_path_for` / `machine_source_paths` | functions | `CONFIG` | U: asset-path selection; `Path::exists()` race documented |

### `rust/src/config/geometry.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `PanelGeometry` | struct | `CONFIG` | U: 4-field (height/main_advance/vertical/tooltip_advance) |
| [x] | `parse_kde_ini` / `applet_root_containment` | functions | `CONFIG` | U: KDE INI header/keyval split; manual port of `_APPLET_ROOT_RE` (no `regex` dep) |
| [x] | `detect_vertical_from_appletsrc[_text|_at]` | functions | `CONFIG` | U: panel-edge detection; `_text`/`_at` test seams |
| [x] | `parse_geom` | function | `CONFIG` | U: 3-field + 4-field (tooltip advance) parsing |
| [x] | `read_geom_file[_at]` / `cache_live_geom[_at]` | functions | `CONFIG` | U: prefers live over cache; falls back to cache when live absent |
| [x] | `detect_panel_geometry[_at]` / `detect_vertical_layout` / `auto_fit_panel` | functions | `CONFIG` | U: full geometry pipeline; auto-fit derives bar/column/spark dims |
| [x] | `detect_machine[_with_dmi]` | functions | `CONFIG` | U: pure `_with_dmi` core takes board+product strings; `detect_machine` reads `/sys/class/dmi/id/*` |

### `rust/src/config/assets.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `code_root` / `xdg_dir` / `home_dir` / `shipped_config` / `shipped_machines` / `shipped_language` / `parent_or_dot` | functions | `CONFIG` | U: asset root resolution; `PIROSTATS_CODE_ROOT` env override for packaged installs |
| [x] | `compute_*` (pure cores) | functions | `CONFIG` | U: pure helpers tested without host env mutation |

## Call-edge accounting gate

Phase 0 must generate a machine-readable AST call-edge report for every Python code file. `tests/test_inventory.py` runs `tools/inventory_ast_reporter.py` across `src`, `tests`, `tools`, and `pirostats`, and this table must match the reporter's per-file `Call sites` and `Unique syntactic callees` counts. Dynamic calls/closures are assigned to enclosing symbol and tested by ordered dependency traces. At planning time, the checked static call totals are:

| File | Call sites | Unique syntactic callees |
|---|---:|---:|
| `src/__init__.py` | 0 | 0 |
| `src/bolt_battery.py` | 54 | 39 |
| `src/chart.py` | 59 | 21 |
| `src/config.py` | 211 | 94 |
| `src/daemon.py` | 388 | 148 |
| `src/formatter.py` | 311 | 104 |
| `src/forms.py` | 5 | 3 |
| `src/items.py` | 62 | 27 |
| `src/metrics.py` | 49 | 8 |
| `src/mono_render.py` | 75 | 25 |
| `src/notifier.py` | 55 | 24 |
| `src/pages.py` | 100 | 59 |
| `src/pagestate.py` | 18 | 16 |
| `src/registry.py` | 147 | 66 |
| `src/render_model.py` | 26 | 20 |
| `src/runtime.py` | 6 | 5 |
| `src/sensors.py` | 575 | 265 |
| `src/traces.py` | 56 | 23 |
| `src/units.py` | 0 | 0 |
| `tests/conftest.py` | 4 | 4 |
| `tests/oracle.py` | 143 | 53 |
| `tests/test_config.py` | 147 | 35 |
| `tests/test_deadcode.py` | 15 | 10 |
| `tests/test_formatter.py` | 301 | 62 |
| `tests/test_golden_render.py` | 32 | 22 |
| `tests/test_inventory.py` | 57 | 34 |
| `tests/test_items.py` | 48 | 14 |
| `tests/test_lint.py` | 5 | 5 |
| `tests/test_mono_render.py` | 72 | 16 |
| `tests/test_notifier.py` | 59 | 18 |
| `tests/test_oracle.py` | 8 | 6 |
| `tests/test_render_model.py` | 89 | 15 |
| `tests/test_sensors.py` | 21 | 4 |
| `tests/vulture_whitelist.py` | 0 | 0 |
| `tools/demo_shot.py` | 38 | 29 |
| `tools/inventory_ast_reporter.py` | 132 | 70 |
| `tools/manual_tooltip_preview.py` | 51 | 16 |
| `tools/qt_shot.py` | 58 | 39 |
| `pirostats` | 10 | 9 |

Closure requires each current call site/callee family to be marked one of: ported and directly asserted; covered by enclosing differential call trace; preserved QML/tool behavior; intentionally removed with proof of no observable behavior. No unclassified dynamic call remains.
