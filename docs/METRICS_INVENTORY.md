# Metrics acquisition inventory

This document maps PlasmaTop readings to their real acquisition paths. Use it to understand poll cost, blocking behavior, process dependencies, cache boundaries, and likely optimization targets. Item behavior and presentation remain documented in [ITEMS.md](ITEMS.md); measured and historical costs remain in [PERFORMANCE.md](PERFORMANCE.md).

## Executive summary

PlasmaTop does not use a general system-monitoring crate such as `sysinfo`. Most readings are parsed directly from Linux `/proc` and `/sys` files with Rust's standard library. Filesystem capacity uses `nix`'s safe `statvfs` wrapper, Logitech Bolt uses `nix::poll` plus direct `hidraw` I/O, and NVIDIA can use the optional `nvml-wrapper` integration.

Collection is synchronous and single-threaded. `src/daemon.rs` calls `src/sensors/mod.rs::collect` once per normal poll, and `collect` runs requested families in this order:

```text
CPU -> memory -> network -> disks -> batteries -> NVIDIA GPU -> Intel GPU -> brightness/status files
```

Every source finishes before the next source starts. Loops over mounts, drives, fans, or batteries are also sequential. Direct file reads, syscalls, NVML calls, HID reads, and subprocess waits therefore block the daemon thread. A blocked pass delays publication, page-state checks, and shutdown observation until the current operation returns.

Collection is demand-driven. Resolved panel and tooltip items, enabled notifications, and configured graph pages determine required capabilities. CPU and memory baselines are always read; unrequested optional capabilities do no sensor work. Shared reads execute once per pass and feed every item that needs them.

After collection and rendering, the daemon sleeps only for the remainder of `display.poll_interval`. If work consumes the whole interval, the next poll starts immediately rather than overlapping with the previous one.

## Acquisition inventory

### CPU and memory

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| Aggregate CPU usage | `/proc/stat` | Direct Rust file read; percentage from consecutive counter snapshots | Every poll; first sample seeds the diff |
| Per-core CPU usage | Per-core lines in `/proc/stat` | Direct Rust file read; consecutive counter diffs | Every poll when the `cpu_cores` page is configured; skipped during fast first paint |
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
| Panel top processes | `/proc/[pid]/stat`, `/proc/meminfo` | Direct directory/file scan; CPU percentage from consecutive process snapshots | 15-second cache; skipped during fast first paint |
| Processes tooltip page | `/proc/[pid]/stat` and selected `/proc/[pid]/cmdline` files | Direct directory/file scan owned by the active page | Updated only while the processes page is active; separate from panel cache |

No `ps`, `top`, or process library is used. Cost scales mainly with process count because the reader walks procfs sequentially.

Relevant code: `src/sensors/process.rs` and active-page handling in `src/daemon.rs`.

### Network

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| Upload/download rate | `/sys/class/net/<device>/statistics/tx_bytes` and `rx_bytes` | Direct Rust file reads; bytes per second from consecutive snapshots | Every requested poll; first sample seeds the diff; device changes reset it |
| Active interface | `ip route get 8.8.8.8`, with `ip route show default` discovery fallback | External `ip` process | Identity cache refreshes every 10 seconds; each command has a 3-second timeout |
| Local IP | `src` token from `ip route get 8.8.8.8` | External `ip` process | Shared 10-second identity cache and 3-second timeout |
| Wi-Fi SSID and signal | `iw dev <device> link` | External `iw` process; dBm converted to a clamped percentage | Only for a wireless active interface; shared 10-second identity cache and 3-second timeout |
| Network history | Rate samples already collected | In-memory vectors | Sampled at `display.history_interval` when required by graph consumers |

Network identity refresh can invoke `ip` followed by `iw` sequentially. Hardware discovery may also invoke both `ip` route forms when the first does not provide a device.

Relevant code: `src/sensors/network.rs` and the network section of `src/sensors/mod.rs`.

