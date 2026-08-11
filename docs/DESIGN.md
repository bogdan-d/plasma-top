# PlasmaTop design

## Problem and goal

PlasmaTop replaced a shell script that repeatedly started processes for every sensor and render. That design cost roughly 2 W on the original machine. The current system keeps hardware inventory, metric samples, histories, formatting, and owner state behind one current-thread Tokio daemon shell. Plasma receives ready-to-display HTML rather than owning sensor logic or another polling clock.

Compatibility drives the shape: the Rust backend preserves the applet, config, runtime files, CLI, sensor formulas, rendering, and graceful absence behavior.

## Repository architecture

```text
src/
  lib.rs, cli.rs             command parsing and dispatch
  daemon.rs                  lifecycle, reload, poll, publish, shutdown
  diagnostics.rs             render, probe, profiling, list-items
  adapters.rs, adapters/     host clock and owned async I/O services
  domain/                    forms, metrics, tokens, readings, state, boundaries
  config/                    typed TOML, merges, assets, geometry
  sensors/                   discovery, one-attempt reads, and owner state
  render/                    cells, dispatch, formatter, mono layout, pages, chart
  runtime/                   paths, atomic publication, locked page state

plasmoid/                    unchanged Plasma display/interaction boundary
config/, style/, lang/       shipped data and presentation assets
service/, packaging/         native Rust runtime installation
tests/                      integration tests, fixtures, and fixed snapshots
```

`lib.rs` is the process composition root. `daemon.rs` owns the runtime loop; feature modules own their rules and state. Host effects cross explicit traits in `domain/boundary.rs`, with production implementations in `adapters.rs` and deterministic fakes under `test_support/`.

## Runtime protocol

The daemon and applet independently derive `<runtime>` as `$XDG_RUNTIME_DIR/plasma-top`, falling back to `/tmp/plasma-top-$UID`.

```text
<runtime>/
  panel.html                 watched panel output
  tooltip.html               watched tooltip output
  state/
    geom                     usable_px glyph_advance vertical tooltip_advance
    page                     current tooltip page counter
    npages                   published page count
    presented/
      <instance-id>          numeric per-applet presentation lease
```

Only panel and tooltip HTML persist directly in the watched directory. Atomic publication creates a transient PID-qualified sibling, then renames it over the destination. Other churn belongs under `state/`; adding a persistent top-level file would trigger unnecessary applet refresh work. Readers see either complete old or complete new content. Page updates use `flock` to avoid lost mouse-wheel increments. The daemon watches stable parent directories with nonblocking inotify, debounces each logical source for 50 ms, and rescans and re-arms after overflow, ignored watches, or directory recreation; there is no periodic file polling fallback.

The applet watches this directory and uses `cat` only after a file change. It publishes geometry to `state/geom`, allowing the daemon to auto-fit bars, columns, sparks, and graph pixels. Each presented applet refreshes its numeric lease every 30 seconds; the daemon expires leases after 90 seconds. `display.poll_interval` remains the system's only display cadence.

## Config and assets

Rust parses TOML with `toml` and `serde` into a typed `Config`. Unknown TOML keys remain harmless, while invalid field types produce contextual errors. Reload keeps the last good config rather than replacing live output with a partial state.

Layers apply in this order:

1. shipped or user `config.toml` defaults;
2. the `machines.toml` block selected by DMI detection;
3. horizontal or vertical panel override;
4. live geometry auto-fit.

Surface sections support replacement plus `items_add`, `items_remove`, and `order_add`. Unknown item tokens and tokens placed on an unsupported surface are dropped with warnings. Colors stay in CSS, glyphs in `style/icons.toml`, labels in `lang/*.toml`; config contains behavior and data only.

Styles and config hot-reload. Plasma theme and panel geometry changes also re-resolve output without restarting the daemon.

## Metric × form model

An item is a validated `metric[:form]` token, not a flat implementation name.

- `domain/metric.rs` defines what a metric means, its hardware gate, supported forms, capabilities, and surfaces.
- `domain/form.rs` defines presentation forms and surface eligibility.
- `domain/item.rs` validates tokens.
- `domain/registry.rs` derives capabilities and placement.
- `render/registry.rs` and `render/formatter.rs` select rendered rows.

Real placement is the intersection of metric and form surfaces. The current demand set is derived from the final configured panel and tooltip items, notification requirements, configured graph history, and selected-page work. A new form does not create a second sensor implementation.

## Readings and state

`HardwareInventory` contains the latest discovered paths, devices, and feature flags and is owned by discovery in `src/sensors/discovery.rs`. `DisplaySnapshot` retains the latest independently captured metric samples for publication. CPU, memory, network, disk, process, power, NVIDIA, Intel GPU, external-file, and GPU-history state live in independent bounded owner loops. Each matching sensor module owns its cache reconciliation and source invalidation rules. `src/sensors/coordinator.rs` provides only short-lived borrowed `OwnerRefs`; it owns no domain state. `src/scheduler/` owns deterministic cadence, demand, deadlines, backoff, coalescing, generations, cancellation acknowledgement, and lifecycle policy, while `src/sensors/scheduled.rs` executes one cadence-free attempt for the matching owner. Cancellation retains the owner reservation until the executor acknowledges it, and every queued start is revalidated immediately before I/O. Notification latches remain separate in `src/domain/state.rs`.

Each independently captured value can be represented as a `MetricSample` with its own monotonic capture time, so sample age is not inferred from `DisplaySnapshot::assembled_at`. Owner completions apply only their owned fields after ticket, generation, source, and run-order checks. Publication is an independent scheduler action rather than the end of a collection barrier.

