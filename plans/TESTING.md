# Parity and test plan

## Testing interpretation

“Test every function call, every file” means complete accountable coverage, not
one low-value assertion per line:

1. Every current Python callable/class appears in `INVENTORY.md` with disposition.
2. Every new Rust callable is exercised directly or through a recorded call edge.
3. Every meaningful branch/state transition has deterministic evidence.
4. Every external call has invocation-contract and result/failure tests.
5. Every tracked file has preserve/port/modify/remove verification.
6. Pure modules target 100% line and branch coverage; I/O/orchestration gaps need
   explicit fixture/live evidence rather than artificial unreachable tests.
7. Coverage percentage never substitutes for parity assertions.

Phase 0 creates a machine-readable callable/call-edge ledger. Rust test reports
update it with test names and evidence. Final cutover requires zero unresolved
ledger entries.

## Exactness levels

| Level | Applies to | Assertion |
|---|---|---|
| E0 byte | deterministic HTML/text, tokens, warnings, config results, page text | exact bytes |
| E1 structure | filesystem events, argv, D-Bus method/property requests, state transitions | exact ordered records |
| E2 image | chart output | exact RGBA pixels/dimensions; compressed bytes where deterministic |
| E3 numeric | live volatile sensors/timings | same formula and declared tolerance/sample rules |
| E4 visual | Qt/Plasma screenshots | fixed environment, exact layout; bounded anti-alias pixel delta |
| E5 performance | startup/RSS/poll/page/Plasma CPU | no material regression against recorded baseline |

Normalization is field-specific and reviewed. Never globally strip whitespace,
sort unordered output, discard stderr, or mask unknown differences.

## Test layers

### T0 — static/build

- Python AST/import compilation, ruff, vulture, shell syntax.
- Rust fmt, check, clippy `-D warnings`, test, doc.
- license/dependency manifest and committed lockfile.
- CSS dark/light selector parity and TOML/language/icon key parity.
- inventory script proves every source/test/asset file is classified.

### T1 — pure unit tests

Directly test parsers, formulas, rendering, merge, thresholds, caches, state
machines, and encoders. Include table tests at every boundary and property tests
for invariants such as widths, wrap, clamp, monotonic histories, and merge order.

### T2 — Python/Rust differential oracle

One fixture feeds both implementations. Compare serialized result records, call
traces, HTML/text, state after call, and warnings/errors. Each callable family
gets:

- normal representative input
- minimum/maximum/boundary input
- missing/empty input
- malformed input
- repeated call/state transition
- ordering and duplicate cases

### T3 — adapter fixtures

Captured `/proc`, `/sys`, command, D-Bus, NVML, and HID inputs. Test production
parsers/adapters against fixture roots/fakes. Assert exact requested path/method/
argv and prove no host access.

### T4 — CLI/process integration

Spawn release/debug test binary with isolated HOME/XDG/runtime and deterministic
adapters. Assert exit code, stdout, stderr, generated files, permissions, and
cleanup for every command/option/error combination.

### T5 — daemon integration

Use fake clock plus isolated runtime. Record ordered events:

```text
reload checks -> rescan decision -> collect calls -> canonical width -> notify
-> panel render/write -> tooltip render/write -> sleep/page wake
```

Test startup, first paint, multiple polls, overruns, malformed reload, recovery,
theme/style/geometry reload, page change, signal cleanup, and adapter failures.

### T6 — Qt/Plasma contract

- `tools/qml_verify.sh --smoke` loads the unchanged applet through
  `plasmawindowed` with the Rust daemon under disposable XDG/runtime roots.
- `tools/qml_verify.sh` runs the same setup interactively for hover, pinning,
  wheel, geometry, and live-update checks; it never installs under `/usr`.
- `tools/qt_shot.py` for panel H/V and every tooltip page.
- dark/light/overlay CSS.
- hover, pin/unpin, desktop background/no-background/outline.
- wheel gesture and concurrent page processes.
- geometry/font/orientation changes.
- watcher event count and lazy tooltip reads.
- verify no `<table>` and no runtime-root state churn.

### T7 — live hardware and soak

Run sanitized probe/shadow sessions across available hardware. Required behavior
for unavailable hardware remains covered by fixtures. Soak includes suspend,
network switching, disk hotplug, service disappearance, config/theme edits, and
long-running histories/cache expiration.

## Existing test migration map

| Current test file | Rust destination/evidence |
|---|---|
| `tests/test_config.py` | config merge/geometry/unit+differential suite |
| `tests/test_formatter.py` | formatter/item/canonical-width suite |
| `tests/test_items.py` | token/metric/placement/capability suite |
| `tests/test_render_model.py` | render model and inline output suite |
| `tests/test_mono_render.py` | five-plan mono serializer suite |
| `tests/test_notifier.py` | notification state-machine suite |
| `tests/test_sensors.py` | disk mount suite; expanded adapter fixture suites |
| `tests/test_golden_render.py` | Python/Rust H/V/tooltip byte snapshots |
| `tests/test_lint.py` | Rust fmt/clippy plus retained Python lint until removal |
| `tests/test_deadcode.py` | Rust dead-code/compiler checks; retained vulture until removal |
| `tests/vulture_whitelist.py` | dynamic-contract audit; remove only with Python |

Existing tests remain active until equivalent Rust evidence exists. Porting does
not justify deleting an oracle assertion.

## External call matrix

Every row requires exact request assertions plus result matrix: success, absent,
permission/connection failure, malformed reply, timeout where possible, and
repeated/cache behavior.

