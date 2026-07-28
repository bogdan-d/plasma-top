# Source and callable inventory

This ledger prevents accidental omission. Boxes are closed only by the integration owner after reviewing a lane handoff and rerunning cited evidence. `Disposition` describes final migration handling, not current status.

Agent tooling (`.agents/`, `scripts/`, `skills-lock.json`) and this `plans/` directory are excluded from product migration inventory. They are not shipped PiroStats behavior.

Evidence codes: **U** unit, **D** Python/Rust differential, **F** fault injection, **I** integration/process, **L** live hardware, **P** preserve existing assertion, **E0–E5** exactness levels from `TESTING.md`.

## File inventory

| Done | Current file | Disposition | Lane | Required verification |
|---|---|---|---|---|
| [x] | `.github/workflows/baseline.yml` | aggregate Rust + retained Python oracle CI | `SCAFFOLD/CUTOVER` | locked Rust fmt/check/clippy/test/doc, Python pytest, and user-install lifecycle jobs |
| [x] | `.gitignore` | update only for generated Rust artifacts | `SCAFFOLD/QML-VERIFY` | ignore audit (Phase 1: `rust/target/` added; QML-VERIFY re-audits at Gate 6) |
| [x] | `.test-artifacts/.gitignore` | retain disposable evidence root without generated artifacts | `BASE/QML-VERIFY` | ignore audit; no generated evidence tracked |
| [x] | `AGENTS.md` | current Rust-focused agent constraints | `CUTOVER` | production paths, gates, and load-bearing contract audit |
| [x] | `LICENSE` | preserve | `PACKAGING` | staged manual/AUR manifests include GPL license |
| [x] | `NOTICE` | preserve | `PACKAGING` | staged manual/AUR manifests include applet attribution |
| [x] | `README.md` | update for native runtime | `CUTOVER` | P8.1 runtime/dependency/architecture audit |
| [x] | `config/config.toml` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [x] | `config/machines.toml` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [x] | `docs/DESIGN.md` | update after cutover | `CUTOVER` | current Rust architecture and contract audit |
| [x] | `docs/DEVELOPMENT.md` | document Rust production and retained Python oracle setup | `CUTOVER` | setup and aggregate command audit |
| [x] | `docs/ITEMS.md` | update after cutover | `CUTOVER` | catalogue audit against current token registry |
| [x] | `docs/LAYOUT.md` | update after cutover | `CUTOVER` | Rust mono-render path and five-plan audit |
| [x] | `docs/PERFORMANCE.md` | update after cutover | `CUTOVER` | current Rust mechanisms separated from historical Python measurements |
| [x] | `install.sh` | native system + user-local install | `PACKAGING/CUTOVER` | locked build; disposable system and user install/upgrade/failure/path tests |
| [x] | `lang/en.toml` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [x] | `packaging/aur/PKGBUILD` | modify for native binary | `PACKAGING` | locked x86_64 Rust build + staged package-function manifest audit |
| [x] | `packaging/aur/pirostats.install` | modify for native binary | `PACKAGING` | native dependency/help text + shell syntax |
| [x] | `packaging/pirostats-launcher` | add packaged asset-root launcher | `PACKAGING` | staged executable sets `/usr/lib/pirostats` and execs sole Rust binary |
| [x] | `pirostats` | replace with Rust checkout launcher | `CUTOVER` | P8.1 Rust CLI smoke + explicit Python oracle separation |
| [x] | `plasmoid/.gitignore` | update only for generated Rust artifacts | `SCAFFOLD/QML-VERIFY` | Gate 6 ignore audit; no generated applet paths needed |
| [x] | `plasmoid/LICENSE` | preserve/review | `INTEGRATION` | file-specific inspection |
| [x] | `plasmoid/package/contents/config/config.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 package/config-page pass; unchanged |
| [x] | `plasmoid/package/contents/config/main.xml` | preserve; edit only if approved | `QML-VERIFY` | T6 disposable action-path substitution + real page/click traces; unchanged source |
| [x] | `plasmoid/package/contents/icons/pirostats.svg` | preserve; edit only if approved | `QML-VERIFY` | T6 package load/manifest pass; unchanged |
| [x] | `plasmoid/package/contents/ui/config/ConfigAppearance.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 live background/outline/font configuration pass; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/CheckBox.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page load/interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/ColorField.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page load/interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/ComboBox.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page load/interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/FontFamily.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page font interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/FormKCM.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page load/interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/Heading.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page load/interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/SpinBox.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 font-size interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/TextAlign.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page load/interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/TextField.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page load/interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/TextFormat.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page load/interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/libconfig/VertAlign.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 config-page load/interaction; unchanged |
| [x] | `plasmoid/package/contents/ui/main.qml` | preserve; edit only if approved | `QML-VERIFY` | T6 horizontal/vertical/planar geometry, watcher/lazy read, hover, pin, wheel, resize, desktop appearance; unchanged |
| [x] | `plasmoid/package/metadata.json` | preserve; edit only if approved | `QML-VERIFY` | correct lowercase id loaded through Application + panel runners; unchanged |
| [x] | `requirements-dev.txt` | retain Python oracle dependencies until P8.4 | `BASE/CUTOVER` | clean venv install + full Python oracle gate |
| [x] | `ruff.toml` | retain until Python removal | `BASE/CUTOVER` | ruff gate |
| [x] | `rust-toolchain.toml` | new; pin stable Rust + clippy/rustfmt components | `SCAFFOLD` | P1.1 toolchain shell + toolchain present in CI |
| [x] | `rust/Cargo.lock` | new; committed per parity plan | `SCAFFOLD` | P1.1 lockfile present + `cargo fetch --locked` no-op |
| [x] | `rust/Cargo.toml` | new; single crate metadata, `test-support` feature | `SCAFFOLD` | P1.1 + P1.2 feature gate; integration-owner path after freeze |
| [x] | `rust/DEPENDENCIES.md` | new; per-dep review ledger | `SCAFFOLD` | P1.4 baseline row + policy fields for future lanes |
| [x] | `rust/rustfmt.toml` | new; rustfmt policy | `SCAFFOLD` | P1.1 fmt gate green |
| [x] | `rust/src/lib.rs` | new; composition root, crate lint attrs | `SCAFFOLD` | P1.2 lint/fmt/clippy/test/doc green; integration-owner path after freeze |
| [x] | `rust/src/main.rs` | new; thin binary entry | `SCAFFOLD` | P1.2 delegating shell; deny `unsafe_code` |
| [x] | `rust/src/error.rs` | new; top-level user-facing error context | `SCAFFOLD` | P1.3 typed `Error` enum + `Result` alias |
| [x] | `rust/src/cli.rs` | production CLI contract for `daemon`/`render`/`probe`/`profiling`/`list-items`/`page`/`click` | `SCAFFOLD` | implemented production command parser/dispatch contract + process CLI tests |
| [x] | `rust/src/adapters.rs` | production clock, command, D-Bus, NVML, and notification adapters | `DAEMON-CLI/COLLECTOR` | deterministic fake-boundary tests + process CLI integration + Phase 7 live runs |
| [x] | `rust/src/domain/mod.rs` | new; domain composition map | `SCAFFOLD` | P1.3 frozen re-exports |
| [x] | `rust/src/domain/form.rs` | new; `Form`/`Shape`/`Surface`/`SurfaceSet` contracts | `SCAFFOLD` | P1.3 mirrors `src/forms.py`; invariant tests |
| [x] | `rust/src/domain/metric.rs` | new; `Metric`/`MetricSpec`/`Capability` contracts | `SCAFFOLD` | P1.3 mirrors `src/metrics.py` + capability map |
| [x] | `rust/src/domain/item.rs` | new; validated `metric[:form]` `ItemToken` | `SCAFFOLD` | P1.3 token rules mirror `src/registry.py` |
| [x] | `rust/src/domain/registry.rs` | new; token/capability derivation layer (`parse`/`unknown_item_names`/`misplaced_items`/`needed_capabilities`/`SEPARATOR_ITEMS`/`list_items`) | `DOMAIN` | P2 mirrors token+capability half of `src/registry.py`; 51-row `list-items` corpus + 51×2 misplaced matrix |
| [x] | `rust/src/domain/boundary.rs` | new; production command/D-Bus/notification boundary contracts plus typed payloads, errors, clock, and filesystem roots | `INTEGRATION/NOTIFY` | P4 contract slices: production traits are shared with deterministic fakes; notification payload preserves title/body/icon/urgency/timeout and exposes typed delivery failure |
| [x] | `rust/src/domain/readings.rs` | new; typed aggregate hardware/readings contracts (`HardwareSnapshot`, `ReadingsSnapshot`, batteries, load, process rows, SMART identity) | `INTEGRATION` | P4 contract slice: replaces placeholder capability sets with formatter/collector-ready typed models |
| [x] | `rust/src/domain/state.rs` | new; typed aggregate mutable daemon/cache state (`DaemonStateSnapshot`, caches, timed values, rate/GPU/notification state) | `INTEGRATION/GPU/NOTIFY` | typed cross-poll state owns notification latches, including retained per-device state, alongside collector caches |
| [x] | `rust/src/render/mod.rs` | new; render composition and public API | `RENDER-CORE` | P3 module registration + documented re-exports |
| [x] | `rust/src/render/model.rs` | new; cells/rows/blocks, thresholds, grouping, inline HTML | `RENDER-CORE` | P3 unit tests + fixed Python byte corpus + no-table invariant |
| [x] | `rust/src/render/mono.rs` | new; five-plan table-free monospace serializer | `RENDER-CORE` | P3 unit/width sweep + fixed Python byte corpus covering every plan |
| [x] | `rust/src/render/traces.rs` | new; bar/column/spark/braille encodings + standalone/combo rows | `TRACES` | P3 ports `src/traces.py`; 12 focused tests + fixed Python byte corpus + combo-row structure parity |
| [x] | `rust/src/render/cells.rs` | new; formatter shared helpers for labels/ellipsis/disk text/separator normalization | `FORMATTER` | P4 helper parity via Rust formatter suite + shipped goldens |
| [x] | `rust/src/render/registry.rs` | new; formatter-side token resolution, CSS form tokens, trace-metric mapping, and hardware gates | `FORMATTER` | P4 gate parity via Rust formatter suite + shipped goldens |
| [x] | `rust/src/render/formatter.rs` | new; main panel/tooltip formatter, item dispatch, canonical width, and formatter-owned irregular rows | `FORMATTER` | P4 byte-identical panel H/V + tooltip goldens, canonical-width guard, and mapped Python formatter oracle |
| [x] | `rust/src/render/chart.rs` | new; deterministic tooltip graph PNG rasterizer (grid/labels/fill/line/overlay) and PNG encode/decode test corpus | `CHART` | P4 decoded-pixel parity against `src/chart.py` for empty/overlay/single/constant corpora + PNG chunk/CRC round-trip |
| [x] | `rust/src/page_commands.rs` | new; tooltip page registry, command execution/cache, connections formatting, title/pager/default click | `PAGES` | P4 exact Python corpora + fake command traces cover argv, 5-second timeout, PTY fallback, cache, output/error cases, service/process resolution, and page shell |
| [x] | `rust/src/notify.rs` | new; notification edge/hold/hysteresis state machine and non-fatal facade degradation reporting | `NOTIFY` | P4 exact ordered payload corpus covers all ten notification types, boundaries, monotonic hold, recovery/retrigger, exclusions, retained device state, disabled/absent inputs, and service failure |
| [x] | `rust/src/daemon.rs` | synchronous lifecycle, reload, collection, rendering, publication, page wake, and shutdown | `DAEMON-CLI` | deterministic lifecycle/process tests + QML/runtime integration + Phase 7 soak |
| [x] | `rust/src/diagnostics.rs` | production render/probe/profiling/list-items commands | `DAEMON-CLI` | process CLI tests + fixed corpora + current-host probe/profiling evidence |
| [x] | `rust/src/render/pages.rs` | new; CPU-core, process, and graphs deep-dive page renderer | `PAGES` | P4 exact CPU/process HTML corpora + graphs image/legend/vendor/network structure tests; table-free tooltip shell |
| [x] | `rust/src/sensors/mod.rs` | new; sensor composition map | `SENSOR-CPU` | P3 module registration for incremental sensor lanes |
| [x] | `rust/src/sensors/cpu.rs` | new; CPU discovery, `/proc/stat` diffs, uptime/loadavg, cpufreq/turbo, and per-core histories | `SENSOR-CPU` | P3 ports CPU-owned pieces of `src/sensors.py`; 17 focused tests cover first/delta/reset/malformed/history/discovery/fallback |
| [x] | `rust/src/sensors/memory.rs` | new; `/proc/meminfo` memory/swap readers, total-memory helper, and bounded memory history | `SENSOR-MEM` | P3 ports memory-owned pieces of `src/sensors.py`; 12 focused tests cover direct/fallback/zero/clamp/malformed/history/swap/rounding |
| [x] | `rust/src/sensors/network.rs` | new; route/device detection, wifi identity/signal, sysfs byte rates, and bounded network history | `SENSOR-NET` | P3 ports network-owned pieces of `src/sensors.py`; 11 focused tests cover `ip` fallback, wired/wireless paths, TTL caching, interface-switch/counter-reset rate resets, and graph-history trimming |
| [x] | `rust/src/sensors/hwmon.rs` | new; shared hwmon directory/spec/int helpers for disk-owned sensor paths | `SENSOR-DISK` | P3 ports the disk lane's generic hwmon helpers; 3 focused tests cover substring matching, manual spec resolution, and parse failures |
| [x] | `rust/src/sensors/disk.rs` | new; mount resolution, statvfs usage, block-device identity/topology, hwmon disk/fan caches, and `/proc/diskstats` byte rates | `SENSOR-DISK` | P3 ports disk-owned pieces of `src/sensors.py`; 17 focused tests cover mount filters, NVMe/SCSI labels, partition stacks, TTL caching, rate resets, and df-style usage math |
| [x] | `rust/src/sensors/gpu_nvidia.rs` | new; NVIDIA PCI detection, NVML/fallback orchestration, `nvidia-smi` CSV parsing/cache, clamps, and active-GPU history | `GPU` | P4 ports GPU-owned pieces of `src/sensors.py`; 10 focused tests cover no GPU, NVML success/init/read failure, exact fallback request, malformed/error results, all-absent caching, TTL boundary, vendor preference, cadence, gaps, and trimming |
| [x] | `rust/src/sensors/gpu_intel.rs` | Intel PCI discovery and DRM-fdinfo usage metrics/cache | `PROCESS` | 18 focused absence/discovery/diff/cache tests; live hardware deferred under D006 |
| [x] | `rust/src/sensors/process.rs` | panel/page process sampling and command-name resolution | `PROCESS` | 22 focused parse/diff/cache/page tests + collector/process-page integration |
| [x] | `rust/src/sensors/tests.rs` | deterministic collector integration matrix | `COLLECTOR` | 47 collector tests cover all sensor families, call sets, cadence, and failure isolation |
| [x] | `rust/src/sensors/power.rs` | new; UPower enumeration/properties, UDisks2 SMART discovery/health, sysfs+UPower system-battery reads with fallback, peripheral-battery reads, and Bolt HID++ queries behind a lane-local facade | `POWER` | P4 ports power-owned pieces of `src/sensors.py`; 38 focused tests cover exact D-Bus arguments/timeouts, bus/service/object/property absence, malformed variants, all three cache TTLs, sysfs→UPower fallback, charge-limit collapse, zero-peripheral suppression, and HID failure/no-level parity |
| [x] | `rust/src/sensors/hid.rs` | new; Bolt hidraw discovery, timeout-bound report I/O, and HID++ 2.0 ROOT/device-name/unified-battery protocol | `HID` | P4 ports `src/bolt_battery.py`; 16 focused tests cover exact packet bytes, discovery, absent/open/write/read failures, timeout, short/mismatch filtering, ten-read bound, feature absence, ASCII replacement, and battery conversion; direct hidraw + safe `nix::poll`, no unsafe/native HID dependency |
| [x] | `rust/src/runtime/mod.rs` | new; runtime path resolution (`runtime_dir`/`state_dir`/accessors) + `ensure_dirs` | `RUNTIME` | P2 ports `src/runtime.py`; lazy per-call path resolution for testability |
| [x] | `rust/src/runtime/atomic.rs` | new; `write_atomic` primitive (PID-unique tmp + rename-over) | `RUNTIME` | P2 ports `src/daemon.py:_write_atomic` shape; atomicity + tmp-cleanup tests |
| [x] | `rust/src/runtime/page.rs` | new; page counter (`read_page`/`set_page`/`npages`/`step_page`/`PageDirection`) with flock | `RUNTIME` | P2 ports `src/pagestate.py`; 32-thread concurrency test proves no lost updates |
| [x] | `rust/src/config/mod.rs` | new; typed `Config` tree + sub-structs + `load_config` + `apply_canonical_width` + drop guardrails | `CONFIG` | P2 ports `src/config.py` lines 76–376 + 719–735 + 772–848 + 863–885; `domain::registry` consumed directly (no duplicate unknown/misplaced helpers) |
| [x] | `rust/src/config/merge.rs` | new; TOML merge pipeline (`deep_merge_tables`/`resolve_items`/`parse_surface`/`load_toml_at`/`load_machines`) | `CONFIG` | P2 ports `src/config.py` lines 30–67 + 424–456 + 738–769 |
| [x] | `rust/src/config/geometry.rs` | new; `PanelGeometry` + DMI machine detect + appletsrc vertical detect + geom live/cache + auto-fit | `CONFIG` | P2 ports `src/config.py` lines 380–401 + 471–716; every disk-touch fn has `_at`/`_text`/`_with_dmi` test seam |
| [x] | `rust/src/config/assets.rs` | new; asset root resolution (`code_root`/`xdg_dir`/`home_dir`/`shipped_*`) with `PIROSTATS_CODE_ROOT` env override | `CONFIG` | P2 replaces Python's `__file__`-relative resolution with `CARGO_MANIFEST_DIR/..` + env override for packaged installs |
| [x] | `rust/tests/config_default_load.rs` | new; integration test loading shipped `config/config.toml` end-to-end | `CONFIG` | P2 asserts typed fields, threshold vectors, horizontal override, no unknown/misplaced items |
| [x] | `rust/tests/cli_daemon.rs` | process-level Rust CLI and daemon lifecycle tests | `DAEMON-CLI` | command dispatch, runtime publication, reload, paging, cleanup, and failure cases |
| [x] | `rust/tests/runtime_paths.rs` | new; integration tests for path resolution + `XDG_RUNTIME_DIR` fallback | `RUNTIME` | env mutation serialized via `ENV_GUARD: Mutex<()>` |
| [x] | `rust/tests/runtime_atomic.rs` | new; integration tests for atomic writes | `RUNTIME` | success/failure/cleanup matrix |
| [x] | `rust/tests/runtime_page.rs` | new; integration tests for page counter + concurrency | `RUNTIME` | 32-thread stress + permission-failure path |
| [x] | `rust/src/test_support.rs` | rewritten as module root for new-style `test_support/` directory | `FIXTURES` | P2 re-exports concrete fakes; `lib.rs` `pub mod test_support;` line unchanged |
| [x] | `rust/src/test_support/fixture_root.rs` | new; virtual FS root (`proc`/`sys`/`run` subtrees, `from_env`, `join`) | `FIXTURES` | P2 no host boundaries touched |
| [x] | `rust/src/test_support/fake_clock.rs` | new; deterministic clock (`at`/`advance`/`tick`/`set_advance_step`) | `FIXTURES` | P2 saturating overflow invariants |
| [x] | `rust/src/test_support/fake_command_runner.rs` | new; argv-keyed FIFO replies/errors + timeout-aware production `CommandRunner` implementation + call trace | `FIXTURES/INTEGRATION` | distinct-argv isolation, exhausted queue, queued adapter failure, and exact 3s/5s timeout traces |
| [x] | `rust/src/test_support/fake_dbus.rs` | new; signature-keyed D-Bus replies + production `DbusFacade` implementation + exact request trace | `FIXTURES/INTEGRATION` | FIFO order, arguments/timeouts, and empty-queue error |
| [x] | `rust/src/test_support/fake_notification.rs` | new; deterministic notification facade with ordered payload recording and queued results | `FIXTURES/NOTIFY` | records exact payload order, including failed calls; no desktop service access in tests |
| [x] | `rust/src/test_support/fixture_loader.rs` | `load_text`/`load_bytes`/`load_oracle_fixture` + intentionally raw `OracleFixtureRaw` view | `FIXTURES` | shared Python fixture schema remains decoupled from production typed snapshots |
| [x] | `rust/tests/fixtures/**` | new; 8 sample fixtures (proc/sys text, oracle TOML, cmd JSON, dbus TOML) | `FIXTURES` | P2 mirrors BASE schema; consumed by loader tests |
| [ ] | `rust/tests/parity_runner.sh` | deferred shared-fixture Python/Rust parity runner | `FIXTURES` | deferred: exits 77 because production render intentionally has no fixture-only flag; fixed corpora and integration tests carry current parity |
| [x] | `screenshots/desktop-black-text.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [x] | `screenshots/desktop-white-text.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [x] | `screenshots/graphs.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [x] | `screenshots/panel-horizontal.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [x] | `screenshots/panel-vertical.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [x] | `screenshots/process.png` | preserve reference | `QML-VERIFY` | visual comparison; regenerate only approved |
| [x] | `service/pirostats.service` | preserve for native binary | `PACKAGING` | staged manifest retains `ExecStart=/usr/bin/pirostats daemon` |
| [x] | `service/pirostats-user.service` | add user-local path variant | `CUTOVER` | normalized unit parity + `%h/.local/bin/pirostats` assertion |
| [x] | `src/__init__.py` | port then remove | `CUTOVER` | empty oracle package marker; no production or callable behavior; remove in P8.4 |
| [x] | `src/bolt_battery.py` | port then remove | `HID` | all seven symbols mapped to `rust/src/sensors/hid.rs`; fixed Rust packet assertions match Python-oracle packet bytes; discovery/error/name/battery branches covered by 16 focused tests |
| [x] | `src/chart.py` | port then remove | `CHART` | symbol + differential parity via Rust chart pixel corpus |
| [x] | `src/config.py` | port then remove | `CONFIG` | symbol + differential parity |
| [x] | `src/daemon.py` | port then remove | `DAEMON-CLI` | symbol + differential parity |
| [x] | `src/formatter.py` | port then remove | `FORMATTER/PAGES` | main panel/tooltip and deep-dive formatter symbols mapped to Rust formatter/page suites |
| [x] | `src/forms.py` | port then remove | `DOMAIN` | symbol + differential parity |
| [x] | `src/items.py` | port then remove | `FORMATTER` | reusable cell/row builders mapped to Rust formatter/cells tests and byte goldens |
| [x] | `src/metrics.py` | port then remove | `DOMAIN` | symbol + differential parity |
| [x] | `src/mono_render.py` | port then remove | `RENDER-CORE` | symbol + differential parity |
| [x] | `src/notifier.py` | port then remove | `NOTIFY` | all five symbols mapped to Rust notification boundary/state machine with exact payload and transition tests |
| [x] | `src/pages.py` | port then remove | `PAGES` | all current page symbols mapped to `rust/src/page_commands.rs` + `rust/src/render/pages.rs`; exact helper/page HTML corpora and command fault traces |
| [x] | `src/pagestate.py` | port then remove | `RUNTIME` | symbol + differential parity |
| [x] | `src/registry.py` | port then remove | `DOMAIN/FORMATTER` | token/capability and render-dispatch halves mapped to verified Rust domain/formatter suites |
| [x] | `src/render_model.py` | port then remove | `RENDER-CORE` | symbol + differential parity |
| [x] | `src/runtime.py` | port then remove | `RUNTIME` | symbol + differential parity |
| [x] | `src/sensors.py` | port then remove | `SENSOR-*/COLLECTOR` | symbol + differential parity |
| [x] | `src/traces.py` | port then remove | `TRACES` | symbol + differential parity |
| [x] | `src/units.py` | port then remove | `DOMAIN` | symbol + differential parity |
| [x] | `style/icons.toml` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [x] | `style/style-dark.css` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [x] | `style/style-light.css` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [x] | `style/style-overlay.css` | preserve | `CONFIG/QML-VERIFY` | byte/key/selector parity |
| [x] | `tests/conftest.py` | retain then port/archive | `BASE/INTEGRATION` | existing assertion mapped to Rust |
| [x] | `tests/golden/panel_h.html` | preserve oracle | `FORMATTER` | byte-identical Rust horizontal-panel snapshot |
| [x] | `tests/golden/panel_v.html` | preserve oracle | `FORMATTER` | byte-identical Rust vertical-panel snapshot |
| [x] | `tests/golden/tooltip.html` | preserve oracle | `FORMATTER` | byte-identical Rust main-tooltip snapshot |
| [x] | `tests/fixtures/oracle_render_full.toml` | preserve full deterministic Python oracle fixture | `BASE/FIXTURES` | Python golden assertion + verbatim Rust fixture-loader corpus |
| [ ] | `tests/oracle.py` | retain then port/archive | `BASE/INTEGRATION` | oracle fixture/render parity mapped to Rust |
| [x] | `tests/test_config.py` | retain then port/archive | `BASE/CONFIG` | existing assertion mapped to Rust |
| [x] | `tests/test_deadcode.py` | retain then port/archive | `BASE/INTEGRATION` | existing assertion mapped to Rust |
| [x] | `tests/test_formatter.py` | retain oracle; mapped to Rust formatter suite | `BASE/FORMATTER` | existing assertion preserved in Python and mapped to Rust formatter coverage |
| [x] | `tests/test_golden_render.py` | retain oracle; mapped to Rust formatter goldens | `BASE/FORMATTER` | existing assertion preserved in Python and mapped to Rust panel/tooltip golden coverage |
| [ ] | `tests/test_inventory.py` | retain then port/archive | `BASE/INTEGRATION` | inventory gate + reporter smoke |
| [x] | `tests/test_items.py` | retain then port/archive | `BASE/DOMAIN` | existing assertion mapped to Rust |
| [x] | `tests/test_lint.py` | retain then port/archive | `BASE/INTEGRATION` | existing assertion mapped to Rust |
| [x] | `tests/test_mono_render.py` | retain then port/archive | `BASE/RENDER-CORE` | existing assertion mapped to Rust |
| [x] | `tests/test_notifier.py` | retain oracle; mapped to Rust notification suite | `BASE/NOTIFY` | all existing hold/hysteresis assertions preserved and expanded across every alert/failure path |
| [ ] | `tests/test_oracle.py` | retain then port/archive | `BASE/INTEGRATION` | oracle fixture/render parity mapped to Rust |
| [x] | `tests/test_render_model.py` | retain then port/archive | `BASE/RENDER-CORE` | existing assertion mapped to Rust |
| [x] | `tests/test_sensors.py` | retain then port/archive | `BASE/SENSOR-DISK` | existing mount-resolution assertions mapped to Rust disk tests; Python baseline still runs 4/4 |
| [x] | `tests/vulture_whitelist.py` | retain then port/archive | `BASE/INTEGRATION` | existing assertion mapped to Rust |
| [ ] | `tools/demo_shot.py` | preserve/update invocation | `BASE/QML-VERIFY` | tool smoke + target parity |
| [x] | `tools/inventory_ast_reporter.py` | preserve/update invocation | `BASE/INTEGRATION` | tool smoke + exact inventory gate |
| [ ] | `tools/manual_tooltip_preview.py` | preserve/update invocation | `BASE/QML-VERIFY` | tool smoke + target parity |
| [x] | `tools/python_oracle.py` | retain Python CLI as explicit stabilization oracle | `CUTOVER` | P8.1 oracle CLI smoke; never installed |
| [x] | `tools/python_live_matrix.sh` | retained Python live-probe matrix for stabilization comparisons | `BASE/HARDWARE` | Phase 7 near-simultaneous Python/Rust probe evidence; never installed |
| [x] | `tools/p6_live_matrix.sh` | add isolated horizontal/vertical/planar applet gate | `QML-VERIFY` | geometry/orientation/watcher/lazy/action traces + human interaction evidence |
| [x] | `tools/p6_png_diff.py` | add strict fixed-image comparator | `QML-VERIFY` | dimension/mean/max/fraction synthetic threshold check |
| [x] | `tools/p6_qt_matrix.sh` | add fixed-host all-page/theme Qt matrix | `QML-VERIFY` | 24 valid screenshots + table-free/golden pre-gates + environment manifest |
| [x] | `tools/p6_package_test.sh` | add disposable native package gate | `PACKAGING` | repo/`/tmp`-only install, upgrade, Python rollback, uninstall, user-file, AUR manifest checks |
| [x] | `tools/qml_verify.sh` | add isolated Rust-daemon/applet launcher | `QML-VERIFY` | bash syntax + correct-id Application smoke; disposable XDG/runtime roots; no system paths |
| [x] | `tools/qt_shot.py` | preserve/update invocation | `BASE/QML-VERIFY` | all-page dark/light/overlay matrix + plasmoid ANSI path |
| [x] | `tools/user_install_test.sh` | add disposable user-local lifecycle gate | `CUTOVER` | no sudo/global calls; spaced paths; install/upgrade/repeat uninstall/config preservation |
| [x] | `uninstall.sh` | native system + user-local uninstall | `PACKAGING/CUTOVER` | exact owned paths removed; config preserved; repeated user uninstall harmless |

## Production Python callable inventory

Every top-level function/class and class method under `src/`, plus the root entry point, is listed. Nested local functions/closures are covered through their enclosing callable branch/call-edge ledger generated in Phase 0.

### `src/__init__.py`

- [x] No declared callable: verify module constants/import/entry behavior and final disposition.

### `src/bolt_battery.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 14 | `_load_hidapi` | function | `HID` | U/F: intentionally replaced by direct hidraw `OpenOptions`; absent/open failures become typed `BoundaryError::HidFailed`, eliminating import-time FFI/native-library loading |
| [x] | 39 | `_bolt_hidraw` | function | `HID` | U/F: `find_bolt_hidraw`; sorted fixture sysfs walk covers Bolt PID/interface match plus malformed/wrong-interface absence |
| [x] | 59 | `_xfer` | function | `HID` | U/D/F: `transfer`; exact write bytes, 1s timeout trace, short/mismatch filtering, read timeout/error, write failure, and ten-read bound |
| [x] | 75 | `_get_feature_idx` | function | `HID` | U/D: `feature_index`; exact Python-oracle ROOT request bytes and absent-feature zero semantics |
| [x] | 83 | `_get_battery` | function | `HID` | U/D/F: `battery_level`; exact unified-battery request, byte-to-level conversion, absent feature, and response timeout |
| [x] | 93 | `_get_name` | function | `HID` | U/D/F: `device_name`; exact name request, NUL termination, ASCII replacement, trim, absent feature, and no-response empty-name semantics |
| [x] | 109 | `query` | function | `HID` | U/D/F: `BoltHidFacade`/`query_device`; optional name-before-battery order, no-level outcome, invalid index, absent receiver, and open failure with path context; live hardware deferred to Phase 7 |