Sensor modules read explicit `/proc` and `/sys` roots and use injected command, typed D-Bus, clock, notification, and HID boundaries. Missing hardware, unavailable services, malformed files, command failures, and permission errors degrade to absent readings where the compatibility contract requires it; one failed sensor must not block later families.

Production orchestration runs on a manually built current-thread Tokio shell with bounded per-owner and completion channels. One explicitly bounded blocking lane isolates process scans, `statvfs`, HID, NVML, graph rasterization, and synchronous command/D-Bus client waits from publication deadlines. The bounded command service runs at most two process groups, drains both pipes concurrently, deterministically retains at most 1 MiB of combined output, and kills and reaps groups on timeout or shutdown. Persistent bounded zbus system/session services provide typed UPower/UDisks requests, desktop notifications, UPower change events, and logind sleep events; the old `busctl`/`notify-send` production transports no longer exist.

## Rendering

The formatter produces `Cell`, `Row`, and `Block` values. Item identity survives to CSS as `.item-<metric>.form-<form>`. Rust assigns semantic `.good`/`.warn`/`.crit`/`.active` classes; CSS owns their colors.

`render/mono.rs` reduces row shapes to five layout plans and aligns columns with monospace `&nbsp;` padding. Rendered panel and tooltip paths must remain table-free: Qt Quick RichText table layout caused severe plasmashell CPU use. See [LAYOUT.md](LAYOUT.md) and [PERFORMANCE.md](PERFORMANCE.md).

Tooltip width is derived from the main page rendered against bounded, maxed readings and floored by `TOOLTIP_WIDTH_FLOOR`. Deep-dive pages and graph PNGs use that width so page changes do not resize the popup. Any new volatile string or width-driving value needs an explicit bound and canonical-width coverage.

Graphs are raster PNGs built in `render/chart.rs` with a small pure-Rust pixel pipeline and `miniz_oxide`; Qt RichText receives a data URI. SVG is avoided because it has crashed plasmashell on this path.

## Tooltip pages

Page zero is the full tooltip. Configured deep pages are `processes`, `cpu_cores`, `connections`, `fastfetch`, and `graphs`. Only the selected page body is built. Commands, process scans, and chart rasterization therefore cost nothing while another page is selected.

Wheel and click actions run `plasma-top page next|prev` and `plasma-top click`. Hover, pinning, and planar/full representation run `plasma-top present <instance-id>` and `plasma-top dismiss <instance-id>`; any live per-applet lease means the tooltip is presented. The daemon watches page and lease state, changes demand, and republishes only the tooltip without running unrelated jobs. Presentation shows the retained `tooltip.html` immediately, then permits one bounded activation refresh. Dismissal stops tooltip builds and writes immediately while a one-second work-only grace avoids cancelling page jobs during brief hover gaps; after the grace, tooltip-only demand and disposable page work stop. Multiple instances aggregate safely because one remaining live lease keeps presentation active.

The daemon and loaded QML must come from the same package version because the lease protocol has no mixed-version readiness fallback. Upgrades leave the old daemon and old loaded QML running until logout; the next login activates both new versions together. Restarting only one side during this window is unsupported.

## Daemon lifecycle

Startup resolves config/assets, creates runtime directories, publishes page metadata, synchronously seeds bounded local `/proc` and `/sys` inventory, initializes the scheduler from that real inventory, and immediately dispatches panel-demand jobs. Command and D-Bus inventory discovery proceeds progressively after first paint only for demanded hardware families. First panel publication occurs when required startup jobs finish or the scheduler's 200 ms deadline is observed; real panel blockers are never replaced by synthetic completions. The normal loop then:

1. converts config, inventory, file, page, time, and lifecycle changes into typed scheduler events;
2. executes emitted start actions one at a time and feeds typed capture, baseline, absence, or failure completions back;
3. evaluates notifications only for accepted notification-eligible captures after first panel publication, independently of an aggregate counter's completion classification;
4. assembles retained samples and renders only when the scheduler emits a publication action;
5. atomically publishes changed panel and tooltip bytes independently;
6. sleeps until the next scheduler deadline, inotify event, I/O completion, or lifecycle signal.

SIGINT and SIGTERM are received by the Tokio shell. UPower changes coalesce into demanded battery and inventory refresh triggers. logind `PrepareForSleep(true)` sends scheduler `Suspend`; the matching false event sends `Resume`, which resets counter baselines and reconciles volatile inventory before dispatch resumes. Shutdown removes live panel and control files, retains the last-good tooltip for immediate activation after restart, and gives I/O services at most 500 ms to cancel commands, kill process groups, and reject pending work. It joins only a finished I/O shell; an over-budget shell is abandoned for process exit and reported as a critical shutdown timeout. The service remains a normal user unit with restart-on-failure.

## Dependencies and verification

Production dependencies are reviewed in `DEPENDENCIES.md`. The crate denies unsafe production code, `unwrap`, `expect`, `todo`, and `unimplemented`. Optional NVML support is feature-gated and falls back non-fatally to `nvidia-smi`.

Rust tests cover domain rules, config, rendering, sensors, adapter traces, runtime concurrency, daemon lifecycle, CLI processes, packaging, and applet integration. Fixed compatibility corpora live with those tests. Full commands live in [DEVELOPMENT.md](DEVELOPMENT.md). Deferred work lives in [`.scratch/`](../.scratch/).