| Boundary | Current calls/paths | Required evidence |
|---|---|---|
| Config files | shipped/XDG config, machines, icons, labels, CSS | precedence, missing, malformed, unreadable, mtime reload |
| Plasma config | appletsrc, kdeglobals, `kreadconfig6` | argv, fallback parser, malformed colors, light threshold |
| Runtime files | panel/tooltip/geom/page/npages/lock/cache | layout, atomicity, concurrent readers/writers, permissions |
| `/proc/stat` | aggregate/per-core jiffies | first/delta/reset/malformed/core changes |
| `/proc/meminfo` | total/available/swap | formula/rounding/missing/zero/large values |
| Process `/proc` | stat, cmdline, fd, fdinfo | PID lifecycle, permissions, escaping, DRM engines |
| `/proc` misc | uptime, loadavg, mount data | malformed/missing/normalization |
| hwmon/sysfs | names, temp/fan inputs | discovery override, labels, milli-unit conversion, cache |
| CPU sysfs | frequency, turbo/boost | primary/fallback, inversion, malformed |
| block sysfs | partitions, device links, rotational, NVMe | topology variants, missing links, identity |
| network sysfs | counters, wireless presence | rates, wrap/reset, interface switch |
| PCI/DRM sysfs | vendor/class/frequency/device | Intel/NVIDIA detection and no false positives |
| backlight/power | brightness and battery files | max zero, charge/rate/limit/state variants |
| `ip` | route/device/IP discovery | exact argv, parser variants, absence/error/timeout |
| `iw` | SSID/signal | exact argv, disconnected/malformed/dBm boundaries |
| `nvidia-smi` | CSV metrics fallback | exact argv, missing fields, unsupported values, timeout |
| `ss` | listening connections | exact argv, process/service parsing, permissions |
| `fastfetch`/`script` | page command/PTY | exact argv, ANSI, TTL, command absence/error |
| click process | plasma-systemmonitor launch | detached argv, launch failure, no wait |
| UPower D-Bus | enumerate/properties | object/type/state/value variants and service absence |
| UDisks2 D-Bus | managed objects/properties/SMART | ATA/NVMe paths, timeout/error/cache/rotation TTL |
| Notifications D-Bus | payload/urgency/timeout | each alert, service failure, edge-only send |
| NVML | init/handle/metrics | success, missing lib/device, unsupported metric, fallback |
| HID | library/device/read/write | packet protocol, short/mismatch/timeout/name/battery |
| external status files | update count/server bool | empty/malformed/unreadable/change |

## Function-call trace requirements

For orchestration functions (`load_config`, `collect`, formatter entry points,
page rendering, notifier pass, daemon iteration, CLI dispatch), fakes record each
dependency call with arguments and result. Differential tests compare ordered
traces where order is observable or stateful. At minimum assert:

- configured capability causes exactly intended calls
- absent capability causes zero calls
- shared capability executes once per collection
- cache hit executes zero underlying reads
- cache expiry executes one read and updates timestamp/value
- failure does not trigger unrelated fallback unless current Python does
- page-only work executes only on active/enabled page according to current rules
- reload rebuilds only owners current Python rebuilds
- render functions perform no hidden I/O

## Renderer corpus

For every valid metric/form token:

1. Render panel H if admitted.
2. Render panel V if admitted.
3. Render tooltip if admitted.
4. Exercise hardware gate false/true.
5. Exercise reading `None`, low, threshold boundaries, high/max.
6. Exercise multi-instance zero/one/odd/even sets.
7. Compare rows/cells/classes and final bytes.
8. Assert canonical width covers widest bounded value.

Additional corpus: every row shape/plan, title/separator sequence, HTML special
characters, unicode glyph widths, bounded long identities, all trace lengths,
and CSS dark/light/overlay combinations.

## Config corpus

- every dataclass/default field
- complete shipped config and minimal config
- unknown scalar/table/item/form/page/language
- machine selectors and first-match order
- recursive replacement and add/remove/order grammar
- horizontal/vertical override
- every geometry rounding boundary and optional fourth field
- user/shipped/explicit asset resolution
- malformed startup versus malformed hot reload
- warning ordering and stderr bytes

## Live hardware matrix

| Capability | Fixture mandatory | Live target before release |
|---|---:|---:|
| CPU/memory/load/process | yes | baseline Linux host |
| Intel GPU | yes | Intel DRM host if available |
| NVIDIA NVML | yes | NVIDIA host if available |
| NVIDIA `nvidia-smi` fallback | yes | NVIDIA host with NVML path disabled |
| UPower system battery | yes | laptop if available |
| UPower peripherals | yes | matching peripheral if available |
| UDisks ATA/NVMe SMART | yes | representative available disks |
| Bolt/HID | yes | device if available |
| wifi/ethernet switching | yes | wifi host plus route switch |
| horizontal/vertical/desktop Plasma | yes | Plasma 6 session |

Unavailable live hardware requires a documented gap, fixture proof, and user
acceptance; never silently skip fixture coverage.

## Performance gates

Compare release Rust candidate to Python baseline on same host/config:

- startup to first paint
- cold/warm `collect`
- full main-page render
- each deep-dive page
- daemon RSS and CPU while tooltip closed/open
- Plasma CPU while tooltip closed/open
- page-command process startup
- subprocess/fork count over representative session

Expected Rust improvement is not a gate; **regression** is. Investigate >10%
repeatable regression in a stable measured phase or any new steady subprocess/
busy-loop/runtime-root churn. Record absolute numbers and methodology.

## Aggregate commands at final cutover

```bash
python3 -m pytest tests/ -v
ruff check .
vulture src/ tests/ pirostats tests/vulture_whitelist.py --min-confidence 60

cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets
cargo doc --manifest-path rust/Cargo.toml --no-deps

bash -n install.sh uninstall.sh packaging/aur/PKGBUILD packaging/aur/pirostats.install
```

Python commands disappear only in Phase 8 after inventory closure.