### `src/chart.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 57 | `_encode_png` | function | `CHART` | U/D: mapped to `rust/src/render/chart.rs:encode_png`; Rust tests validate PNG chunk order, CRCs, scanline filter bytes, and decoded round-trip |
| [x] | 76 | `area_chart_png` | function | `CHART` | U/D: mapped to `rust/src/render/chart.rs:area_chart_png`; Rust tests pin Python-oracle decoded-pixel CRCs + sampled RGBA pixels for empty/overlay/single/constant corpora |

### `src/config.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 34 | `default_config_path` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 43 | `resolve_style` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 52 | `user_machines_path` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 60 | `_deep_merge` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 70 | `_from_dict` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 86 | `DisplayConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 98 | `PagesConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 111 | `BarConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 134 | `SparkConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 144 | `BrailleConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 154 | `ColumnConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 188 | `Section` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 195 | `Surface` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 205 | `Surface.has` | method | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 210 | `Surface.item_set` | method | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 215 | `ThresholdConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 251 | `NotifyThresholds` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 272 | `NotificationConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 286 | `SensorOverrides` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 300 | `DiskConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 318 | `BatteryConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 328 | `SystemUpdatesConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 336 | `ServerCheckConfig` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 344 | `Config` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 380 | `detect_machine` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 406 | `_build_section` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 410 | `_load_toml_at` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 424 | `_resolve_items` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 440 | `_parse_surface` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 508 | `PanelGeometry` | class | `CONFIG` | U/D: defaults, construction, invariants, round-trip |
| [x] | 523 | `_parse_kde_ini` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 540 | `_int_or_none` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 553 | `_detect_vertical_from_appletsrc` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 570 | `_parse_geom` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 592 | `_read_geom_file` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 611 | `cache_live_geom` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 629 | `detect_panel_geometry` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 651 | `detect_vertical_layout` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 657 | `_auto_fit_panel` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 719 | `apply_canonical_width` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 738 | `machines_path_for` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 745 | `machine_source_paths` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 755 | `_load_machines` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 772 | `load_config` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 851 | `_drop_items` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 863 | `_drop_unknown_items` | function | `CONFIG` | U/D: direct + Python differential; boundaries |
| [x] | 873 | `_drop_misplaced_items` | function | `CONFIG` | U/D: direct + Python differential; boundaries |

