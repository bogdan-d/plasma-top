# Performance

Read this before changing daemon polling, sensor caching, command boundaries, or HTML layout. The runtime is Rust-only. Current behavior and historical pre-cutover measurements are separated; old numbers are baselines, not claims about current Rust timings.

## Measure current Rust behavior

`plasma-top profiling` uses `std::time::Instant` around real config loading, hardware discovery, and cold/warm collection. It prints timings and cache state to stdout and never writes daemon runtime files:

```bash
./plasma-top profiling --config config/config.toml
```

For process-level work, compare release binaries and record host, kernel, hardware, enabled config, poll interval, page, and command/service availability. For plasmashell layout cost, use the live applet plus `pidstat`; a headless `QTextDocument` does not reproduce the expensive `QQuickText` path.

Do not copy historical measurements into a current benchmark report. Rerun them.

## Cost ownership

The daemon spends most of its time asleep. Work belongs to four boundaries:

1. `/proc`, `/sys`, and device I/O in `src/sensors/`;
2. timeout-bound commands and `busctl` calls in `src/adapters.rs`;
3. pure Rust formatting and chart rasterization in `src/render/`;
4. Qt RichText parsing/layout in plasmashell.

Formatting is deterministic and allocation-heavy relative to arithmetic, but hardware and process I/O usually dominate. Measure before optimizing either. Failure paths must remain bounded: every external command has a timeout, absent services degrade without retries in a tight loop, and logs stay bounded.

## Pay only for active pages

The daemon publishes tooltip HTML every poll because it cannot see hover state, but it builds only the active page body.

- `processes` uses a page-owned `/proc` diff sample while shown; panel process data keeps its separate 15-second cache.
- `cpu_cores` history is collected only when that page is configured.
- `connections` runs `ss` only while shown.
- `fastfetch` runs only while shown and caches output for 30 seconds.
- `graphs` rasterizes PNGs only while shown; required histories are sampled only when the page is enabled.

Page state is checked during sleep in 100 ms steps. A page change republishes the tooltip without waiting a full `display.poll_interval` and without running a new full collection.

## Current cache policy

Cache timestamps use monotonic `Duration` values. `Option` represents “never sampled”; no numeric timestamp doubles as a sentinel. Diff-based caches holding no real sample retry promptly rather than hiding the first value for a full TTL.

| Reading | Current interval |
|---|---:|
| disk temperature | 30 s |
| fan speed | 30 s |
| NVIDIA NVML | every requested poll |
| `nvidia-smi` fallback | 3 s |
| Intel GPU usage | 30 s |
| system battery | 30 s |
| UPower peripheral battery | 30 s |
| Bolt HID battery | 1 h |
| panel top processes | 15 s |
| network identity/Wi-Fi info | 10 s |
| fastfetch page output | 30 s |
| peripheral discovery | 60 s |

SMART intervals remain configurable by drive class. Histories use `display.history_interval` and trim to the largest enabled consumer.

During the first 90 seconds, `src/daemon.rs` logs when requested slow readings first become available. The boot watch then disables itself, keeping steady-state observability cost negligible.

Canonical tooltip width is currently recomputed from bounded, maxed readings on first paint and each normal poll. This keeps width correct after mounts, hardware, or identity changes. Treat memoization as a future optimization only after profiling proves this render contributes material work.

## Process-backed boundaries

PlasmaTop minimizes subprocess work but is not fork-free.

- Plasma uses `cat` after watched HTML changes.
- D-Bus requests use timeout-bound `busctl --json=short`.
- Notifications use timeout-bound `notify-send`.
- `nvidia-smi` is the cached fallback when NVML is unavailable.
- `ip`, `iw`, `ss`, and `fastfetch` run only for requested capabilities/pages.
- system-update and server checks read files produced by external jobs rather than starting package managers or network probes inside the poll loop.

Hardware presence uses sysfs instead of tools such as `lspci`. Historical measurement found NVIDIA detection through `lspci` took roughly 2000 ms while the equivalent sysfs walk took roughly 2 ms. Keep detection in-process.

## Table-free rendering

This is the largest measured plasmashell optimization and remains load-bearing. With the tooltip open, HTML tables forced Qt Quick RichText to rebalance columns on every changed value.

Historical live-app measurement used `pidstat -p $(pidof plasmashell) 1`, the same applet `cat` path, and one value changing every 1.5 seconds:

| Tooltip content | plasmashell CPU while hovering |
|---|---:|
| 14 `<table>` elements | ~30% in bursts |
| same content flattened to rows | ~1% |
| `<style>` present, no tables | ~1% |
| one 33-row `<table>` | 85–100% |
| tables without percentage widths | ~50% |

Before the rewrite, the normal tooltip reached 15–20% CPU, roughly 300 ms every 1.5 seconds. `src/render/mono.rs` now aligns five row shapes with monospace `&nbsp;` padding and emits no tables. Historical tooltip-open CPU fell to roughly 1–3%.

Do not reintroduce `<table>` on any render path. Keep the 8 px inset in the plasmoid text padding, not a layout table. Validate rendered changes with the real Qt path. Rust unit tests enforce table-free output for mono layouts, pages, and render models; `tools/p6_qt_matrix.sh` covers the Qt RichText path.

`pidstat -h` reports `%CPU` in field 8; `$(NF-1)` is the CPU/core id, not the percentage.

## Watch-driven applet

The applet has no timer. `FolderListModel` watches the runtime directory and coalesces the panel/tooltip rename burst with a 50 ms debounce. One display rate, `display.poll_interval`, avoids timer aliasing and stale frames.

Reading still starts `cat`; the watch aligns that work with actual publication rather than a free-running timer. Tooltip reads remain gated on hover or pin, while panel reads do not. Historical table-free measurements were:

| plasmashell state | CPU |
|---|---:|
| tooltip closed, panel updating | ~1.1% |
| tooltip pinned, reparsed every poll | ~4.9% |

The hover/pin gate saved roughly 3.8% of one core on that machine. These are historical applet measurements, not current Rust daemon benchmarks; the applet contract remains unchanged.

## Historical Python baseline

Measurements below describe the pre-cutover Python daemon on one desktop. They remain useful regression context only.

| Work | Historical time |
|---|---:|
| startup to first `/tmp` write | ~150 ms |
| config load | ~3 ms |
| hardware discovery | ~45–60 ms |
| first collect, format, write | ~60–90 ms |
| Python imports, mostly GI Notify | ~150 ms |
| warm loop work at 1.5 s polling | ~1.5 ms |
| formatting | <1 ms |
| two atomic writes | <1 ms |
| process page body | <0.5 ms |
| CPU-core page body | <0.5 ms |
| connections page body | ~13 ms |
| fastfetch page body | ~24 ms |
| graphs page body | ~33–70 ms |
| page-owned process scan | ~15–20 ms |
| 15 displayed process cmdline reads | ~0.04 ms |
| NVMe temperature read | ~5 ms |
| `psutil.cpu_freq()` | ~2–4 ms |
| common psutil memory/swap/network reads | ~0.4–0.6 ms each |

Relative bottlenecks depended on that Python implementation and host. Use current Rust profiling plus live applet measurements for present decisions.
