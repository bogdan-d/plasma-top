# Performance

Read this before changing daemon polling, metric-sample retention, freshness budgets, command boundaries, or HTML layout. The runtime is Rust-only. Current behavior and historical pre-cutover measurements are separated; old numbers are baselines, not claims about current Rust timings.

## Measure current Rust behavior

`plasma-top profiling` uses `std::time::Instant` around real config loading, hardware discovery, and cold/warm serial scheduler execution. It prints timings to stdout and never writes daemon runtime files:

```bash
./plasma-top profiling --config config/config.toml
```

Timed profiling runs the production scheduler-aware async owner loop and I/O services while suppressing panel and tooltip file publication. Its disposable root carries only isolated event-protocol state and config copies. `main` is the default timed scenario; `hidden` models panel/notification/history demand, and a configured page id models a presented tooltip with that page selected. Unknown or unconfigured page ids are rejected.

```bash
./plasma-top profiling --config config/config.toml --duration 30
./plasma-top profiling --config config/config.toml --duration 30 --scenario hidden
./plasma-top profiling --config config/config.toml --duration 30 --scenario graphs
```

These ordinary timed commands hold the requested presentation and page state steady for the full measurement. They do not inject page, presentation, or config changes.

The timed report includes per-job captured/baseline/absent/failed/rejected/cancelled/in-flight dispositions, process and D-Bus call counts, queue/run percentiles, maximum age from accepted retained source-capture timestamps, timer wake lateness, first-paint latency, render percentiles separated by publication reason, and scheduled-publication lateness measured after rendering against the scheduler's typed display deadline. Queue latency ends only when work actually begins, so blocking-lane and task-pool admission are included there rather than in run time. Deadline counts are limited to phase-locked display ticks actually skipped, first paints over 250 ms, and scheduled publications over 50 ms; nanosecond timer delay is reported only in wake-lateness percentiles. Runtime-file write timing is reported as not applicable because panel and tooltip writes are deliberately disabled. A duration must be finite and positive; `--scenario` is valid only with `--duration`.

Add `--stimuli` only when measuring page, presentation-control, and config event latency:

```bash
./plasma-top profiling --config config/config.toml --duration 5 --scenario main --stimuli
```

This opt-in run executes one serial bounded sequence through the isolated page, presentation-lease, and copied-config files. Every action waits for the watcher to observe its exact expected page/presentation/config state and, when applicable, for the exact scheduler publication identity to finish tooltip HTML rendering in memory. Protocol-file writes update only requested stimulus state; acceptance, scheduler work, rendering, and stimulus completion use cached watcher/scheduler-observed state. The next action is scheduled relative to that completion, not scheduler boot. A short or slow run reports the pending stimulus identity, expected state, timeout, unstarted actions, and separate protocol-requested and watcher/scheduler-observed states instead of consuming restore actions, claiming an unobserved page, or implying restoration. The report labels aggregate job/call/render counts as covering both the scenario and stimulus work. The user's config and runtime remain untouched.

Page, control, and config latency starts immediately after the external-protocol atomic rename succeeds. It includes inotify observation and debounce, production event/reload and scheduler handling, and ends only at the correlated in-memory tooltip HTML publication. A publication caused by an external style/config source cannot satisfy a stimulus merely because it has the same broad publication reason. Publication bytes remain in memory instead of being written, so this boundary measures the daemon's complete publication work but not plasmashell parsing or QML layout. Each kind reports sample count, p50/p95/p99, and the number over 100 ms.

Shutdown timing starts when the profile deadline is observed, immediately before both the scheduler shutdown event and shell/service shutdown request. It ends only after daemon owner/notification orchestration has terminated, command and D-Bus services have completed their shutdown, and the owned runtime thread has joined or exhausted the same shared 500 ms budget. The report gives the isolated duration and count over 500 ms; total process wall time is not used.

These reports are instrumentation, not benchmark evidence. The retained development-host report predates explicit stimulus opt-in, so its timed scenarios contain mixed stimulus work and are not steady-scenario evidence. A later [AMDGPU release comparison](../.scratch/amdgpu-support/validation.md) records separate steady and stimulus runs against the pre-AMDGPU commit. It is a single development-host pass, not an SLO or power guarantee.

Production command, system-D-Bus, and notification handles contain no profiling field or per-call profiling branch; timed mode wraps them in profiling-only counting facades. Owner timing and in-memory publication suppression still use one optional session check at orchestration boundaries. Those checks do not take clocks or allocate when profiling is absent. Splitting the complete owner/publication loop into duplicate production and profiling implementations would remove those branches at disproportionate maintenance and correctness cost, so the truthful claim is no production profiling collection or hot boundary-adapter branch, not literal zero instructions everywhere.

For process-level work, compare release binaries and record host, kernel, hardware, enabled config, poll interval, page, and command/service availability. For plasmashell layout cost, use the live applet plus `pidstat`; a headless `QTextDocument` does not reproduce the expensive `QQuickText` path.

Do not copy historical measurements into a current benchmark report. Rerun them.

## Cost ownership

The daemon spends most of its time asleep. Work belongs to four boundaries:

1. `/proc`, `/sys`, and device I/O in `src/sensors/`;
2. bounded command and persistent zbus calls in `src/adapters/`;
3. pure Rust formatting and chart rasterization in `src/render/`;
4. Qt RichText parsing/layout in plasmashell.

Formatting is deterministic and allocation-heavy relative to arithmetic, but hardware and process I/O usually dominate. Measure before optimizing either. Failure paths must remain bounded: every external command has a timeout, absent services degrade without retries in a tight loop, and logs stay bounded.

## Pay only for the selected page