### `src/daemon.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 55 | `_css_path_for` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 65 | `_parse_rgb` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 74 | `_window_bg` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 102 | `_plasma_is_light` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 122 | `_read_css_file` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 138 | `_overlay_css_path` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 147 | `_read_css` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 160 | `_strip_html` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 200 | `_mtime` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 207 | `_write_atomic` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 213 | `_render_page` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 237 | `_render_tooltip` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 244 | `_publish_pages` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 252 | `_cleanup` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 263 | `_warmed_readings` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 278 | `run_probe` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 312 | `_tooltip_html_for_render` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 324 | `run_render` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 396 | `_print_timings` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 413 | `_print_cache_state` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 460 | `run_profile` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 610 | `_log_boot_ready` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 628 | `run_daemon` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 824 | `run_list_items` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 853 | `run_page` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 865 | `run_click` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |
| [x] | 877 | `main` | function | `DAEMON-CLI` | I/D/F: process or daemon call trace + errors |

### `src/formatter.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 66 | `_net_fmt` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 75 | `_maxed_readings` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 123 | `_separator_size` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 140 | `_normalize_separators` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 165 | `PanelFormatter` | class | `FORMATTER` | U/D: defaults, construction, invariants, round-trip |
| [x] | 166 | `PanelFormatter.__init__` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 178 | `PanelFormatter.format_panel` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 219 | `PanelFormatter._wrap_tooltip` | method | `FORMATTER/PAGES` | U/D E0: exact shared tooltip shell assertions in Rust formatter/page suites |
| [x] | 227 | `PanelFormatter.format_page` | method | `FORMATTER/PAGES` | U/D E0: exact arbitrary body/header/footer shell corpus |
| [x] | 232 | `PanelFormatter.format_cpu_cores` | method | `FORMATTER/PAGES` | U/D E0: exact Python populated/no-data HTML; braille width, classes, pager width |
| [x] | 271 | `PanelFormatter.format_top_process` | method | `FORMATTER/PAGES` | U/D E0: exact Python populated/no-data HTML; elastic width, escaping, threshold classes, 15-row cap |
| [x] | 328 | `PanelFormatter._graph_val` | method | `FORMATTER/PAGES` | U/D: missing/banded/active value HTML and threshold boundaries |
| [x] | 336 | `PanelFormatter._gpu_graph` | method | `FORMATTER/PAGES` | U/D: NVIDIA preference, Intel fallback, and absent-GPU graph composition |
| [x] | 349 | `PanelFormatter.format_graphs` | method | `FORMATTER/PAGES` | U/D/E2: CPU/memory/GPU/network image count/dimensions/legend/pager shell atop CHART pixel parity |
| [x] | 423 | `PanelFormatter.canonical_width` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 448 | `PanelFormatter._canonical_sig` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 458 | `PanelFormatter.format_tooltip` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 478 | `PanelFormatter._build_entries` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 534 | `PanelFormatter._available` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 546 | `PanelFormatter._render_item` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 556 | `PanelFormatter._label_cell` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 573 | `PanelFormatter._battery_sys_is_full` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 577 | `PanelFormatter._battery_sys_icon` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 606 | `PanelFormatter._middle_ellipsis` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 620 | `PanelFormatter._disk_label` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 632 | `PanelFormatter._disk_smart_icon` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 638 | `PanelFormatter._disk_smart_class` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 647 | `PanelFormatter._fmt_disk_space` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 668 | `PanelFormatter._hd_label` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 678 | `PanelFormatter._pair_grid` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 728 | `PanelFormatter._disk_smart_pair` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 748 | `PanelFormatter._hd_temp_pair` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 765 | `PanelFormatter._fan_speed_pair` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 778 | `PanelFormatter._string_row` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 790 | `PanelFormatter._wifi_signal` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 802 | `PanelFormatter._net_device_ip` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 813 | `PanelFormatter._wifi_ssid_signal` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 831 | `PanelFormatter._fmt_freq` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 841 | `PanelFormatter._uptime` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 851 | `PanelFormatter._load_avg` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 871 | `PanelFormatter._top_process` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 889 | `PanelFormatter._dual_rate_rows` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 912 | `PanelFormatter._net_speed` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 916 | `PanelFormatter._disk_io` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 920 | `PanelFormatter._battery_sys` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 968 | `PanelFormatter._battery_periph` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 986 | `PanelFormatter._system_updates` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 993 | `PanelFormatter._server_check` | method | `FORMATTER` | U/D: direct + Python differential; boundaries |

