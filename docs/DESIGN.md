# PiroStats design

## Problem and goal

PiroStats replaced a shell script that repeatedly started processes for every
sensor and render. That design cost roughly 2 W on the original machine. The
current system keeps discovery, readings, history, formatting, and cache state in
one synchronous Rust daemon. Plasma receives ready-to-display HTML rather than
owning sensor logic or another polling clock.

Compatibility drives the shape: the Rust backend preserves the applet, config,
runtime files, CLI, sensor formulas, rendering, and graceful absence behavior.

## Repository architecture

```text
rust/src/
  lib.rs, cli.rs             command parsing and dispatch
  daemon.rs                  lifecycle, reload, poll, publish, shutdown
  diagnostics.rs             render, probe, profiling, list-items
  adapters.rs                host clock, commands, D-Bus, notifications
  domain/                    forms, metrics, tokens, readings, state, boundaries
  config/                    typed TOML, merges, assets, geometry
  sensors/                   discovery, collection, per-family caches/history
  render/                    cells, dispatch, formatter, mono layout, pages, chart
  runtime/                   paths, atomic publication, locked page state

plasmoid/                    unchanged Plasma display/interaction boundary
config/, style/, lang/       shipped data and presentation assets
service/, packaging/         native Rust runtime installation
tests/                       retained migration evidence and Rust integration tests
```

`lib.rs` is the process composition root. `daemon.rs` owns the runtime loop;
feature modules own their rules and state. Host effects cross explicit traits in
`domain/boundary.rs`, with production implementations in `adapters.rs` and
deterministic fakes under `test_support/`.

## Runtime protocol

The daemon and applet independently derive `<runtime>` as
`$XDG_RUNTIME_DIR/pirostats`, falling back to `/tmp/pirostats-$UID`.

```text
<runtime>/
  panel.html                 watched panel output
  tooltip.html               watched tooltip output
  state/
    geom                     usable_px glyph_advance vertical tooltip_advance
    page                     current tooltip page counter
    npages                   published page count
```

Only panel and tooltip HTML persist directly in the watched directory. Atomic
publication creates a transient PID-qualified sibling, then renames it over the
destination. Other churn belongs under `state/`; adding a persistent top-level
file would trigger unnecessary applet refresh work. Readers see either complete
old or complete new content. Page updates use `flock` to avoid lost mouse-wheel
increments.

The applet watches this directory and uses `cat` only after a file change. It
publishes geometry to `state/geom`, allowing the daemon to auto-fit bars, columns,
sparks, and graph pixels. `display.poll_interval` is the system's only display
clock.

## Config and assets

Rust parses TOML with `toml` and `serde` into a typed `Config`. Unknown TOML keys
remain harmless, while invalid field types produce contextual errors. Reload
keeps the last good config rather than replacing live output with a partial
state.

Layers apply in this order:

1. shipped or user `config.toml` defaults;
2. the `machines.toml` block selected by DMI detection;
3. horizontal or vertical panel override;
4. live geometry auto-fit.

Surface sections support replacement plus `items_add`, `items_remove`, and
`order_add`. Unknown item tokens and tokens placed on an unsupported surface are
dropped with warnings. Colors stay in CSS, glyphs in `style/icons.toml`, labels
in `lang/*.toml`; config contains behavior and data only.

Styles and config hot-reload. Plasma theme and panel geometry changes also
re-resolve output without restarting the daemon.

## Metric × form model

An item is a validated `metric[:form]` token, not a flat implementation name.

- `domain/metric.rs` defines what a metric means, its hardware gate, supported
  forms, capabilities, and surfaces.
- `domain/form.rs` defines presentation forms and surface eligibility.
- `domain/item.rs` validates tokens.
- `domain/registry.rs` derives capabilities and placement.
- `render/registry.rs` and `render/formatter.rs` select rendered rows.