### Disks and fans

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| Mount discovery | Configured mounts or `/proc/mounts`; device identity from `/sys` | Direct Rust file and symlink reads | Resolved when disk usage is requested |
| Filesystem usage | Mounted filesystem | `nix::sys::statvfs::statvfs` syscall wrapper | Once per requested mount per poll, sequentially |
| Disk read/write rate | `/proc/diskstats` | Direct Rust file read; sector-counter diff using 512-byte sectors | Every requested poll; first sample seeds the diff; device changes reset it |
| Disk temperature | Discovered `nvme` or `drivetemp` hwmon `temp*_input` files | Direct Rust file read | 30-second cache per drive |
| Fan speed | Discovered hwmon `fan*_input` files | Direct Rust file read | 30-second cache per fan |
| SMART health | UDisks2 `SmartUpdate` and property calls | External `busctl --system --json=short` processes through the D-Bus facade | Per-drive configurable SSD/HDD cache interval; calls are sequential; SMART update timeout is 15 seconds |

SMART acquisition is the disk path with the largest individual timeout. One refresh may require multiple D-Bus calls for each drive, and configured drives are processed one at a time.

Relevant code: `src/sensors/disk.rs`, `src/sensors/hwmon.rs`, and SMART functions in `src/sensors/power.rs`.

### Batteries and HID

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| System battery | `/sys/class/power_supply/<id>/...` | Direct Rust file reads | Preferred path; 30-second cache |
| System battery fallback | UPower properties | External `busctl --system --json=short` processes through the D-Bus facade | Used when sysfs cannot provide the battery; 30-second cache |
| UPower mouse/keyboard battery | UPower device properties | External `busctl --system --json=short` processes through the D-Bus facade | 30-second cache |
| Logitech Bolt mouse/keyboard battery | `/dev/hidraw*`, discovered through `/sys/class/hidraw` | Direct HID++ report writes/reads using standard file I/O and `nix::poll` | One-hour cache; skipped during fast first paint; each report read has a 1-second timeout and a query accepts at most 10 reads |

Production D-Bus calls default to a 5-second timeout unless a request supplies another value. Peripheral discovery is retried at most every 60 seconds when requested hardware remains unresolved.

Relevant code: `src/sensors/power.rs`, `src/sensors/hid.rs`, and `src/adapters.rs`.

### GPUs

| Reading | Primary source | Method | Normal cadence and notes |
| --- | --- | --- | --- |
| NVIDIA temperature, utilization, memory, decoder, fan | NVIDIA Management Library | Optional `nvml-wrapper` feature; library loaded at runtime | Every requested poll; skipped during fast first paint |
| NVIDIA fallback metrics | `nvidia-smi --query-gpu=... --format=csv,noheader,nounits` | External `nvidia-smi` process | Used when NVML is unavailable or a read fails; 3-second cache and 5-second timeout |
| NVIDIA history | Current NVIDIA sample | In-memory vectors | Sampled at `display.history_interval` when required |
| Intel GPU frequency | Discovered DRM/sysfs frequency file | Direct Rust file read | Every requested poll |
| Intel GPU render/decoder utilization | `/proc/[pid]/fd/*/fdinfo` DRM engine counters associated with the Intel PCI device | Direct procfs scan and consecutive counter diff | 30-second cache; skipped during fast first paint |

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

Deep-dive page bodies are built only while active. Page changes are checked during the daemon's sleep in 100 ms steps and can republish the tooltip without another full collection.

| Page | Source | Blocking/caching behavior |
| --- | --- | --- |
| Full stats | Current `ReadingsSnapshot` | No extra acquisition beyond normal collection |
| Processes | Direct `/proc` scan | Active-page only; no external process |
| CPU cores | Per-core `/proc/stat` data | Collection enabled when page is configured; rendering active-page only |
| Connections | `ss -4tlnp` | External process only while active; no cache; 5-second timeout |
| Fastfetch | `fastfetch`, optionally wrapped by `script -qec` for terminal behavior | External process only while active; 30-second cache; 5-second timeout |
| Graphs | Histories already held in memory, then pure-Rust PNG rasterization | Active-page render only; no acquisition subprocess |

## Rust crates used at acquisition boundaries

| Crate | Acquisition role |
| --- | --- |
| Rust standard library | `/proc` and `/sys` reads, directory walks, symlink inspection, direct file I/O, subprocess spawning, clocks, and synchronous daemon control |
| `nix` | Safe `statvfs` filesystem-capacity call and `poll(2)` timeout around Bolt HID reads; also supports unrelated runtime locking/user boundaries |
| `nvml-wrapper` | Optional safe, runtime-loaded NVML integration for NVIDIA metrics |
| `wait-timeout` | Bounded synchronous waits for every production subprocess |
| `serde_json` | Decodes `busctl --json=short` output into the internal D-Bus facade format |