### `src/forms.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 25 | `Shape` | class | `DOMAIN` | U/D: defaults, construction, invariants, round-trip |
| [x] | 34 | `Surface` | class | `DOMAIN` | U/D: defaults, construction, invariants, round-trip |
| [x] | 47 | `Form` | class | `DOMAIN` | U/D: defaults, construction, invariants, round-trip |
| [x] | 83 | `form_from_token` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |

### `src/items.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 40 | `row` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 48 | `per` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 63 | `label` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 80 | `value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 111 | `spark` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 123 | `braille` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 137 | `freq_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 146 | `turbo_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 156 | `turbo_icon` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 177 | `disk_label` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 183 | `disk_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 195 | `disk_space` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 222 | `mem_space` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 239 | `fan_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 256 | `gpu_fan_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 272 | `hd_temp_value` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 291 | `_thr` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |

### `src/metrics.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 29 | `_ALWAYS` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [x] | 63 | `Metric` | class | `DOMAIN` | U/D: defaults, construction, invariants, round-trip |
| [x] | 78 | `_m` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [x] | 141 | `supports` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [x] | 150 | `item_surfaces` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |

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
| [x] | 26 | `_send` | function | `NOTIFY` | U/F E1: typed `NotificationFacade` + fake records exact title/body/icon/critical/never payloads; queued service failure is reported and later alerts continue |
| [x] | 41 | `Latch` | class | `NOTIFY` | U: mapped to `NotificationLatch`; default, hold, active, clear, and retrigger transitions covered |
| [x] | 49 | `NotifState` | class | `NOTIFY` | U: mapped to daemon-owned `NotificationState`; scalar and per-device latches plus Python retention-on-removal covered |
| [x] | 64 | `_sustained` | function | `NOTIFY` | U/D: monotonic elapsed hold, zero hold, continuous-trip reset, hysteresis/no-hysteresis, one edge per episode |
| [x] | 95 | `check_and_notify` | function | `NOTIFY` | U/D/F E1: all ten types, exact ordered payloads, thresholds, exclusions, disable/absence, device ownership, recovery/retrigger, and non-fatal facade failure |

### `src/pages.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 38 | `Page` | class | `PAGES` | U/D: Rust page/source/command enums + exhaustive registry metadata test preserve ids, labels, argv, TTL, PTY, renderer, and click |
| [x] | 82 | `build_pages` | function | `PAGES` | U/D: page 0, configured order/duplicates, and unknown-id skip semantics |
| [x] | 94 | `_run_command` | function | `PAGES` | U/D/F: exact argv + 5-second timeout trace; missing, adapter failure, non-zero/stderr, empty, truncation, ANSI/PTY, cache hit/expiry |
| [x] | 141 | `text_to_mono_html` | function | `PAGES` | U/D E0: fixed Python byte corpus for escaping, spaces, blank lines |
| [x] | 149 | `_text_width` | function | `PAGES` | U/D: multiline/SGR-visible width corpus |
| [x] | 156 | `_esc` | function | `PAGES` | U/D E0: covered through exact connections HTML and process/page escaping |
| [x] | 160 | `_ellipsize` | function | `PAGES` | U/D: Python boundary behavior including widths 0/1 and process-column clipping |
| [x] | 183 | `_proc_name` | function | `PAGES` | U/F: fixture proc root covers interpreter script resolution and missing cmdline fallback |
| [x] | 204 | `_service_for_port` | function | `PAGES` | U/F: curated daemon mapping, fixture `/etc/services` parse, and unknown fallback |
| [x] | 219 | `_format_connections` | function | `PAGES` | U/D E0: exact Python HTML+width corpus covers confirmed/inferred, loopback/exposed, alignment, and no-socket fallback |
| [x] | 270 | `page_inner` | function | `PAGES` | U/D E0: exact fastfetch body+pager corpus and connections colorizer shell |
| [x] | 282 | `title_html` | function | `PAGES` | U/D E0: exact Python title bytes |
| [x] | 291 | `pager_html` | function | `PAGES` | U/D E0: exact Python no-pager/active-dot/centering bytes |
| [x] | 311 | `default_click` | function | `PAGES` | U/D: exact `plasma-systemmonitor` argv; detached launch remains DAEMON-CLI |

### `src/pagestate.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 19 | `read_page` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |
| [x] | 28 | `set_page` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |
| [x] | 37 | `_npages` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |
| [x] | 44 | `step_page` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |

### `src/registry.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 37 | `_form_token` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [x] | 56 | `_historied` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 148 | `render` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 164 | `parse` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [x] | 181 | `render_item` | function | `FORMATTER` | U/D: direct + Python differential; boundaries |
| [x] | 189 | `item_gate` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [x] | 206 | `needed_capabilities` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [x] | 227 | `unknown_item_names` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |
| [x] | 233 | `misplaced_items` | function | `DOMAIN` | U/D: direct + Python differential; boundaries |

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
| [x] | 30 | `_runtime_dir` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |
| [x] | 63 | `ensure_dirs` | function | `RUNTIME` | U/I/F: direct + filesystem/concurrency failures |