The scheduler models hidden, presented-main, and selected-page demand from the production presentation leases. Hidden demand contains panel metrics, enabled notifications, and configured graph histories; main-tooltip and selected-page jobs are added only while at least one tooltip lease is live. Dismissal stops tooltip builds and writes immediately, leaves the last `tooltip.html` intact, and gives active tooltip-only work a one-second grace before cancellation.

- `processes` uses a page-owned `/proc` diff sample while selected; panel process data has a separate 15-second freshness budget.
- `cpu_cores` sampling and history run only while that page is selected and presented by scheduler input.
- `connections` runs `ss` only while selected.
- `fastfetch` runs only while selected and has a 30-second freshness budget.
- `graphs` rasterizes PNGs only while selected; required histories are sampled while the page is configured.

Selected-page state is watched through nonblocking inotify with a 50 ms logical-source debounce. A page change updates demand and targets republished tooltip HTML in under 100 ms without waiting for `display.poll_interval` or running unrelated work.

## Current sample and freshness policy

Metric-sample capture times and attempt times use monotonic `Duration` values. `Option` represents “never sampled”; no numeric timestamp doubles as a sentinel. The scheduler phase-locks normal deadlines, starts fast jobs 50 ms before publication, skips missed ticks, and gives diff-based owners a prompt baseline retry. Bounded exponential failure backoff never exceeds the normal freshness budget.

| Metric sample | Current freshness budget |
|---|---:|
| disk temperature | 30 s |
| fan speed | 30 s |
| NVIDIA NVML | every requested poll |
| `nvidia-smi` fallback | 3 s |
| AMDGPU usage, codec, VRAM, graphics clock | every requested poll |
| AMDGPU temperature, power, fan RPM | 30 s |
| Intel GPU usage | 30 s |
| system battery | 30 s |
| UPower peripheral battery | 30 s |
| Bolt HID battery | 1 h |
| panel top processes | 15 s |
| network identity/Wi-Fi info | 10 s |
| fastfetch page output | 30 s |
| system and UPower peripheral source reconciliation | 30 s |
| other demanded hardware inventory reconciliation | 60 s |
| automatic mount reconciliation | `display.poll_interval` |

SMART intervals remain configurable by drive class. Histories use `display.history_interval` and trim to the largest enabled consumer.

AMDGPU uses one fast and one slow job on a serialized owner. Jobs read only demanded supported fields; configured graphs keep usage and codec history active while the tooltip is hidden, without demanding temperature, power, fan, clock, or VRAM. Each field retains its own successful sample and capture time across failures. Confirmed capability loss clears only the affected field; confirmed device replacement clears all AMD samples and resets the selected GPU history. Discovery uses direct DRM/PCI/hwmon inspection and introduces no subprocess.

The 2026-09-05 Strix Halo comparison recorded 44 AMD fast captures, 43 GPU history captures, two AMD inventory passes, and no AMD slow job in a 65-second hidden run. Subprocess counts matched the unchanged baseline in hidden, main, and graphs scenarios. Both revisions reported no skipped display deadlines or publications over 50 ms. See the [report](../.scratch/amdgpu-support/validation.md) for configuration, latency distributions, workload response, and measurement limits. Newly restored AMD config demand took about one poll to reappear in a separate live-daemon check.

During the first 90 seconds, `src/daemon.rs` logs when demanded slow metric samples first become available. The boot watch then disables itself, keeping steady-state observability cost negligible.

Canonical tooltip width is recomputed from a bounded, maxed display snapshot on first paint and each normal publication pass. This keeps width correct after mounts, hardware inventory, or identity changes. Memoization is justified only if profiling shows this render contributes material work.

## Async I/O boundaries

PlasmaTop minimizes subprocess work but is not fork-free.

- Plasma uses `cat` after watched HTML changes.
- `nvidia-smi` is the retained fallback sample source when NVML is unavailable.
- `ip`, `iw`, `ss`, and `fastfetch` run only when included in the current demand set.
- system-update and server checks read files initially and after inotify changes rather than starting package managers or network probes.

All daemon and diagnostic subprocesses use one bounded Tokio command service with two child slots. Each child owns a process group, stdout and stderr drain concurrently, final output retains no more than 1 MiB with deterministic stdout-first allocation, and timeout or shutdown kills the group and explicitly reaps the direct child. UPower, UDisks, desktop notifications, and sleep signals use persistent zbus connections instead of helper processes. Each bus permits two in-flight calls; disconnection fails waiting calls promptly and one serialized exponential reconnect loop is capped at two seconds, avoiding request-driven reconnect storms.

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

Do not reintroduce `<table>` on any render path. Keep the 8 px inset in the plasmoid text padding, not a layout table. Validate rendered changes with the real Qt path. Rust unit tests enforce table-free output for mono layouts, pages, and render models; `tools/qt_render_matrix.sh` covers the Qt RichText path.

`pidstat -h` reports `%CPU` in field 8; `$(NF-1)` is the CPU/core id, not the percentage.

## Watch-driven applet and presentation leases

`FolderListModel` watches the runtime directory and coalesces the panel/tooltip rename burst with a 50 ms debounce. The 30-second lease heartbeat while presented is the only steady-state recurring QML timer introduced by the presentation protocol, and it stops while hidden; existing one-shot/debounce, bootstrap-until-first-frame, and wheel-gesture timers retain their separate roles. One display rate, `display.poll_interval`, avoids timer aliasing and stale frames.

Reading still starts `cat`; the watch aligns that work with actual publication rather than a free-running display timer. Tooltip rendering, writes, and reads are gated by hover, pin, or planar/full representation, while panel work is not. On activation QML reads the retained tooltip immediately; the daemon's bounded activation refresh then replaces it when fresh demanded work completes or reaches its 100 ms deadline. Historical table-free measurements were:

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