Real placement is the intersection of metric and form surfaces. Collection is
demand-driven from the final configured item set, enabled pages, and notification
requirements. A new form does not create a second sensor implementation.

## Readings and state

`HardwareSnapshot` contains discovered paths, devices, and feature flags.
`ReadingsSnapshot` is the typed value set produced for one poll. Persistent
diffs, histories, timestamps, and caches live in `CollectorState` and
`DaemonStateSnapshot` rather than module globals.

Sensor modules read explicit `/proc` and `/sys` roots and use injected command,
D-Bus, clock, notification, and HID boundaries. Missing hardware, unavailable
services, malformed files, command failures, and permission errors degrade to
absent readings where the compatibility contract requires it; one failed sensor
must not block later families.

Linux D-Bus access currently uses timeout-bound `busctl --json=short` calls.
Desktop notifications use timeout-bound `notify-send`. Page commands and
fallback tools use the same command boundary. These process-backed adapters are
deliberate dependency choices, not hidden shell expansion.

## Rendering

The formatter produces `Cell`, `Row`, and `Block` values. Item identity survives
to CSS as `.item-<metric>.form-<form>`. Rust assigns semantic
`.good`/`.warn`/`.crit`/`.active` classes; CSS owns their colors.

`render/mono.rs` reduces row shapes to five layout plans and aligns columns with
monospace `&nbsp;` padding. Rendered panel and tooltip paths must remain table-free:
Qt Quick RichText table layout caused severe plasmashell CPU use. See
[LAYOUT.md](LAYOUT.md) and [PERFORMANCE.md](PERFORMANCE.md).

Tooltip width is derived from the main page rendered against bounded, maxed
readings and floored by `TOOLTIP_WIDTH_FLOOR`. Deep-dive pages and graph PNGs use
that width so page changes do not resize the popup. Any new volatile string or
width-driving value needs an explicit bound and canonical-width coverage.

Graphs are raster PNGs built in `render/chart.rs` with a small pure-Rust pixel
pipeline and `miniz_oxide`; Qt RichText receives a data URI. SVG is avoided
because it has crashed plasmashell on this path.

## Tooltip pages

Page zero is the full tooltip. Configured deep pages are `processes`,
`cpu_cores`, `connections`, `fastfetch`, and `graphs`. Only the active page body
is built. Commands, process scans, and chart rasterization therefore cost
nothing while their page is inactive.

Wheel and click actions run `pirostats page next|prev` and `pirostats click`.
The daemon checks page state in 100 ms sleep steps and republishes only the
tooltip on a page change, without running a full sensor poll. Middle-click
pinning remains QML-owned.

## Daemon lifecycle

Startup resolves config/assets, creates runtime directories, publishes page
metadata, discovers hardware, performs a fast first collection, computes
canonical width, and writes the first panel/tooltip pair. The normal loop then:

1. checks config/style/theme/geometry changes;
2. rescans timed hardware boundaries when due;
3. collects one fresh readings snapshot;
4. evaluates notifications;
5. renders panel and active tooltip page;
6. atomically publishes changed output;
7. sleeps in page-aware steps until the next poll.

SIGINT and SIGTERM use `signal-hook`; shutdown removes daemon-owned runtime files.
The service remains a normal user unit with restart-on-failure.

## Dependencies and verification

Production dependencies are reviewed in `rust/DEPENDENCIES.md`. The crate denies
unsafe production code, `unwrap`, `expect`, `todo`, and `unimplemented`. Optional
NVML support is feature-gated and falls back non-fatally to `nvidia-smi`.

Rust tests cover domain rules, config, rendering, sensors, adapter traces,
runtime concurrency, daemon lifecycle, CLI processes, packaging, and applet
integration. Fixed compatibility corpora remain migration evidence. Full commands live in
[DEVELOPMENT.md](DEVELOPMENT.md); migration state lives in
[`plans/STATUS.md`](../plans/STATUS.md).