### `src/sensors.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 65 | `_bus` | function | `POWER` | U: folded into the shared `DbusFacade::call` boundary; system bus connection owned by the production adapter, tests use `FakeDbus` |
| [x] | 78 | `_upower_enumerate` | function | `POWER` | U/D/F: Rust `power::upower_enumerate` issues `EnumerateDevices` on the system bus and decodes the flat `[path1, ...]` reply body; empty when the bus/service/call is unavailable |
| [x] | 93 | `_upower_device_props` | function | `POWER` | U/D/F: Rust `power::upower_device_props` models Python's proxy cached-property read as one `GetAll` call whose interleaved `[k, v, ...]` body decodes `Percentage`/`State`/`EnergyRate`/`Model`/`Type`; None on any failure |
| [x] | 116 | `timed_section` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 131 | `BatterySys` | class | `INTEGRATION` | U: mapped to typed `BatterySystemReading`; default/state token invariants tested |
| [x] | 140 | `BatteryPeriph` | class | `INTEGRATION` | U: mapped to typed `BatteryPeripheralReading`; empty aggregate invariants tested |
| [x] | 146 | `DiskUsage` | class | `INTEGRATION` | U: mapped to typed `DiskUsageReading`; formatter and disk formula tests consume it |
| [x] | 153 | `HardwareInfo` | class | `INTEGRATION` | U: mapped to typed `HardwareSnapshot`; safe empty-machine default tested |
| [x] | 192 | `_BatterySysCache` | class | `INTEGRATION` | U: mapped to `BatterySystemCache`; unset default tested |
| [x] | 200 | `_BatteryPeriphCache` | class | `INTEGRATION` | U: mapped to `BatteryPeripheralCache`; unset default tested |
| [x] | 206 | `_NetInfoCache` | class | `INTEGRATION` | U: mapped to typed network cache state consumed by SENSOR-NET |
| [x] | 215 | `_RateState` | class | `INTEGRATION` | U: mapped to `RateState`; empty previous-sample state tested through rate readers |
| [x] | 224 | `DaemonState` | class | `INTEGRATION` | U: mapped to typed `DaemonStateSnapshot`; empty cross-poll state tested |
| [x] | 294 | `Readings` | class | `INTEGRATION` | U: mapped to typed `ReadingsSnapshot`; empty zero-time default and formatter consumption tested |
| [x] | 359 | `discover_hardware` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 383 | `rescan_peripherals` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 402 | `needs_periph_rescan` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 420 | `collect` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 606 | `_cached_by_label` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 622 | `_read_hd_temp_cached` | function | `SENSOR-DISK` | U/F: Rust `read_hd_temp_cached` TTL cache mirrors Python label-keyed behavior |
| [x] | 633 | `_read_fan_speed_cached` | function | `SENSOR-DISK` | U/F: Rust `read_fan_speed_cached` TTL cache mirrors Python label-keyed behavior |
| [x] | 640 | `_hwmon_find` | function | `SENSOR-DISK` | U/F: Rust `hwmon::hwmon_dirs_matching` preserves case-insensitive `name` substring discovery |
| [x] | 656 | `_resolve_sensor` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 666 | `_read_path_millideg` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 676 | `_read_path_int` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 687 | `_find_cpu_temp` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 698 | `_find_cpu_freq_path` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 704 | `_find_hd_temps` | function | `SENSOR-DISK` | U/F: Rust `find_hd_temp_paths` covers override precedence plus NVMe/drivetemp autodetect |
| [x] | 730 | `_resolve_nvme_namespace` | function | `SENSOR-DISK` | U/F: Rust `resolve_nvme_namespace` maps controller labels to first namespace with fallback |
| [x] | 744 | `_hwmon_device_label` | function | `SENSOR-DISK` | U/F: Rust `hwmon_device_label` preserves NVMe and SCSI-backed disk labels |
| [x] | 770 | `_find_fans` | function | `SENSOR-DISK` | U/F: Rust `find_fan_speed_paths` mirrors numbered override discovery and early stop semantics |
| [x] | 785 | `_token_after` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 795 | `_detect_net_device` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 811 | `_is_wireless` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 815 | `_dbm_to_pct` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 821 | `_read_net_info` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 855 | `_read_net_info_cached` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 864 | `_resolve_mount_device` | function | `SENSOR-DISK` | U/F: Rust mount-table parsing resolves mountpoint → device basename including escaped mount paths |
| [x] | 875 | `_whole_disk_of` | function | `SENSOR-DISK` | U/F: Rust `whole_disk_of` preserves partition-parent discovery with mapper fallback |
| [x] | 892 | `_detect_disk_io_device` | function | `SENSOR-DISK` | U/F: Rust `detect_disk_io_device` mirrors mount→whole-disk topology walk |
| [x] | 914 | `_is_rotational` | function | `SENSOR-DISK` | U/F: Rust `is_rotational` preserves kernel queue flag behavior |
| [x] | 924 | `_udisks_prop` | function | `POWER` | U/D/F: Rust `power::udisks_get` issues exact `org.freedesktop.DBus.Properties.Get` with interface/property arguments; used by `read_disk_smart` for `SmartCriticalWarning`/`SmartFailing` |
| [x] | 940 | `_detect_disks` | function | `SENSOR-DISK` | U/F: Rust `detect_disks` enumerates supported whole disks and preserves rotational classification |
| [x] | 984 | `_read_disk_smart` | function | `POWER` | U/D/F: Rust `power::read_disk_smart` triggers `SmartUpdate` then decodes NVMe `SmartCriticalWarning` (healthy iff empty) / ATA `SmartFailing` (healthy iff false); None when any call fails |
| [x] | 1011 | `_read_disk_smart_cached` | function | `POWER` | U/F: Rust `power::read_disk_smart_cached` mirrors `_cached_by_label` TTL semantics with per-drive intervals (HDD vs SSD) and caches None until TTL elapses |
| [x] | 1018 | `_find_battery_sys` | function | `POWER` | U/F: Rust `power::find_battery_sys` filters/sorts UPower paths containing `/battery_BAT` |
| [x] | 1022 | `_find_peripherals` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1061 | `_detect_cpu_turbo_supported` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1069 | `_detect_has_backlight` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1081 | `_detect_has_wifi` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1091 | `_detect_nvidia` | function | `GPU` | U/F: Rust `detect_nvidia` uses explicit sys root and requires vendor `0x10de` plus display class `0x03`; missing/malformed fixture trees return false; live NVIDIA deferred to Phase 7 |
| [x] | 1102 | `_detect_intel_gpu` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1126 | `_read_cpu_usage` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1168 | `_read_cpu_cores` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1213 | `_read_uptime` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1221 | `_read_load_avg` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1234 | `_mem_total_bytes` | function | `SENSOR-MEM` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1242 | `_read_proc_stat_times` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1286 | `_cmdline_name` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1310 | `_read_top_process_cached` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1324 | `_diff_top_process` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1349 | `_read_top_process` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1360 | `read_top_process_page` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1383 | `_read_mem_usage` | function | `SENSOR-MEM` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1408 | `_sample_gpu_history` | function | `GPU` | U/D: Rust `sample_gpu_history` covers graphs gating, NVIDIA preference, Intel fallback, cadence, decoder-zero fill, missing-sample buffer exposure, and bounded trimming; fixed Python oracle assertions match |
| [x] | 1438 | `_sample_net_history` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1461 | `_read_swap_usage` | function | `SENSOR-MEM` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1468 | `_counter_rate` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1485 | `_read_net_speed` | function | `SENSOR-NET` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1496 | `_resolve_mounts` | function | `SENSOR-DISK` | U/F/P: Rust `resolve_mounts` ports all four existing Python mount-resolution assertions |
| [x] | 1518 | `_read_disk_usage` | function | `SENSOR-DISK` | U/F: Rust `read_disk_usage` mirrors df/psutil-style `statvfs` percent plus half-even GiB rounding |
| [x] | 1528 | `_read_disk_io` | function | `SENSOR-DISK` | U/F: Rust `read_disk_io` ports byte-rate diffs with first-sample/device-switch/rollback suppression |
| [x] | 1538 | `_read_cpu_freq` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1548 | `_read_cpu_turbo` | function | `SENSOR-CPU` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1558 | `_read_brightness` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1574 | `_sysfs_bat_rate` | function | `POWER` | U/F: Rust `power::sysfs_bat_rate` reads `power_now` (µW) and applies banker's-rounding watts via `round_half_even_ratio`; returns 0 on read/parse failure. Pulled forward from COLLECTOR because `_read_battery_sys` needs it. |
| [x] | 1584 | `_sysfs_bat_charge_limit` | function | `POWER` | U/F: Rust `power::sysfs_bat_charge_limit` reads `charge_control_end_threshold`, returning None when absent or reporting 100 (no meaningful limit). Pulled forward from COLLECTOR. |
| [x] | 1604 | `_sysfs_bat_read` | function | `POWER` | U/F: Rust `power::sysfs_bat_read` reads `capacity`/`status`/`power_now`, maps status via `_SYSFS_BAT_STATUS_MAP`, returns None on sysfs absence (triggers the UPower fallback in `read_battery_sys`). Pulled forward from COLLECTOR. |
| [x] | 1616 | `_read_battery_sys` | function | `POWER` | U/D/F: Rust `power::read_battery_sys` tries sysfs first (`capacity`/`status`/`power_now`/`charge_control_end_threshold`) with banker's-rounding watts, falls back to UPower `GetAll` on sysfs absence, and uses sysfs `power_now` when UPower reports a zero rate while charging/discharging |
| [x] | 1652 | `_read_battery_periph` | function | `POWER` | U/D/F: Rust `power::read_battery_periph` decodes `Percentage`/`Model` via UPower `GetAll`, caches the model name once, and suppresses zero/missing charge so the row disappears from the tooltip |
| [x] | 1677 | `_read_battery_bolt` | function | `POWER` | U/D/F: Rust `power::read_battery_bolt` consumes the lane-local `BoltBatteryFacade` trait implemented in production by `hid::BoltHidFacade`, caches name+level with the 1h TTL, advances the timestamp even on `level=None` to suppress the wake-up cost, and skips timestamp advance on HID failure (immediate retry) |
| [x] | 1710 | `_read_intel_gpu_engine_times` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1760 | `_read_intel_gpu_metrics` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1789 | `_read_intel_gpu_metrics_cached` | function | `PROCESS` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1802 | `_gpu_cache_ttl` | function | `GPU` | U/D: Rust `gpu_cache_ttl` selects 0s while NVML is usable and 3s after absence/init failure; cache-hit and exact-expiry tests |
| [x] | 1809 | `_nvidia_cap` | function | `GPU` | U/D: Rust `nvidia_cap` matches Python for None, negative, ordinary, and >99 values |
| [x] | 1818 | `_pynvml_handle_get` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1835 | `_read_nvidia_pynvml` | function | `GPU` | U/F: typed `NvmlFacade::read_device_zero` preserves mandatory read failure versus optional fan/decoder absence; success values clamp and init/read failures take the Python fallback paths; live adapter deferred to COLLECTOR/Phase 7 |
| [x] | 1859 | `_read_nvidia_smi` | function | `GPU` | U/D/F E1: exact executable/query/format/5s timeout, CSV fan/decoder reorder, unsupported fields, nonzero/signal, malformed/short/invalid UTF-8, and adapter timeout/error |
| [x] | 1883 | `_read_nvidia` | function | `GPU` | U/D/F: NVML preferred, permanent init failure, retryable read failure, fallback, all-None cache, 0s/3s cadence, and timestamp update mapped to Rust `read_nvidia` |
| [x] | 1894 | `_read_count_file` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |
| [x] | 1905 | `_read_server_file` | function | `COLLECTOR` | U/D/F/L: fixture formula, call trace, failures, live where available |

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

- [x] No declared callable: verify module constants/import/entry behavior and final disposition.

### `tools/python_oracle.py`

- [x] No declared callable: retained entry behavior is explicit oracle-only and excluded from production packaging.

## Existing test/tool callable inventory

Existing tests remain oracle evidence until mapped to a passing Rust test or intentionally retained integration check. Fixtures/helpers also require disposition because their assumptions define behavior.

### `tests/conftest.py`

- [x] No declared callable: fixture path setup remains covered by the full Python oracle gate.

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
| [x] | 11 | `test_apply_canonical_width_sets_resolved_width` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 17 | `test_apply_canonical_width_does_not_ratchet` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 24 | `test_apply_canonical_width_floors_at_builtin_minimum` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 30 | `test_apply_canonical_width_ignores_nonpositive` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 39 | `test_deep_merge_override_scalar` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 44 | `test_deep_merge_nested_dicts_merge_recursively` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 51 | `test_deep_merge_does_not_mutate_base` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 57 | `test_deep_merge_dict_replaces_non_dict` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 65 | `test_detect_machine_no_dmi_access_returns_none` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 72 | `test_detect_machine_board_contains_match` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 82 | `test_detect_machine_no_match_returns_none` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 89 | `test_detect_machine_product_contains_match` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 96 | `test_detect_machine_ignores_non_dict_entries` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 105 | `test_resolve_items_plain` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 109 | `test_resolve_items_add_appends_without_dups_preserving_order` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 114 | `test_resolve_items_remove` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 119 | `test_parse_surface_order_drives_sections` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 135 | `test_parse_surface_order_add_appends_section` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 146 | `test_surface_has_and_item_set_empty` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 154 | `test_drop_unknown_items_removes_typos` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 163 | `test_drop_unknown_items_spares_separators` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 175 | `test_drop_misplaced_items_removes_panel_only_from_the_tooltip` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 184 | `test_drop_misplaced_items_removes_tooltip_only_from_the_panel` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 194 | `test_drop_misplaced_items_leaves_a_section_empty_rather_than_absent` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 205 | `test_load_config_missing_path_returns_no_machine` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 211 | `test_load_config_section_schema` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 230 | `test_load_config_machine_items_add` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 251 | `test_load_config_machine_order_add_new_section` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 272 | `test_unknown_item_names_flags_only_unknowns` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 278 | `test_default_config_has_no_unknown_items` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 288 | `test_load_config_warns_on_unknown_item` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 300 | `test_detect_vertical_layout_defaults_vertical_without_appletsrc` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 307 | `test_detect_vertical_layout_reads_panel_edge` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 329 | `_patch_plasma` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 344 | `test_detect_panel_geometry_reads_geom_file` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 352 | `test_detect_panel_geometry_falls_back_to_appletsrc_orientation` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 360 | `test_detect_panel_geometry_ignores_degenerate_geom_file` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 368 | `test_detect_panel_geometry_stale_geom_orientation_uses_appletsrc` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 378 | `test_detect_panel_geometry_defaults_when_unreadable` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 387 | `test_read_geom_falls_back_to_cache_when_live_absent` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 398 | `test_read_geom_prefers_live_over_cache` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 409 | `test_read_geom_none_when_live_absent_and_no_cache` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 416 | `test_cache_live_geom_persists_valid_live` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 427 | `test_cache_live_geom_ignores_degenerate_and_absent` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 443 | `test_auto_fit_panel_derives_knobs_from_geometry` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 460 | `test_auto_fit_bar_height_zero_uses_main_advance` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 471 | `test_auto_fit_horizontal_sizes_column_height` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 481 | `test_auto_fit_noop_when_geometry_unpublished` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 509 | `test_orientation_override_horizontal_picks_column` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 518 | `test_orientation_override_vertical_picks_bar` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |
| [x] | 527 | `test_column_panel_width_loads` | function | `BASE/CONFIG` | P: preserve assertion; map to Rust test |

### `tests/test_deadcode.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 24 | `test_no_dead_code` | function | `BASE/INTEGRATION` | P: preserve assertion; map to Rust test |

### `tests/test_formatter.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 9 | `_bare_hw` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 24 | `test_val_cell_no_class_is_plain_val` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 31 | `test_val_cell_with_class_appends_it` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 38 | `test_fmt_perc_panel_caps_at_100_without_percent_sign` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 42 | `test_fmt_perc_tooltip_always_has_percent_sign` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 46 | `test_fmt_perc_below_100_has_percent_sign_either_way` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 53 | `test_net_fmt_zero` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 57 | `test_net_fmt_kilobits` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 61 | `test_net_fmt_megabits` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 67 | `test_disk_label_root_mount` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 71 | `test_disk_label_strips_mnt_prefix` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 75 | `test_disk_label_basename_for_run_media` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 79 | `test_middle_ellipsis_short_string_unchanged` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 83 | `test_middle_ellipsis_keeps_head_and_tail` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 87 | `test_middle_ellipsis_never_exceeds_budget` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 92 | `test_middle_ellipsis_bounds_ssid_to_max` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 97 | `test_net_device_ip_truncates_long_interface` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 105 | `test_string_row_caps_net_device_leaves_ip_raw` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 111 | `_canonical_guard_cfg` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 119 | `test_canonical_width_exceeds_short_content` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 130 | `_guard_full_hw` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 144 | `_guard_readings` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 176 | `_tooltip_tokens` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 191 | `test_canonical_width_covers_every_tooltip_item_guard` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 214 | `test_hd_label_strips_trailing_index` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 218 | `test_hd_label_no_trailing_index` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 222 | `test_hd_label_nvme_namespace_block_device` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 232 | `_fmt` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 236 | `test_std_never_attaches_bar_or_history_for_cpu_usage` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 243 | `test_bar_html_for_empty_when_value_missing` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 249 | `test_bar_html_for_empty_when_width_zero` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 255 | `test_spark_html_for_empty_when_history_missing` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 260 | `test_bar_spark_row_empty_when_only_bar_available` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 269 | `test_bar_spark_row_renders_when_both_available` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 277 | `test_bar_row_and_spark_row_agree_with_bar_spark_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 289 | `_titles` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 298 | `test_available_hw_bound_items_off_on_bare_machine` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 307 | `test_available_unbound_items_always_on` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 315 | `test_available_present_hw_turns_item_on` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 326 | `test_available_battery_periph_via_bolt_config` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 336 | `_surface_cfg` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 349 | `test_empty_section_drops_title_and_separator` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 361 | `test_first_section_has_no_leading_separator` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 369 | `test_panel_has_no_title_rows_and_no_separators` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 380 | `test_hd_temp_row_empty_without_temp` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 389 | `test_top_process_no_padding_to_fixed_count` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 398 | `_hw_disks` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 403 | `test_disk_smart_packs_two_drives_per_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 411 | `test_disk_smart_odd_count_uses_blank_filler` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 421 | `test_disk_smart_single_disk_is_full_width_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 428 | `test_disk_smart_single_result_among_many_is_full_width` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 436 | `test_disk_smart_empty_when_no_results` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 443 | `_hw_hd_temps` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 447 | `test_hd_temp_pair_packs_two_drives_per_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 455 | `test_hd_temp_pair_odd_count_uses_blank_filler` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 464 | `test_hd_temp_pair_single_disk_is_full_width_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 471 | `test_hd_temp_pair_skips_disks_without_temp` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 479 | `test_hd_temp_pair_empty_when_no_temps` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 486 | `_hw_fans` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 490 | `test_fan_speed_pair_two_fans_one_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 499 | `test_fan_speed_pair_odd_count_uses_blank_filler` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 508 | `test_fan_speed_pair_single_fan_is_full_width_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 515 | `test_fan_speed_pair_skips_fans_without_reading` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 523 | `test_fan_speed_pair_empty_when_no_readings` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 528 | `test_disk_smart_empty_when_smart_disabled` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 540 | `_row` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 543 | `test_normalize_keeps_separator_between_two_rows` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 548 | `test_normalize_drops_leading_and_trailing_separators` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 553 | `test_normalize_collapses_consecutive_keeping_largest` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 558 | `test_normalize_section_edge_separator_becomes_inter_section_gap` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |

### `tests/test_golden_render.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 27 | `_full_hw` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 41 | `_full_readings` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 65 | `_render` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |
| [x] | 84 | `test_golden_render` | function | `BASE/FORMATTER` | P: preserve assertion; map to Rust test |

### `tests/test_inventory.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 24 | `_run_report` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 39 | `_names` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 43 | `_file_counts` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 54 | `_call_edge_rows` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 82 | `test_inventory_ast_reporter_workspace_smoke` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |
| [ ] | 140 | `test_inventory_call_edge_table_matches_ast_reporter` | function | `BASE/INTEGRATION` | P/I: preserve inventory/reporter gate + exact markdown sync |

### `tests/test_items.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 13 | `_cfg` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 25 | `_where` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 34 | `test_cpu_usage_needs_no_dedicated_sensor` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 39 | `test_item_pulls_its_capability` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 44 | `test_metric_can_need_multiple_capabilities` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 49 | `test_form_does_not_change_the_capability` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 55 | `test_notification_keeps_sensor_alive_without_the_item` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 61 | `test_unknown_token_contributes_nothing` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 65 | `test_gpu_nvidia_metrics_share_one_capability` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 72 | `test_unknown_item_names_flags_bad_metric_and_bad_form` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 80 | `test_value_metrics_live_on_both_surfaces` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 86 | `test_bare_visuals_are_panel_only` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 93 | `test_wide_forms_and_string_metrics_are_tooltip_only` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 100 | `test_misplaced_items_flags_tooltip_only_in_panel` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 108 | `test_misplaced_items_flags_panel_only_in_tooltip` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |
| [x] | 117 | `test_misplaced_items_ignores_unknown_names` | function | `BASE/DOMAIN` | P: preserve assertion; map to Rust test |

### `tests/test_lint.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 24 | `test_ruff_clean` | function | `BASE/INTEGRATION` | P: preserve assertion; map to Rust test |

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
| [x] | 15 | `_Clock` | class | `BASE/NOTIFY` | P/U: mapped to shared `FakeClock` monotonic fixture |
| [x] | 17 | `_Clock.__init__` | method | `BASE/NOTIFY` | P/U: deterministic zero/start state covered by `FakeClock` tests |
| [x] | 20 | `_Clock.__call__` | method | `BASE/NOTIFY` | P/U: one monotonic snapshot is passed to each notification pass |
| [x] | 23 | `_Clock.advance` | method | `BASE/NOTIFY` | P/U: explicit duration advancement drives hold boundaries without sleeping |
| [x] | 27 | `_Hw` | class | `BASE/NOTIFY` | P/U: typed `HardwareSnapshot::cpu_count` drives load normalization |
| [x] | 33 | `sent` | function | `BASE/NOTIFY` | P/U: shared `FakeNotificationFacade` records full ordered payloads, not bodies only |
| [x] | 42 | `clock` | function | `BASE/NOTIFY` | P/U: shared fake clock fixture |
| [x] | 48 | `_cfg` | function | `BASE/NOTIFY` | P/U: isolated enable-flag helper in Rust suite |
| [x] | 58 | `_poll` | function | `BASE/NOTIFY` | P/U: Rust `poll_cpu` drives exact pass/state/facade contract |
| [x] | 66 | `test_cpu_temp_spike_never_notifies` | function | `BASE/NOTIFY` | P/U: same boost-spike sequence remains silent |
| [x] | 75 | `test_cpu_temp_notifies_once_when_sustained` | function | `BASE/NOTIFY` | P/U: same 60 × 1.5s sustained corpus emits once |
| [x] | 84 | `test_cpu_temp_hysteresis_blocks_rattle` | function | `BASE/NOTIFY` | P/U: threshold-band sequence emits no duplicate |
| [x] | 96 | `test_cpu_temp_rearms_after_cooling` | function | `BASE/NOTIFY` | P/U: below-clear recovery followed by retrigger emits twice |
| [x] | 108 | `test_cpu_temp_hold_restarts_on_a_dip` | function | `BASE/NOTIFY` | P/U: one sub-trip sample resets continuous hold |
| [x] | 118 | `test_cpu_temp_sustain_zero_fires_immediately` | function | `BASE/NOTIFY` | P/U: zero-hold exact payload corpus fires on first threshold sample |
| [x] | 127 | `test_cpu_temp_notification_off_stays_silent` | function | `BASE/NOTIFY` | P/U: disabled/absent pass remains silent and state-stable |
| [x] | 136 | `test_sustained_hold_measures_time_not_polls` | function | `BASE/NOTIFY` | P/U: two samples bracketing 61 elapsed seconds trip independent of poll count |
| [x] | 145 | `test_sustained_fires_once_per_episode` | function | `BASE/NOTIFY` | P/U: active latch suppresses repeated sends |
| [x] | 156 | `test_sustained_without_hysteresis_clears_at_the_trip_point` | function | `BASE/NOTIFY` | P/U: load-style clear==trip re-arms below threshold |

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

- [x] No declared callable: dynamic Python-oracle lookups remain covered by the vulture gate until P8.4.

### `tools/demo_shot.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 34 | `_demo_hw` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |
| [ ] | 51 | `_demo_readings` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |
| [ ] | 76 | `main` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |

### `tools/inventory_ast_reporter.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 39 | `CallContext` | class | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 44 | `_display_path` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 51 | `_read_source` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 56 | `_callee_text` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 63 | `_callee_key` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 79 | `_iter_targets` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 107 | `FileAnalyzer` | class | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 108 | `FileAnalyzer.__init__` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 115 | `FileAnalyzer.analyze` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 140 | `FileAnalyzer._visit_module_stmt` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 149 | `FileAnalyzer._visit_recorded_class` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 164 | `FileAnalyzer._visit_recorded_function` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 178 | `FileAnalyzer._visit_class_header` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 188 | `FileAnalyzer._visit_function_header` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 197 | `FileAnalyzer._record_callable` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 220 | `FileAnalyzer._context` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 227 | `FileAnalyzer._current_context` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 230 | `FileAnalyzer.visit_FunctionDef` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 235 | `FileAnalyzer.visit_AsyncFunctionDef` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 240 | `FileAnalyzer.visit_ClassDef` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 245 | `FileAnalyzer.visit_Call` | method | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 265 | `analyze_file` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 272 | `build_report` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 312 | `build_parser` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |
| [x] | 337 | `main` | function | `BASE/INTEGRATION` | I: tool smoke + exact inventory gate |

### `tools/manual_tooltip_preview.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [ ] | 35 | `_val` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |
| [ ] | 40 | `build_entries` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |
| [ ] | 75 | `main` | function | `BASE/QML-VERIFY` | I: tool smoke; E0/E4 result |

### `tools/qt_shot.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 65 | `_plasmoid_output` | function | `QML-VERIFY` | I/E4: fastfetch SGR/newline conversion matrix + focused sample assertions |
| [x] | 163 | `_qml_for_html` | function | `BASE/QML-VERIFY` | I/E4: 24-cell Qt matrix |
| [x] | 178 | `main` | function | `BASE/QML-VERIFY` | I/E4: 24-cell Qt matrix |

### `tools/p6_png_diff.py`

| Done | Line | Symbol | Kind | Lane | Evidence required |
|---|---:|---|---|---|---|
| [x] | 23 | `_delta` | function | `QML-VERIFY` | I/E4: synthetic pass/fail threshold matrix |
| [x] | 56 | `main` | function | `QML-VERIFY` | I/E4: synthetic CLI pass/fail threshold matrix |

## QML callable and signal-handler inventory


### `plasmoid/package/contents/config/config.qml`

- [x] Declarative-only file: T6 load/bind/config visual verification.

