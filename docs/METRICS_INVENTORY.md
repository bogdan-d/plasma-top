# Metrics acquisition inventory

This document maps PlasmaTop metric samples to their real acquisition paths. Use it to understand source cost, blocking behavior, process dependencies, freshness budgets, and likely optimization targets. Item behavior and presentation remain documented in [ITEMS.md](ITEMS.md); measured and historical costs remain in [PERFORMANCE.md](PERFORMANCE.md).

## Executive summary

PlasmaTop does not use a general system-monitoring crate such as `sysinfo`. Most metric samples are parsed directly from Linux `/proc` and `/sys` files with Rust's standard library. Filesystem capacity uses `nix`'s safe `statvfs` wrapper, Logitech Bolt uses `nix::poll` plus direct `hidraw` I/O, and NVIDIA can use the optional `nvml-wrapper` integration.

Owner dispatch remains serial during issue 04. `src/daemon.rs` stores each domain owner separately; the pure scheduler emits typed jobs, and `src/sensors/scheduled.rs` borrows the matching owner for one cadence-free attempt at a time. Commands and D-Bus calls cross bounded async services while the executor preserves deterministic owner/source ordering:

```text
CPU and panel processes -> memory -> network -> disks/SMART/temperatures/fans -> batteries/HID -> NVIDIA GPU -> Intel GPU -> GPU history -> brightness/status files
```

Every owner source finishes before the next source starts. Loops over mounts, drives, fans, or batteries are represented as independent keyed jobs but still execute sequentially. Command waits and typed D-Bus waits are owned by the Tokio shell, with bounded concurrency and cancellation, while the issue-04 compatibility executor still waits synchronously for each typed reply. Direct file reads, syscalls, NVML calls, and HID reads remain synchronous until owner cutover in issue 05.

The hidden demand set contains resolved panel items, enabled notifications, and configured graph histories. Presented-main demand adds tooltip items, and selected-page demand adds page-owned work. CPU and memory have no unconditional exception. Shared owner reads can feed several metrics, and no owner overlaps itself.

Display publication is a separate scheduler deadline. Fast jobs become due 50 ms beforehand; late jobs carry retained values, missed job/history ticks are skipped, and no catch-up burst or owner overlap occurs. The daemon sleeps until the next scheduler deadline or the 100 ms compatibility observation step.

## Acquisition inventory

### CPU and memory

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| Aggregate CPU usage | `/proc/stat` | Direct Rust file read; percentage from consecutive counter snapshots | Every poll; first sample seeds the diff |
| Per-core CPU usage | Per-core lines in `/proc/stat` | Direct Rust file read; consecutive counter diffs | Every poll only while the `cpu_cores` page is selected and presented |
| CPU history | Aggregate/per-core samples already collected | In-memory vectors | Sampled at `display.history_interval`; bounded by configured consumers |
| CPU temperature | Discovered `/sys/class/hwmon/.../temp*_input` | Direct Rust file read | Every requested poll |
| CPU frequency | `/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq`, then first `cpu MHz` in `/proc/cpuinfo` | Direct Rust file read with procfs fallback | Every requested poll |
| CPU turbo/boost | `/sys/devices/system/cpu/intel_pstate/no_turbo`, then `/sys/devices/system/cpu/cpufreq/boost` | Direct Rust file read | Every requested poll |
| Uptime | `/proc/uptime` | Direct Rust file read | Every requested poll |
| Load average | `/proc/loadavg` | Direct Rust file read | Every requested poll |
| RAM usage and size | `/proc/meminfo` | Direct Rust file read and Linux available-memory formula | Every poll |
| Swap usage | `/proc/meminfo` | Direct Rust file read | Every requested poll |
| Memory history | RAM sample already collected | In-memory vector | Sampled at `display.history_interval`; bounded by configured consumers |

Relevant code: `src/sensors/cpu.rs`, `src/sensors/memory.rs`, and the CPU/memory sections of `src/sensors/mod.rs`.

### Processes

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| Panel top processes | `/proc/[pid]/stat`, `/proc/meminfo` | Direct directory/file scan; CPU percentage from consecutive process samples | 15-second freshness budget; a configured panel item is a real startup blocker |
| Processes tooltip page | `/proc/[pid]/stat` and selected `/proc/[pid]/cmdline` files | Direct directory/file scan owned by the selected page | Updated only while the processes page is selected; separate from the panel process sample |