`toml`, `serde`, `miniz_oxide`, and `signal-hook` are production dependencies but do not acquire metrics: they handle configuration, graph compression, and signals.

## External executable inventory

Metric and page acquisition:

| Executable | Purpose | Timeout/cache |
| --- | --- | --- |
| `ip` | Active route, interface, and local IP | 3 seconds per call; identity cached 10 seconds |
| `iw` | Wi-Fi SSID and signal | 3 seconds per call; identity cached 10 seconds |
| `nvidia-smi` | NVIDIA fallback metrics | 5 seconds; cached 3 seconds |
| `busctl` | UPower batteries and UDisks2 discovery/SMART | 5-second default per call; SMART update uses 15 seconds; sensor-specific caches apply |
| `ss` | Connections tooltip page | 5 seconds; no cache; active-page only |
| `fastfetch` | System-info tooltip page | 5 seconds; cached 30 seconds; active-page only |
| `script` | Optional pseudo-terminal wrapper for `fastfetch` | Shares page command's 5-second timeout |

Related external processes that do not acquire normal metrics:

| Executable | Purpose |
| --- | --- |
| `notify-send` | Desktop notifications; blocking call with a 5-second timeout |
| `kreadconfig6` | Plasma color-scheme lookup at startup and after theme changes; 2-second timeout |
| `plasma-systemmonitor` | Default click target for tooltip pages; launched on user action |
| `cat` | Applet-side reading of published HTML after watched file changes; not started by sensor collection |

Commands run directly without shell expansion. The exception is the deliberate `script -qec` wrapper used for the fastfetch page when `script` is available.

## Blocking and failure model

The production command runner starts one child and waits synchronously with `wait-timeout`. On timeout it kills and reaps the child, then returns an absent/error result to the caller. The production D-Bus facade is also synchronous because each D-Bus call starts and waits for `busctl`.

Sensor failures are isolated logically: a missing file, malformed value, unavailable service, permission error, command failure, or timeout normally produces `None` or an empty reading, allowing later sensor families to run. This isolation does not make work concurrent; elapsed time before a timeout still delays everything that follows it.

A rough worst-case pass is the sum of sequential slow operations that are both requested and cache-due. Per-command timeout values are ceilings, not expected timings, but multiple D-Bus calls or drives can accumulate beyond one timeout period.

## Optimization map

Measure first with:

```bash
./plasma-top profiling --config config/config.toml
```

Highest-value questions:

1. Which `collect` sections dominate cold and warm profiles on the target machine?
2. Are process count, mount count, drive count, or Intel DRM client count making a direct scan expensive?
3. Are `busctl`, `ip`, `iw`, or `nvidia-smi` frequently reaching timeout rather than returning quickly?
4. Is release packaging enabling NVML, avoiding the recurring `nvidia-smi` process?
5. Are configured items or notifications requesting capabilities that are not useful on this machine?
6. Is an active uncached connections page repeatedly running `ss`?

Low-risk optimization levers already supported are removing unused metric capabilities, enabling NVML in packaging, increasing configurable SMART/history intervals where freshness permits, and avoiding expensive deep-dive pages when not needed. Before adding threads or async code, profile whether a specific sequential boundary causes visible latency; concurrency would add state, cancellation, publication-order, and shutdown complexity to a daemon whose common procfs/sysfs reads are normally cheap.

## Source map

- Poll lifecycle and active-page wake behavior: `src/daemon.rs`
- Collection order and capability gating: `src/sensors/mod.rs`
- CPU, memory, network, disk, process, GPU, power, and HID details: matching modules under `src/sensors/`
- Subprocess, D-Bus, notification, and clock adapters: `src/adapters.rs`
- Command-backed tooltip pages: `src/page_commands.rs`
- Metric-to-capability mapping: `src/domain/metric.rs` and `src/domain/registry.rs`
- Typed per-poll result: `src/domain/readings.rs`
- Persistent collector/cache state: `src/sensors/mod.rs` and `src/domain/state.rs`
- Current cache policy and profiling guidance: [PERFORMANCE.md](PERFORMANCE.md)