### `plasmoid/package/contents/ui/config/ConfigAppearance.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 15 | `onDesktop` — `readonly property bool onDesktop: plasmoid.formFactor === PlasmaCore.Types.Planar` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/CheckBox.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 11 | `onClicked` — `onClicked: plasmoid.configuration[configKey] = !plasmoid.configuration[configKey]` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/ColorField.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 19 | `onTextChanged` — `onTextChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 52 | `onValueChanged` — `onValueChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 90 | `onClicked` — `onClicked: dialogLoader.active = true` | QML-VERIFY T6 event/interaction path |
| [x] | 137 | `onSelectedColorChanged` — `onSelectedColorChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 142 | `onAccepted` — `onAccepted: {` | QML-VERIFY T6 event/interaction path |
| [x] | 146 | `onRejected` — `onRejected: {` | QML-VERIFY T6 event/interaction path |
| [x] | 154 | `Component.onCompleted` — `Component.onCompleted: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/ComboBox.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 21 | `onPopulate` — `onPopulate: {` | QML-VERIFY T6 event/interaction path |
| [x] | 36 | `onConfigValueChanged` — `onConfigValueChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 58 | `Component.onCompleted` — `Component.onCompleted: {` | QML-VERIFY T6 event/interaction path |
| [x] | 63 | `onCurrentIndexChanged` — `onCurrentIndexChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 75 | `size` — `function size() {` | QML-VERIFY T6 event/interaction path |
| [x] | 87 | `findValue` — `function findValue(val) {` | QML-VERIFY T6 event/interaction path |
| [x] | 96 | `selectValue` — `function selectValue(val) {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/FontFamily.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 17 | `isMonospace` — `function isMonospace(family) {` | QML-VERIFY T6 event/interaction path |
| [x] | 22 | `onPopulate` — `onPopulate: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/FormKCM.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 24 | `Window.onWindowChanged` — `Window.onWindowChanged: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/Heading.qml`

- [x] Declarative-only file: T6 load/bind/config visual verification.

### `plasmoid/package/contents/ui/libconfig/SpinBox.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 41 | `onValueRealChanged` — `onValueRealChanged: serializeTimer.start()` | QML-VERIFY T6 event/interaction path |
| [x] | 90 | `onTriggered` — `onTriggered: {` | QML-VERIFY T6 event/interaction path |
| [x] | 129 | `onActiveFocusChanged` — `onActiveFocusChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 134 | `selectValue` — `function selectValue() {` | QML-VERIFY T6 event/interaction path |
| [x] | 143 | `fixMinus` — `function fixMinus(str) {` | QML-VERIFY T6 event/interaction path |
| [x] | 155 | `fixDecimals` — `function fixDecimals(str) {` | QML-VERIFY T6 event/interaction path |
| [x] | 162 | `fixText` — `function fixText(str) {` | QML-VERIFY T6 event/interaction path |
| [x] | 166 | `onTextEdited` — `function onTextEdited() {` | QML-VERIFY T6 event/interaction path |
| [x] | 197 | `bindContentItem` — `function bindContentItem() {` | QML-VERIFY T6 event/interaction path |
| [x] | 210 | `onContentItemChanged` — `onContentItemChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 214 | `Component.onCompleted` — `Component.onCompleted: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/TextAlign.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 13 | `setValue` — `function setValue(val) {` | QML-VERIFY T6 event/interaction path |
| [x] | 20 | `updateChecked` — `function updateChecked() {` | QML-VERIFY T6 event/interaction path |
| [x] | 28 | `Component.onCompleted` — `Component.onCompleted: updateChecked()` | QML-VERIFY T6 event/interaction path |
| [x] | 34 | `onClicked` — `onClicked: setValue(Text.AlignLeft)` | QML-VERIFY T6 event/interaction path |
| [x] | 41 | `onClicked` — `onClicked: setValue(Text.AlignHCenter)` | QML-VERIFY T6 event/interaction path |
| [x] | 48 | `onClicked` — `onClicked: setValue(Text.AlignRight)` | QML-VERIFY T6 event/interaction path |
| [x] | 55 | `onClicked` — `onClicked: setValue(Text.AlignJustify)` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/TextField.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 12 | `onConfigValueChanged` — `onConfigValueChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 19 | `onTextChanged` — `onTextChanged: serializeTimer.start()` | QML-VERIFY T6 event/interaction path |
| [x] | 30 | `onClicked` — `onClicked: textField.text = defaultValue` | QML-VERIFY T6 event/interaction path |
| [x] | 42 | `onTriggered` — `onTriggered: {` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/TextFormat.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 30 | `onClicked` — `onClicked: plasmoid.configuration[configKey] = checked` | QML-VERIFY T6 event/interaction path |
| [x] | 40 | `onClicked` — `onClicked: plasmoid.configuration[configKey] = checked` | QML-VERIFY T6 event/interaction path |
| [x] | 50 | `onClicked` — `onClicked: plasmoid.configuration[configKey] = checked` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/libconfig/VertAlign.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 13 | `setValue` — `function setValue(val) {` | QML-VERIFY T6 event/interaction path |
| [x] | 20 | `updateChecked` — `function updateChecked() {` | QML-VERIFY T6 event/interaction path |
| [x] | 27 | `Component.onCompleted` — `Component.onCompleted: updateChecked()` | QML-VERIFY T6 event/interaction path |
| [x] | 33 | `onClicked` — `onClicked: setValue(Text.AlignTop)` | QML-VERIFY T6 event/interaction path |
| [x] | 40 | `onClicked` — `onClicked: setValue(Text.AlignVCenter)` | QML-VERIFY T6 event/interaction path |
| [x] | 47 | `onClicked` — `onClicked: setValue(Text.AlignBottom)` | QML-VERIFY T6 event/interaction path |

### `plasmoid/package/contents/ui/main.qml`

| Done | Line | Callable/handler | Required evidence |
|---|---:|---|---|
| [x] | 24 | `onNewData` — `onNewData: (sourceName, data) => {` | QML-VERIFY T6 event/interaction path |
| [x] | 32 | `exec` — `function exec(cmd) {` | QML-VERIFY T6 event/interaction path |
| [x] | 52 | `execOnce` — `function execOnce(cmd) {` | QML-VERIFY T6 event/interaction path |
| [x] | 57 | `performClick` — `function performClick() {` | QML-VERIFY T6 event/interaction path |
| [x] | 61 | `performMouseWheelUp` — `function performMouseWheelUp() {` | QML-VERIFY T6 event/interaction path |
| [x] | 65 | `performMouseWheelDown` — `function performMouseWheelDown() {` | QML-VERIFY T6 event/interaction path |
| [x] | 81 | `wheelStep` — `function wheelStep(delta) {` | QML-VERIFY T6 event/interaction path |
| [x] | 100 | `onTriggered` — `onTriggered: widget.wheelInGesture = false` | QML-VERIFY T6 event/interaction path |
| [x] | 149 | `resetState` — `function resetState(state) {` | QML-VERIFY T6 event/interaction path |
| [x] | 155 | `parseAnsiCode` — `function parseAnsiCode(n, i, tokens, state) {` | QML-VERIFY T6 event/interaction path |
| [x] | 176 | `formatHexInt` — `function formatHexInt(n) {` | QML-VERIFY T6 event/interaction path |
| [x] | 185 | `rgbToHex` — `function rgbToHex(r, g, b) {` | QML-VERIFY T6 event/interaction path |
| [x] | 188 | `parseColorMode` — `function parseColorMode(i, tokens) {` | QML-VERIFY T6 event/interaction path |
| [x] | 218 | `parseAnsiEscape` — `function parseAnsiEscape(codes, state) {` | QML-VERIFY T6 event/interaction path |
| [x] | 258 | `desktopRecolor` — `function desktopRecolor(html, color) {` | QML-VERIFY T6 event/interaction path |
| [x] | 278 | `formatOutputText` — `function formatOutputText(stdout) {` | QML-VERIFY T6 event/interaction path |
| [x] | 319 | `onExited` — `function onExited(cmd, exitCode, exitStatus, stdout, stderr) {` | QML-VERIFY T6 event/interaction path |
| [x] | 341 | `runCommand` — `function runCommand() {` | QML-VERIFY T6 event/interaction path |
| [x] | 346 | `runTooltipCommand` — `function runTooltipCommand() {` | QML-VERIFY T6 event/interaction path |
| [x] | 376 | `onDataChanged` — `onDataChanged: readDebounce.restart()` | QML-VERIFY T6 event/interaction path |
| [x] | 377 | `onRowsInserted` — `onRowsInserted: readDebounce.restart()` | QML-VERIFY T6 event/interaction path |
| [x] | 386 | `onTriggered` — `onTriggered: widget.readOutputs()` | QML-VERIFY T6 event/interaction path |
| [x] | 389 | `readOutputs` — `function readOutputs() {` | QML-VERIFY T6 event/interaction path |
| [x] | 399 | `Component.onCompleted` — `Component.onCompleted: {` | QML-VERIFY T6 event/interaction path |
| [x] | 408 | `Plasmoid.onActivated` — `Plasmoid.onActivated: widget.performClick()` | QML-VERIFY T6 event/interaction path |
| [x] | 413 | `onExpandedChanged` — `onExpandedChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 479 | `onLinkActivated` — `onLinkActivated: Qt.openUrlExternally(link)` | QML-VERIFY T6 event/interaction path |
| [x] | 507 | `onIsVerticalChanged` — `onIsVerticalChanged: output.publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [x] | 522 | `onItemWidthChanged` — `// onItemWidthChanged: console.log('itemWidth', itemWidth, 'implicitWidth', output.implicitWidth, 'contentWidth', output.contentWidth)` | QML-VERIFY T6 event/interaction path |
| [x] | 537 | `onItemHeightChanged` — `// onItemHeightChanged: console.log('itemHeight', itemHeight, 'implicitHeight', output.implicitHeight, 'contentHeight', output.contentHeight)` | QML-VERIFY T6 event/interaction path |
| [x] | 550 | `onHoveredChanged` — `onHoveredChanged: {` | QML-VERIFY T6 event/interaction path |
| [x] | 574 | `onClicked` — `onClicked: (mouse) => {` | QML-VERIFY T6 event/interaction path |
| [x] | 582 | `onWheel` — `onWheel: (wheel) => {` | QML-VERIFY T6 event/interaction path |
| [x] | 607 | `onAdvanceWidthChanged` — `onAdvanceWidthChanged: output.publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [x] | 617 | `onAdvanceWidthChanged` — `onAdvanceWidthChanged: output.publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [x] | 619 | `publishGeometry` — `function publishGeometry() {` | QML-VERIFY T6 event/interaction path |
| [x] | 635 | `onWidthChanged` — `onWidthChanged: publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [x] | 636 | `Component.onCompleted` — `Component.onCompleted: publishGeometry()` | QML-VERIFY T6 event/interaction path |
| [x] | 643 | `onLinkActivated` — `onLinkActivated: Qt.openUrlExternally(link)` | QML-VERIFY T6 event/interaction path |
| [x] | 700 | `onDesktop` — `readonly property bool onDesktop: plasmoid.formFactor === PlasmaCore.Types.Planar` | QML-VERIFY T6 event/interaction path |
| [x] | 705 | `onClicked` — `onClicked: widget.expanded = false   // middle-click again un-pins` | QML-VERIFY T6 event/interaction path |
| [x] | 713 | `onWheel` — `onWheel: (wheel) => {` | QML-VERIFY T6 event/interaction path |
| [x] | 774 | `onLinkActivated` — `onLinkActivated: Qt.openUrlExternally(link)` | QML-VERIFY T6 event/interaction path |

## Shell/package callable inventory


### `tools/qml_verify.sh`

- [x] line 6 `usage` — help/default/manual-mode contract checked.
- [x] line 39 `cleanup` — smoke proves daemon/applet termination and disposable-root removal.
- [x] line 108 `wait_for_file` — isolated daemon publication startup gate checked.


### `install.sh`

- [x] No declared function: shell syntax + full disposable DESTDIR scenario; real host install waived by D005.

### `packaging/pirostats-launcher`

- [x] No declared function: staged executable exports packaged asset root and `exec`s the Rust binary.

### `packaging/aur/PKGBUILD`

- [x] `pkgver` — tagged/untagged fallback retained; shell syntax checked.
- [x] `build` — locked release build with runtime-loaded NVML feature passed.
- [x] `package` — sourced directly into a disposable `/tmp` package root; native manifest audited.

### `packaging/aur/pirostats.install`

- [x] `post_install` — native dependency/help text and shell syntax checked.
- [x] `post_upgrade` — restart guidance and shell syntax checked.
- [x] `pre_remove` — user-config preservation guidance and shell syntax checked.

### `tools/p6_package_test.sh`

- [x] `stage_native` / `assert_native_layout` / `stage_python_rollback` — disposable install, repeat upgrade, Python rollback, uninstall, user-file preservation, and AUR package-function manifest pass.

### `uninstall.sh`

- [x] No declared function: shell syntax + disposable DESTDIR removal scenario; real host service removal waived by D005.

## Rust callable inventory

Mirrors the Python ledger for production Rust callables. Each entry's `Lane` is
the migration lane that established the final shape; `SCAFFOLD` rows are shared
contracts now consumed by the completed implementation. Evidence codes follow the same
legend as the rest of this file (U/D/F/I/L/P + E0–E5).

### `rust/src/lib.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `run` | function | `SCAFFOLD` | U/I: dispatch tests cover help/version and every production command |

### `rust/src/error.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `Error` | enum | `SCAFFOLD` | U: `Cli`, `Config`, and contextual `Runtime` variants |
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
| [x] | `DbusArgument` / `DbusRequest` | enum/struct | `INTEGRATION/POWER` | U/F: exact typed method arguments and per-call timeout are recorded by `FakeDbus`; POWER asserts `Properties.Get/GetAll` and 15-second `SmartUpdate` requests |
| [x] | `BoundaryError` | enum | `INTEGRATION/HID` | U: promoted shared boundary error contract for command/D-Bus production traits and fixture failures; HID adds typed absent/open/write context through `HidFailed` |
| [x] | `CommandRunner` / `DbusFacade` | traits | `INTEGRATION` | U: promoted production boundary traits implemented by fakes; command contract records exact program/argv/timeout (`3s` network, `5s` pages) |
| [x] | `NotificationPayload` / `NotificationUrgency` / `NotificationTimeout` | struct/enums | `INTEGRATION/NOTIFY` | U/F E1: typed production payload preserves exact title/body/icon plus critical urgency and never-expire policy |
| [x] | `NotificationError` / `NotificationFacade` | struct/trait | `INTEGRATION/NOTIFY` | U/F: explicit service failure contract shared by production and fake; notification pass records degradation and continues |
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
| [x] | `CounterRateState` / `GpuCache` | structs | `INTEGRATION` | U: typed diff/cache state shared by active collectors |
| [x] | `NotificationLatch` / `NotificationState` | structs | `NOTIFY` | U/D: sustained and edge state, per-device ownership, default invariants, and retention after device removal match Python |
| [x] | `DaemonStateSnapshot` | struct | `INTEGRATION/NOTIFY` | U: default/invariant tests cover typed cross-poll state, daemon-owned notification latches, and retained page/poll bookkeeping |

### `rust/src/notify.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `NotificationFailure` / `NotificationReport` | structs | `NOTIFY` | U/F: ordered attempted/failure accounting exposes adapter degradation without rolling back state |
| [x] | `sustained` | function | `NOTIFY` | U/D: monotonic hold, zero hold, hysteresis, recovery, dip reset, and one-send-per-episode parity |
| [x] | `check_and_notify` | function | `NOTIFY` | U/D/F E1: all ten alert types, exact payload order/metadata/text, boundaries, exclusions, state retention, disable/absence, and facade failure |

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
| [x] | `PanelFormatter` | struct | `FORMATTER` | U/D: borrowed config/hardware formatter for main panel/tooltip parity |
| [x] | `PanelFormatter::new` / `with_now_unix` / `format_panel` / `format_tooltip` / `canonical_width` | methods | `FORMATTER` | U/D: shipped panel H/V + tooltip goldens, deterministic battery alternation, and canonical-width guard |
| [x] | `PanelFormatter::build_entries` + item-render helper family | methods | `FORMATTER` | U/D: section collapse, titles, separators, regular/irregular rows, paired rows, batteries, dual-rate rows, and formatter-owned dispatch from `src/formatter.py` |

### `rust/src/render/chart.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `RGBA` + palette constants (`GRID`, `LABEL`, `BLUE_*`, `PURPLE_*`, `GREEN_*`, `ORANGE_LINE`, `TEAL_*`, `RED_LINE`) | type alias + consts | `CHART` | U: stable color contract mirrors `src/chart.py`'s baked tooltip graph palette |
| [x] | `AreaChartOptions` | struct | `CHART` | U: defaults mirror `src/chart.py` keyword defaults (`vmax`, colors, grid levels, left_pad, overlay, label_values) |
| [x] | `encode_png` | function | `CHART` | U/D: Rust PNG round-trip test validates scanline filter bytes, chunk order, CRCs, and decoded RGBA reconstruction for the `_encode_png` parity slice |
| [x] | `area_chart_png` | function | `CHART` | U/D: fixed Python decoded-pixel CRC corpus covers empty/overlay/single/constant charts, clipped labels, fill, line AA, overlay, and repeated-call determinism |

### `rust/src/page_commands.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `Page` / `PageSource` / `PageCommandSpec` / `PageRenderKind` / `PageColorizer` | structs/enums | `PAGES` | U/D: exhaustive registry metadata mirrors every current Python page |
| [x] | `CommandLookup` / `PageCommandCache` / `PageCommandContext` / `PageEnvironment` | structs | `PAGES` | U/F: injected executable/proc/services/clock/cache boundaries; no host commands in tests |
| [x] | `build_pages` | function | `PAGES` | U/D: full page first, configured order and duplicates retained, unknown ids skipped |
| [x] | `run_command` | function | `PAGES` | U/D/F: exact argv/timeout, PTY/fallback, ANSI cleanup, stdout/stderr, no output, cache hit/expiry, missing/error matrix |
| [x] | `text_to_mono_html` / `text_width` / formatting helper family | functions | `PAGES` | U/D E0: fixed Python HTML/width corpus including spacing, SGR, ellipsis, proc cmdline, services, exposed sockets |
| [x] | `format_connections` / `page_inner` / `title_html` / `pager_html` | functions | `PAGES` | U/D E0: exact Python connection/fastfetch/title/pager bytes and canonical-width behavior |
| [x] | `default_click` / `top_process_page_rows` | functions | `PAGES` | U/D: stable `plasma-systemmonitor` argv and 15-row process-page bound |

### `rust/src/render/pages.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `PageFormatter` | struct | `PAGES` | U/D: borrowed config/hardware deep-dive formatter performs no I/O |
| [x] | `PageFormatter::new` / `format_page` / `format_cpu_cores` / `format_top_process` | methods | `PAGES` | U/D E0: exact Python shell, populated/no-data CPU/process HTML, escaping, classes, width, pager, 15-row cap |
| [x] | `PageFormatter::format_graphs` + graph helper family | methods/functions | `PAGES` | U/D/E2: image dimensions/count, legends, threshold classes, NVIDIA preference/Intel fallback/network gate atop CHART pixel corpus |

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
| [x] | `FakeNotificationFacade` | struct | `FIXTURES/NOTIFY` | U/F E1: ordered full-payload recording and queued results; records failed attempts; no real desktop calls |
| [x] | `FakeCommandRunner` / `CommandCall` + re-exported `CommandRunner` trait | structs + trait | `FIXTURES`/`INTEGRATION` | U: argv-keyed FIFO output/error queues + exact program/argv/timeout call trace + `next_call`; implements production `CommandRunner` |
| [x] | `FakeDbus` + re-exported `DbusFacade` trait | struct + trait | `FIXTURES`/`INTEGRATION` | U: signature-keyed reply FIFO + exact `DbusRequest` trace including typed arguments and timeout; implements promoted `domain::boundary::DbusFacade` |
| [x] | `FixtureLoader` + `OracleFixtureRaw` | struct + struct | `FIXTURES` | U: `load_text`/`load_bytes`/`load_oracle_fixture`; raw `toml::Value` preserves the shared oracle schema without coupling to production snapshots |
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
| [x] | `NetworkState::net_up_history` / `net_down_history` | methods | `SENSOR-NET` | U: read-only history exposure consumed by graph rendering |
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
| [x] | `DiskKind` / `DiskIdentity` | enum/struct | `SENSOR-DISK` | U: stable disk identity consumed by power and formatting; NVMe vs ATA + rotational flag |
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

### `rust/src/sensors/gpu_nvidia.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `NvidiaMetrics` / `NvmlError` / `NvmlFacade` | struct/enum/trait | `GPU` | U/F: typed NVML success shape, permanent initialization failure, retryable read failure, and optional fan/decoder absence |
| [x] | `detect_nvidia` | function | `GPU` | U/F: explicit sys-root PCI walk requires NVIDIA vendor plus display class; missing/malformed trees are absent hardware |
| [x] | `gpu_cache_ttl` / `nvidia_cap` | functions | `GPU` | U/D: exact 0s NVML versus 3s fallback selection and Python-compatible optional 99 clamp |
| [x] | `read_nvidia` / `read_nvidia_smi` | functions | `GPU` | U/D/F: NVML selection/failure/cache state plus exact fallback argv, timeout, CSV order, malformed/absent/error handling |
| [x] | `sample_gpu_history` | function | `GPU` | U/D: graphs gate, NVIDIA preference, Intel fallback, cadence, decoder zero-fill, gap exposure, and bounded history |
| [x] | `parse_nvidia_smi` / `parse_metric` / `cap_metrics` / `metrics_from_cache` / `store_metrics` / `history_due` / `trim_to_len` | private helpers | `GPU` | U: exercised through focused parser, cache, failure, cadence, and history tests |

### `rust/src/sensors/power.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `BoltBattery` / `BoltBatteryFacade` | struct/trait | `POWER/HID` | U: POWER-owned HID++ facade implemented by `hid::BoltHidFacade`; `Ok(None)` = no battery level (including timeout/unsupported response), `Err` = discovery/open/write failure (drives timestamp/no-retry split) |
| [x] | `upower_enumerate` | function | `POWER` | U/D/F: `EnumerateDevices` system-bus call decoding flat `[path, ...]` body; empty list on any failure |
| [x] | `upower_device_props` | function | `POWER` | U/D/F: models Python's `proxy.get_cached_property` round-trip as one `GetAll` call with interleaved `[k, v, ...]` body; decodes Percentage/State/EnergyRate/Model/Type |
| [x] | `find_battery_sys` | function | `POWER` | U/F: filters and sorts UPower paths containing `/battery_BAT` |
| [x] | `detect_smart_disks` | function | `POWER` | U/D/F: UDisks2 `GetManagedObjects` walk with partition/optical/empty-drive/missing-drive filtering plus sysfs rotational; replaces SENSOR-DISK's sysfs-only identity view with the typed `SmartDisk` shape |
| [x] | `read_disk_smart` / `read_disk_smart_cached` | functions | `POWER` | U/D/F: `SmartUpdate` + NVMe `SmartCriticalWarning`/ATA `SmartFailing` decode; label-keyed TTL cache (per-drive interval) caches None until TTL elapses |
| [x] | `sysfs_bat_read` / `sysfs_bat_rate` / `sysfs_bat_charge_limit` | functions | `POWER` | U/F: `/sys/class/power_supply/<name>/{capacity,status,power_now,charge_control_end_threshold}` readers; banker's-rounding watts; pulled forward from COLLECTOR because `_read_battery_sys` needs them |
| [x] | `read_battery_sys` | function | `POWER` | U/D/F: sysfs-primary with UPower `GetAll` fallback; zero-rate sysfs `power_now` back-channel when UPower reports 0 while charging/discharging; charge-limit-100 collapse; 30s TTL |
| [x] | `read_battery_periph` | function | `POWER` | U/D/F: UPower `GetAll` Percentage/Model decode; caches model once; suppresses zero/missing charge so the row disappears from the tooltip; 30s TTL |
| [x] | `read_battery_bolt` | function | `POWER` | U/D/F: `BoltBatteryFacade` consumer; caches name+level with 1h TTL; advances timestamp on `Ok(None)` (suppresses wake-up cost); skips timestamp advance on HID failure (immediate retry) |
| [x] | `parse_managed_objects` / `parse_property_map` / `parse_object_paths` / `parse_bool` / `round_half_even_ratio` / `round_half_even_f64` | helpers | `POWER` | U: body decoders + numeric parity helpers; documented D-Bus body encoding (empty-string-separated object chunks, interleaved key/value pairs, flat path lists) |

### `rust/src/sensors/hid.rs`

| Done | Symbol | Kind | Lane | Evidence required |
|---|---|---|---|---|
| [x] | `HidError` | enum | `HID` | U/F: absent/index/open/write variants preserve source/path context and map to `BoundaryError::HidFailed` |
| [x] | `BoltHidFacade` | struct | `HID` | U/F: production `BoltBatteryFacade` implementation with host defaults and fixture-root constructor; absent/open/index outcomes asserted without host I/O |
| [x] | `find_bolt_hidraw` | function | `HID` | U/F: sorted sysfs discovery checks product `c548`, interface `.2`, malformed hierarchy, and fixture dev-root mapping |
| [x] | `transfer` / `feature_index` / `battery_level` / `device_name` / `query_device` | private functions | `HID` | U/D/F: 16-test binary protocol corpus pins report bytes/order, timeout/read bound, short/mismatch behavior, feature absence, name decoding, battery conversion, and combined query shape |

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
| [x] | `PageDirection` | enum | `RUNTIME` | U/I: local `{ Next, Prev }`; `daemon::run_page` owns CLI translation |
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
| [x] | `ConfigError` | enum | `CONFIG` | U/I: `Io`/`Toml` variants promoted through `Error::Config` |
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

Phase 0 must generate a machine-readable AST call-edge report for every Python code file. `tests/test_inventory.py` runs `tools/inventory_ast_reporter.py` across `src`, `tests`, and `tools`, and this table must match the reporter's per-file `Call sites` and `Unique syntactic callees` counts. Dynamic calls/closures are assigned to enclosing symbol and tested by ordered dependency traces. The checked static call totals are:

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
| `tools/python_oracle.py` | 9 | 8 |
| `tools/p6_png_diff.py` | 53 | 34 |
| `tools/qt_shot.py` | 88 | 59 |

Closure requires each current call site/callee family to be marked one of: ported and directly asserted; covered by enclosing differential call trace; preserved QML/tool behavior; intentionally removed with proof of no observable behavior. No unclassified dynamic call remains.