No `ps`, `top`, or process library is used. Cost scales mainly with process count because the reader walks procfs sequentially.

Relevant code: process owner state and one-attempt reads in `src/sensors/process.rs`, with selected-page orchestration and final display-snapshot assembly in `src/daemon.rs`.

### Network

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| Upload/download rate | `/sys/class/net/<device>/statistics/tx_bytes` and `rx_bytes` | Direct Rust file reads; bytes per second from consecutive snapshots | Every requested poll; first sample seeds the diff; device changes reset it |
| Active interface | `ip route get 8.8.8.8`, with `ip route show default` discovery fallback | External `ip` process | Identity sample has a 10-second freshness budget; each command has a 3-second timeout |
| Local IP | `src` token from `ip route get 8.8.8.8` | External `ip` process | Shared identity sample with a 10-second freshness budget and 3-second timeout |
| Wi-Fi SSID and signal | `iw dev <device> link` | External `iw` process; dBm converted to a clamped percentage | Only for the current wireless interface; shared identity sample with a 10-second freshness budget and 3-second timeout |
| Network history | Rate samples already collected | In-memory vectors | Sampled at `display.history_interval` when required by graph consumers |

Network identity refresh can invoke `ip` followed by `iw` sequentially. Hardware discovery may also invoke both `ip` route forms when the first does not provide a device.

Relevant code: `src/sensors/network.rs` and the network section of `src/sensors/mod.rs`.

### Disks and fans

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| Mount discovery | Configured mounts or `/proc/mounts`; device identity from `/sys` | Direct Rust file and symlink reads | Explicit mounts are fixed; automatic mounts are reconciled at `display.poll_interval` while disk usage is demanded |
| Filesystem usage | Mounted filesystem | `nix::sys::statvfs::statvfs` syscall wrapper | Once per requested mount per poll, sequentially |
| Disk read/write rate | `/proc/diskstats` | Direct Rust file read; sector-counter diff using 512-byte sectors | Every requested poll; first sample seeds the diff; device changes reset it |
| Disk temperature | Discovered `nvme` or `drivetemp` hwmon `temp*_input` files | Direct Rust file read | 30-second freshness budget per drive |
| Fan speed | Discovered hwmon `fan*_input` files | Direct Rust file read | 30-second freshness budget per fan |
| SMART health | UDisks2 `SmartUpdate` and typed property calls | Persistent system zbus service | Per-drive configurable SSD/HDD freshness budget; owner calls are sequential; SMART update timeout is 15 seconds |

SMART acquisition is the disk path with the largest individual timeout. One refresh may require multiple D-Bus calls for each drive, and configured drives are processed one at a time.

Relevant code: `src/sensors/disk.rs`, `src/sensors/hwmon.rs`, and SMART functions in `src/sensors/power.rs`.

### Batteries and HID

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| System battery | `/sys/class/power_supply/<id>/...` | Direct Rust file reads | Preferred path; 30-second freshness budget |
| System battery fallback | UPower properties | Persistent typed system zbus service | Used when sysfs cannot provide the battery; 30-second maximum reconciliation plus signal-triggered refresh |
| UPower mouse/keyboard battery | UPower device properties | Persistent typed system zbus service | 30-second maximum reconciliation plus signal-triggered refresh |
| Logitech Bolt mouse/keyboard battery | `/dev/hidraw*`, discovered through `/sys/class/hidraw` | Direct HID++ report writes/reads using standard file I/O and `nix::poll` | One-hour freshness budget; a configured panel item is a real startup blocker; each report read has a 1-second timeout and a query accepts at most 10 reads |

Production D-Bus calls default to a 5-second timeout unless a request supplies another value. Demanded system and UPower peripheral source inventory is reconciled every 30 seconds; other demanded hardware families use a 60-second reconciliation budget.

Relevant code: `src/sensors/power.rs`, `src/sensors/hid.rs`, and `src/adapters.rs`.

