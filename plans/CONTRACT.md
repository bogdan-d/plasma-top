# Compatibility contract

This document defines behavior the Rust backend must preserve. Source comments,
tests, `README.md`, and `docs/` provide rationale; when they disagree, executable
Python behavior plus an explicit decision in `STATUS.md` wins.

## Scope

### Replace

- root `pirostats` Python entry point
- all runtime behavior in `src/*.py`
- Python runtime dependencies (`psutil`, `python-gobject`, `pynvml`) only after
  equivalent Rust paths pass parity gates

### Preserve

- applet id `com.github.lucazade.pirostats`
- QML interaction and visual settings
- TOML schema and layering
- CSS selectors and Qt RichText constraints
- language/icon keys
- systemd service semantics
- user configuration under `~/.config/pirostats`

### Non-goals during parity work

- new metrics, forms, pages, configuration, or themes
- asynchronous runtime or multiple worker services
- replacing QML with Rust/Qt bindings
- changing UI design
- changing polling defaults or cache TTLs
- optimizing behavior without measured evidence
- preserving Python as a production fallback after cutover

## Runtime filesystem protocol

Root: `$XDG_RUNTIME_DIR/pirostats`; fallback outside a login session:
`/tmp/pirostats-$UID`.

```text
<runtime>/                 watched by QML; no extra persistent/churning entries
  panel.html               atomically replaced by daemon
  tooltip.html             atomically replaced by daemon
  state/                   not watched by QML FolderListModel
    geom                   written by QML
    page                   page counter
    npages                 daemon-published wrap bound
    page.lock              advisory flock target
```

Rules:

1. HTML temp files stay on the same filesystem and are renamed over targets.
2. No new runtime-root file may be introduced without QML watcher-cost review.
3. `page` writes use PID-unique temp files and an exclusive `flock`.
4. Missing/malformed `page` defaults to `0`; missing/malformed `npages` to `1`.
5. `page next|prev` wraps modulo `npages`; no-op when `npages <= 1`.
6. SIGTERM/SIGINT cleanup removes published HTML/page metadata as Python does.
7. One-shot render/profile output stays under `/tmp`, never runtime root.

## Geometry protocol

`state/geom`:

```text
<usable_px> <panel_glyph_advance_px> <vertical_0_or_1> [tooltip_glyph_advance_px]
```

- First three fields required; fourth backward-compatible optional.
- Non-positive/malformed values invalidate the record.
- Live geometry wins; `~/.cache/pirostats/geom` is fallback.
- Valid live geometry is mirrored best-effort to cache.
- Orientation controls both panel override merge and formatter layout.
- Auto-fit formulas and rounding must match `src/config.py` exactly.

## Publication and timing

- One authoritative clock: `display.poll_interval`.
- First paint occurs before slow cold sensors by using `skip_slow` behavior.
- Each normal poll: reload checks → peripheral rescan if due → collect → derive
  canonical width → notify → active-page refresh → write panel and tooltip.
- Sleep compensates for work duration.
- During sleep, page counter is checked at roughly 100 ms cadence and tooltip is
  re-rendered immediately on a page change without recollecting all sensors.
- Tooltip file is rendered each poll; QML reads it only while hovered/pinned.

## Configuration contract

Resolution:

1. Explicit `--config`, or user XDG config if present, otherwise shipped config.
2. Machine data: explicit config's sibling `machines.toml`, or shipped plus XDG
   machine files for default resolution.
3. First matching DMI machine override.
4. Orientation override: `[panel_horizontal]` or `[panel_vertical]`.
5. Auto-fit from geometry.
6. Unknown and misplaced items removed with deterministic stderr warnings.

Merge behavior:

- recursive table merge; scalar/list replacement
- `items` replacement plus ordered deduplicated `items_add` and `items_remove`
- `order` plus ordered deduplicated `order_add`
- unknown dataclass/config fields currently ignored; retain during parity phase
- malformed initial config is fatal to the command; malformed hot reload keeps
  the last good config and logs once per changed mtime
- glyphs resolve from XDG `style/icons.toml` then shipped file
- labels resolve from shipped `lang/<language>.toml`

## Metric × form contract

- Tokens are `metric[:form]`; absent form means `value` unless metric owns an
  intrinsic shape.
- `Metric` owns capabilities, hardware gate, allowed forms, intrinsic shape,
  and metric-level surfaces.
- `Form` owns generic rendering form and allowed surfaces.
- Effective placement is metric surfaces intersected with form surfaces.
- Separators are valid section entries but not metrics.
- Capability collection is derived from configured items, enabled notifications,
  and graph-page requirements.
