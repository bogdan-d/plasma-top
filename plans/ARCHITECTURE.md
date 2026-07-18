# Target Rust architecture

## Design choice

Use one synchronous application crate. Keep Rust under `rust/` while Python is
the oracle. Do not introduce a workspace, async runtime, plugin framework, or
Python FFI unless evidence later proves one necessary.

```text
rust/
  Cargo.toml
  Cargo.lock
  src/
    lib.rs                  composition map only
    main.rs                 process entry
    cli.rs                  argument parsing and dispatch
    error.rs                top-level user-facing error context
    domain/
      mod.rs
      form.rs               Form, Shape, Surface
      metric.rs             Metric, MetricSpec, capabilities, placement
      item.rs               validated metric[:form] token
      readings.rs           Readings and value types
      state.rs              DaemonState and cache state
    config/
      mod.rs                typed Config and validation
      merge.rs              raw TOML/machine/orientation merge
      geometry.rs           appletsrc/geom parsing and auto-fit
      assets.rs             config/style/lang resolution
    render/
      mod.rs                formatter orchestration
      model.rs              Cell, Row, Block, Ident
      registry.rs           item render dispatch
      cells.rs              reusable cell builders
      traces.rs             bar/column/spark/braille
      mono.rs               table-free layout plans and serialization
      chart.rs              deterministic PNG pixels/encoding
      pages.rs              formatted page shells
    sensors/
      mod.rs                discovery/collect composition, capability routing
      source.rs             fixtureable filesystem/clock/command boundaries
      cpu.rs
      memory.rs
      process.rs
      network.rs
      disk.rs
      hwmon.rs
      power.rs
      gpu_intel.rs
      gpu_nvidia.rs
      hid.rs
    runtime/
      mod.rs                runtime/cache paths
      atomic.rs             atomic publication
      page.rs               page state + flock
    page_commands.rs        ss/fastfetch/click execution and formatting
    notify.rs               latches + notification adapter
    daemon.rs               single owner of loop/reload/shutdown
    diagnostics.rs          probe/render/profile/list-items
  tests/
    oracle_*.rs
    cli_*.rs
    runtime_*.rs
    fixtures/
```

Names may change only through integration-owner review. Ownership boundaries,
not exact file count, are important.

## Data flow

```text
CLI
 ├─ page ───────────────────────────────> runtime::page (fast path)
 ├─ diagnostics ─┐
 └─ daemon ──────┴─> config -> capabilities -> discover -> collect
                                                │          │
                                                └-> state <-┘
                                                        │
                                              notify + formatter
                                                        │
                                           atomic panel/tooltip writes
```

## Core modeling

- `Metric`, `Form`, `Shape`, `Surface`, `Capability`, and known page ids are
  enums. Parse strings once at boundaries.
- `Item` is validated at construction (`FromStr`/`TryFrom`), preventing unknown
  metric/form combinations internally.
- Use small newtypes where unit confusion is plausible: percentages, bytes/sec,
  pixel/glyph widths, durations, process ids, temperatures.
- `Readings` retains `Option<T>` where absence is expected and user-visible.
- Expected adapter failures use `Result`; conversion to “sensor absent” happens
  in the subsystem owner with diagnostic policy explicit.
- Do not turn all sensors into one broad trait. Use small injectable boundaries
  (`Clock`, command runner, filesystem root, D-Bus facade) only where fixture
  testing or true polymorphism needs them.
- `DaemonState` owns all cross-poll mutation. Render functions borrow immutable
  config/hardware/readings and perform no I/O.

## Runtime model

Stay synchronous:

- workload is one ordered poll with blocking Linux/D-Bus reads
- current behavior depends on deterministic sequencing and shared mutable state
- slow operations already use demand gates/TTLs
- async would add cancellation and lifecycle complexity without measured gain

Daemon owns startup, signal handling, reload, collection, notification, render,
publication, sleep, and cleanup. Adapters do not spawn unowned background work.

## Adapter strategy

### Linux data

Prefer direct `/proc` and `/sys` parsing to preserve current formulas. A generic
system-information crate is acceptable only after fixture comparison proves
identical semantics for memory availability, mounts, disk identity, rates, and
CPU normalization.

All filesystem readers accept a fixture root or narrow reader abstraction. No
test rewrites host `/proc` or `/sys`.

### D-Bus

Use a blocking D-Bus client. Keep separate facades for UPower, UDisks2, and
freedesktop notifications. Capture decoded fixture shapes at each facade; domain
logic must be testable without a session/system bus.

### Commands

One command-runner boundary records argv, timeout, environment policy, stdout,
stderr, and status. Never build shell strings except where QML itself owns the
existing shell contract. Commands run without shell expansion in Rust.

### NVML/HID

Use safe crates/wrappers with optional compilation/runtime detection. Any
required `unsafe` stays inside adapter modules, gets local `// SAFETY:` evidence,
and receives focused tests/review. Missing library/device remains non-fatal.

## Rendering strategy

Port structure, not Python metaprogramming:

- static metric metadata table or exhaustive matches
- explicit registry dispatch by `(Metric, Option<Form>)`
- reusable cell constructors for regular rows
- dedicated functions for irregular layouts
- one source of truth for row plans in `render::mono`

Avoid trait-object render trees and macro-generated DSLs initially. Current data
set is closed and small; enums/exhaustive matches improve reviewability.

## Error and diagnostic policy

- Library/domain code returns typed errors.
- CLI/daemon edge adds path/device/command context and chooses exit/log/degrade.
- Recoverable config hot-reload errors retain last good state.
- Expected absent hardware is not noisy.
- Unexpected repeated adapter failures are rate-limited/logged, not silently
  swallowed forever.
- No `unwrap`/`expect` in recoverable production paths.
- Panic means violated internal invariant; tests must identify each permitted
  invariant panic, ideally none.

## Dependency policy

Likely categories: CLI parsing, Serde/TOML, blocking D-Bus, Linux syscalls/flock,
PNG/zlib, signal handling, optional NVML/HID. Before adoption, lane owner records:

- package identity/repository/license
- necessity versus stdlib/current dependency
- default features and native/build scripts
- transitive footprint and runtime library requirements
- Arch packaging impact
- exit/replacement cost

Pin through committed `Cargo.lock`. No bulk upgrades during parity work.

## Build and quality gates

Baseline Rust commands:

```bash
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets
cargo doc --manifest-path rust/Cargo.toml --no-deps
```

Adjust `--all-features` only if optional native features cannot coexist; document
the supported feature matrix rather than hiding failures.