### GPUs

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| NVIDIA temperature, utilization, memory, decoder, fan | NVIDIA Management Library | Optional `nvml-wrapper` feature; library loaded at runtime | Every requested poll; a locally discovered configured panel source is a real startup blocker |
| NVIDIA fallback metrics | `nvidia-smi --query-gpu=... --format=csv,noheader,nounits` | External `nvidia-smi` process | Used when NVML is unavailable or a read fails; 3-second freshness budget and 5-second timeout |
| NVIDIA history | Current NVIDIA sample | In-memory vectors | Sampled at `display.history_interval` when required |
| Intel GPU frequency | Discovered DRM/sysfs frequency file | Direct Rust file read | Every requested poll |
| Intel GPU render/decoder utilization | `/proc/[pid]/fd/*/fdinfo` DRM engine counters associated with the Intel PCI device | Direct procfs scan and consecutive counter diff | 30-second freshness budget; a locally discovered configured panel source is a real startup blocker |

The default Cargo feature set does not enable NVML. Packaging must build with the `nvml` feature to use `nvml-wrapper`; otherwise NVIDIA always uses the `nvidia-smi` fallback.

Relevant code: `src/sensors/gpu_nvidia.rs`, `src/sensors/gpu_intel.rs`, and the `nvml` feature in `Cargo.toml`.

### Other panel and tooltip readings

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| Screen brightness | First usable `/sys/class/backlight/<device>/brightness` and `max_brightness` pair | Direct Rust file reads | Every requested poll |
| Pending system updates | Configured `system_updates.file` | Direct Rust file read of a count produced by another job | Every requested poll; PlasmaTop does not run a package manager |
| Server status | Configured `server_check.file` | Direct Rust file read of a status produced by another job | Every requested poll; PlasmaTop does not perform the network probe |

The update and server-check producers are outside the daemon. Optimizing or changing their schedule must happen in the jobs that write those files, not in sensor collection.

## Deep-dive page inventory

Deep-dive page bodies are built only for the selected page. Page changes are checked at scheduler wakes no more than 100 ms apart, update page demand, and can republish the tooltip without running unrelated jobs. Until the issue-06 presentation lease protocol lands, the production compatibility adapter reports the tooltip as presented.

| Page | Source | Blocking/retention behavior |
| --- | --- | --- |
| Full stats | Current `DisplaySnapshot` | No extra acquisition beyond normal collection |
| Processes | Direct `/proc` scan | Selected-page only; no external process |
| CPU cores | Per-core `/proc/stat` data | Sampling and history enabled only while selected and presented |
| Connections | `ss -4tlnp` | External process attempted on every selected-page render because its freshness budget is zero; the latest successful output is retained after a failed attempt; 5-second timeout |
| Fastfetch | `fastfetch`, optionally wrapped by `script -qec` for terminal behavior | External process only while selected; 30-second freshness budget; 5-second timeout |
| Graphs | Histories already held in memory, then pure-Rust PNG rasterization | Selected-page render only; no acquisition subprocess |

## Rust crates used at acquisition boundaries

| Crate | Acquisition role |
| --- | --- |
| Rust standard library | `/proc` and `/sys` reads, directory walks, symlink inspection, direct file I/O, clocks, and synchronous owner control |
| `nix` | Safe `statvfs`, Bolt HID `poll(2)`, and command process-group kill wrappers; also supports runtime locking/user boundaries |
| `nvml-wrapper` | Optional safe, runtime-loaded NVML integration for NVIDIA metrics |
| `tokio` | Current-thread command, timeout, pipe-drain, bounded-channel, timer, and Unix signal services |
| `zbus` | Persistent typed UPower/UDisks/session-notification calls plus UPower/logind signal streams |

`toml`, `serde`, and `miniz_oxide` are production dependencies but do not acquire metrics; they handle configuration and graph compression.

## External executable inventory

Metric and page acquisition:

| Executable | Purpose | Timeout/freshness budget |
| --- | --- | --- |
| `ip` | Current route, interface, and local IP | 3 seconds per call; identity freshness budget is 10 seconds |
| `iw` | Wi-Fi SSID and signal | 3 seconds per call; identity freshness budget is 10 seconds |
| `nvidia-smi` | NVIDIA fallback metrics | 5 seconds; freshness budget is 3 seconds |
| `ss` | Connections tooltip page | 5 seconds; zero freshness budget; selected-page only |
| `fastfetch` | System-info tooltip page | 5 seconds; 30-second freshness budget; selected-page only |
| `script` | Optional pseudo-terminal wrapper for `fastfetch` | Shares page command's 5-second timeout |