- BAR resolves to `column` on horizontal panels and `bar` on vertical panels.
- CSS identity remains `.item-<metric>.form-<form>`.

## Rendering contract

- Never emit `<table>` in panel/tooltip paths.
- Preserve cell roles, CSS classes, threshold boundaries, HTML escaping, visible
  width calculation, `&nbsp;` padding, global right edge, separator normalization,
  and five row layout plans documented in `docs/LAYOUT.md`.
- Horizontal panel remains one inline `<span>` row.
- Vertical panel/tooltip use table-free `<div>` rows.
- Missing values render or collapse exactly as current gate/item rules dictate.
- Canonical tooltip width covers maximum bounded output for every tooltip item.
- Raw device/SSID strings retain current middle truncation limits.
- Dark/light/overlay CSS comments and whitespace are stripped exactly enough for
  Qt RichText; selector behavior and order remain stable.
- Qt-supported HTML/CSS subset—not browser behavior—is authoritative.

## Page contract

- Page 0 always full stats; configured known page ids follow in order.
- Unknown page ids ignored.
- Processes and CPU-core pages use collected data and formatter styling.
- Connections invokes `ss -4tlnp`; fastfetch uses its current argv/PTY behavior
  and cache; graphs rasterize only while active.
- Command not found, timeout, non-zero exit, malformed output, and empty output
  retain current user-visible messages.
- Pager width and title alignment remain stable across pages.
- Click continues launching the current default action detached.

## Sensor contract

General:

- Produce a fresh `Readings`; mutate only owned `DaemonState`/`HardwareInfo`.
- Missing/unsupported hardware usually maps to `None`/empty and hides gated rows.
- Demand-driven capability checks prevent unrelated I/O.
- Counter rates require two samples, use elapsed monotonic time, reset on device
  changes, and avoid negative/spurious values.
- History cadence is `display.history_interval`, independent of poll cadence.
- TTL sentinels force first read (`-infinity` semantics); cached `None` retry
  behavior must match each existing sensor.
- Percentage rounding, clamping, units, disk identity, mount filtering, process
  CPU normalization, and GPU engine attribution must match Python formulas.

External boundaries requiring explicit parity fixtures:

- `/proc`: stat, meminfo/process stat/cmdline/fd/fdinfo, uptime, loadavg, mounts
- `/sys`: hwmon, CPU frequency/turbo, block topology/rotational, PCI/DRM,
  network, backlight, power supply, hidraw
- D-Bus: UPower, UDisks2, desktop notifications
- commands: `ip`, `iw`, `nvidia-smi`, `ss`, `fastfetch`, `script`,
  `kreadconfig6`, and click target
- optional NVML and HID APIs
- externally written update/server status files

## Notification contract

- Edge-triggered, not level-triggered.
- Temperature/load hold times use monotonic elapsed time.
- Temperature hysteresis and per-device latch ownership remain unchanged.
- Battery charging/zero-value exclusions remain unchanged.
- Notification service failure never crashes daemon.
- Title/body/icon/urgency/timeout match current observable behavior.

## QML contract

Backend rewrite must not require QML changes. Preserve:

- runtime path independently derived from `QStandardPaths::RuntimeLocation`
- stable `cat` command prefixes used by executable DataSource callbacks
- nonce behavior for page/click/tooltip commands
- inotify `FolderListModel` with `*.html` and 50 ms coalescing timer
- lazy tooltip reads, middle-click pinning, wheel gesture grouping
- panel/desktop orientation and appearance behavior
- four-field geometry publication
- ANSI-to-RichText conversion and desktop recoloring

Any QML edit is handled as a separate late lane with Qt screenshot evidence.

## CLI contract

Commands and exit behavior:

- `daemon [--config PATH]`
- `render [--config PATH] [--component ...] [--format ...] [--layout ...] [--page ...]`
- `probe [--config PATH]`
- `profiling [--config PATH]`
- `list-items`
- `page next|prev`
- `click`

Preserve help-visible names, choices, defaults, stdout/stderr routing, diagnostic
file paths, and fast page startup. Add machine-readable/testing-only flags only
if hidden or explicitly accepted; they must not alter normal output.

## Packaging contract

- `/usr/bin/pirostats` remains executable path used by QML/systemd.
- `/usr/lib/pirostats` or an explicitly accepted replacement contains shipped
  assets resolvable independently of current working directory.
- systemd unit starts `pirostats daemon` and retains restart/session behavior.
- user config and cache survive upgrade/uninstall according to current scripts.
- AUR native package must declare concrete architectures, commit `Cargo.lock`,
  build with locked dependencies, and preserve GPL-2.0-or-later obligations.