Related external processes that do not acquire normal metrics:

| Executable | Purpose |
| --- | --- |
| `kreadconfig6` | Plasma color-scheme lookup at startup and after theme changes; 2-second timeout |
| `plasma-systemmonitor` | Default click target for tooltip pages; launched on user action |
| `cat` | Applet-side reading of published HTML after watched file changes; not started by sensor collection |

Commands run directly without shell expansion. The exception is the deliberate `script -qec` wrapper used for the fastfetch page when `script` is available.

## Blocking and failure model

The production command handle sends typed program/argv/timeout requests to a bounded Tokio service. At most two child process groups run at once; pipe drains cannot deadlock, retained output is capped, and timeout/shutdown kills the full process group before the direct child is reaped. The synchronous owner compatibility handle waits for the typed service reply, but no production path bypasses the service.

The system and session zbus services own persistent connections and bounded request queues. Calls are limited to two in flight per bus. Disconnected requests fail promptly while one serialized bounded-backoff reconnect continues independently. UPower and UDisks replies remain typed through sensor consumption rather than becoming generic string bodies. Desktop notification `Notify` calls carry the existing title, body, icon, critical urgency, and never-expire timeout directly on the session bus.

Sensor failures are isolated logically: a missing file, malformed value, unavailable service, permission error, command failure, or timeout normally produces `None` or an empty reading, allowing later sensor families to run. This isolation does not make work concurrent; elapsed time before a timeout still delays everything that follows it.

A rough worst-case pass is the sum of sequential slow operations that are both demanded and due under their freshness budgets. Per-command timeout values are ceilings, not expected timings, but multiple D-Bus calls or drives can accumulate beyond one timeout period.

## Optimization map

Measure first with:

```bash
./plasma-top profiling --config config/config.toml
```

Highest-value questions:

1. Which `collect` sections dominate cold and warm profiles on the target machine?
2. Are process count, mount count, drive count, or Intel DRM client count making a direct scan expensive?
3. Are typed D-Bus calls, `ip`, `iw`, or `nvidia-smi` frequently reaching timeout rather than returning quickly?
4. Is release packaging enabling NVML, avoiding the recurring `nvidia-smi` process?
5. Are configured items or notifications requesting capabilities that are not useful on this machine?
6. Is the selected connections page repeatedly running `ss` on its zero freshness budget?

Low-risk optimization levers already supported are removing unused metric capabilities, enabling NVML in packaging, increasing configurable SMART/history intervals where freshness permits, and avoiding expensive deep-dive pages when not needed. Before adding threads or async code, profile whether a specific sequential boundary causes visible latency; concurrency would add state, cancellation, publication-order, and shutdown complexity to a daemon whose common procfs/sysfs reads are normally cheap.

## Source map

- Publication lifecycle, selected-page wake behavior, and final process-page display-snapshot assembly: `src/daemon.rs`
- Pure cadence, demand, deadline, backoff, identity, and lifecycle policy: `src/scheduler/`
- Production job catalog and serial execution: `src/sensors/catalog.rs` and `src/sensors/scheduled.rs`
- Short-lived borrowed owner wiring and collection boundaries: `src/sensors/coordinator.rs`
- Separate owner state and reconciliation interfaces: matching domain modules under `src/sensors/`
- One-attempt result contracts and source reads: `src/sensors/attempts.rs`, `src/sensors/attempts/`, and matching owner modules under `src/sensors/`
- Hardware inventory discovery and reconciliation: `src/sensors/discovery.rs`
- Subprocess, D-Bus, notification, signal, and clock services: `src/adapters.rs` and `src/adapters/`
- Command-backed tooltip pages: `src/page_commands.rs`
- Metric-to-capability mapping: `src/domain/metric.rs` and `src/domain/registry.rs`
- `MetricSample`, `HardwareInventory`, and `DisplaySnapshot` contracts: `src/domain/readings.rs`
- Notification latch state: `src/domain/state.rs`
- Issue-02 collection characterization retained only for owner regression tests: `src/sensors/tests/legacy_collect.rs`
- Current freshness policy and profiling guidance: [PERFORMANCE.md](PERFORMANCE.md)
