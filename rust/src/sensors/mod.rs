//! Linux sensor collection building blocks.
//!
//! Each submodule owns one hardware domain and exposes deterministic,
//! fixture-friendly readers that take explicit proc/sys roots and clock
//! snapshots. This module drives them in compatibility order: one-time
//! [`discover_hardware`], periodic
//! [`rescan_peripherals`] / [`needs_periph_rescan`], and the per-poll
//! capability-gated [`collect`].
//!
//! ## State ownership
//!
//! Domain-specific state structs
//! ([`cpu::CpuState`], [`memory::MemoryState`], [`network::NetworkState`],
//! [`disk::DiskState`], [`process::ProcessState`], [`gpu_intel::IntelGpuState`]),
//! which encapsulate details the typed [`DaemonStateSnapshot`] does not carry
//! (e.g. per-interface rate-device tracking). [`CollectorState`] bundles those
//! states and is their single source of truth.
//!
//! [`DaemonStateSnapshot`] owns POWER battery/SMART caches,
//! GPU ([`gpu_nvidia`] reads `state.gpu_cache`; graphs history lives in
//! `state.gpu_*_history`), and notification latches. The daemon owns one
//! `DaemonStateSnapshot` and lends it to [`collect`] by `&mut`. Fields on
//! `DaemonStateSnapshot` that mirror a domain-specific buffer (cpu/mem/net/disk
//! histories, hd_temp/fan caches, net rate) stay at their default during
//! collection: their [`CollectorState`] counterparts own that data and the fresh
//! [`ReadingsSnapshot`] carries the per-poll copy the formatter/notifier read.

pub mod cpu;
pub mod disk;
pub mod gpu_intel;
pub mod gpu_nvidia;
pub mod hid;
pub mod hwmon;
pub mod memory;
pub mod network;
pub mod power;
pub mod process;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

use crate::config::Config;
use crate::domain::boundary::{ClockSnapshot, CommandRunner, DbusFacade, FilesystemRoots};
use crate::domain::item::ItemToken;
use crate::domain::metric::Capability;
use crate::domain::readings::{
    DiskUsageReading, HardwareSnapshot, LoadAverage, ReadingsSnapshot, TopProcessSummary,
};
use crate::domain::registry::needed_capabilities;
use crate::domain::state::DaemonStateSnapshot;

use crate::sensors::gpu_intel::IntelGpuState;
use crate::sensors::gpu_nvidia::NvmlFacade;
use crate::sensors::power::BoltBatteryFacade;

/// `ip`/`iw` subprocess timeout — mirrors `src/sensors.py`'s `timeout=3`.
pub const NETWORK_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

/// Items whose formatter hardware gate reads `hw.net_device`; as long as one is
/// configured and the device is `None`, peripheral rescan is worth retrying.
/// Mirrors `_NET_GATED_ITEMS` in `src/sensors.py`.
const NET_GATED_ITEMS: &[&str] = &["net_speed", "net_device_ip", "net_ip", "net_device"];

/// Accumulated per-section wall-clock elapsed, keyed by section name.
///
/// Populated only when [`collect`] is given `Some(&mut Timings)` (the profiling
/// subcommand). Deterministic tests pass `None`, so no wall clock is read.
pub type Timings = BTreeMap<String, Duration>;

/// Bundled domain-specific state owned by the collector.
///
/// Build with [`CollectorState::default`] at daemon startup and hold it across
/// polls.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CollectorState {
    /// CPU aggregate/per-core diff + histories.
    pub cpu: cpu::CpuState,
    /// Memory usage history.
    pub memory: memory::MemoryState,
    /// Network identity/rate/history state (encapsulates per-device rate reset).
    pub network: network::NetworkState,
    /// Disk temperature/fan caches + I/O rate state.
    pub disk: disk::DiskState,
    /// Top-process diff/cache state.
    pub process: process::ProcessState,
    /// Intel iGPU engine-diff/cache state.
    pub intel_gpu: IntelGpuState,
}

/// Borrowed roots, boundaries, and clock for one collection pass.
///
/// Grouping the `&mut` boundaries keeps [`collect`]'s parameter list reviewable
/// and makes the daemon's per-poll wiring explicit. The collector never stores
/// the context; it borrows for the duration of one [`collect`] call.
pub struct CollectCtx<'a> {
    /// `/proc` fixture root (production: `/proc`).
    pub proc_root: &'a Path,
    /// `/sys` fixture root (production: `/sys`).
    pub sys_root: &'a Path,
    /// Command runner for `ip`/`iw`/`nvidia-smi`.
    pub commands: &'a mut dyn CommandRunner,
    /// D-Bus facade for UPower/UDisks2.
    pub dbus: &'a mut dyn DbusFacade,
    /// Optional NVML facade. `None` selects the `nvidia-smi` fallback path
    /// (matches Python with `python-nvidia-ml-py` absent).
    pub nvml: Option<&'a mut dyn NvmlFacade>,
    /// Optional Bolt HID facade. `None` suppresses Bolt battery reads.
    pub bolt: Option<&'a mut dyn BoltBatteryFacade>,
    /// Single monotonic/wall snapshot for the whole pass.
    pub clock: ClockSnapshot,
    /// First-paint flag: skip the cache-cold slow sensors.
    pub skip_slow: bool,
}

impl<'a> CollectCtx<'a> {
    /// Builds a context from a [`FilesystemRoots`] plus the boundary trait
    /// objects the daemon holds, defaulting `skip_slow = false`.
    #[must_use]
    pub fn new(
        roots: &'a FilesystemRoots,
        commands: &'a mut dyn CommandRunner,
        dbus: &'a mut dyn DbusFacade,
        clock: ClockSnapshot,
    ) -> Self {
        Self {
            proc_root: &roots.proc_root,
            sys_root: &roots.sys_root,
            commands,
            dbus,
            nvml: None,
            bolt: None,
            clock,
            skip_slow: false,
        }
    }
}

/// No-op when `timings` is `None`; otherwise records accumulated wall-clock
/// elapsed under `key`. Mirrors `timed_section` in `src/sensors.py`.
///
/// Deterministic tests pass `None`, so [`std::time::Instant::now`] is never
/// consulted in the deterministic path. The profiling subcommand passes
/// `Some(&mut Timings)` and reads the accumulated per-section durations.
fn timed<R>(timings: &mut Option<&mut Timings>, key: &str, work: impl FnOnce() -> R) -> R {
    match timings.as_mut() {
        Some(map) => {
            let start = std::time::Instant::now();
            let result = work();
            let elapsed = start.elapsed();
            map.entry(key.to_owned())
                .and_modify(|acc| *acc += elapsed)
                .or_insert(elapsed);
            result
        }
        None => work(),
    }
}

// ── Hardware discovery ───────────────────────────────────────────────────────

/// One-time startup discovery of static hardware paths and presence flags.
///
/// Mirrors `discover_hardware` in `src/sensors.py`: resolves the CPU/disk/fan
/// hwmon paths, enumerates UPower batteries, detects NVIDIA/Intel GPUs, the
/// default-route net device, the root-disk I/O device, backlight/wifi/turbo
/// presence, and (when SMART is enabled) the SMART-capable drives. Peripheral
/// (mouse/keyboard) UPower ids are discovered via the private
/// `find_peripherals` helper.
///
/// `cpu_count` is supplied by the caller (the daemon resolves it via
/// `available_parallelism`); tests pass a fixed value so discovery stays
/// deterministic.
#[must_use]
pub fn discover_hardware(
    sys_root: &Path,
    proc_root: &Path,
    cfg: &Config,
    dbus: &mut dyn DbusFacade,
    commands: &mut dyn CommandRunner,
    cpu_count: usize,
) -> HardwareSnapshot {
    let cpu_paths = cpu::discover_cpu_paths(sys_root, &cfg.sensors);
    let hd_temp_paths = disk::find_hd_temp_paths(sys_root, &cfg.sensors);
    let fan_paths = disk::find_fan_speed_paths(sys_root, &cfg.sensors);
    let battery_sys_ids = power::find_battery_sys(dbus);
    let has_nvidia = gpu_nvidia::detect_nvidia(sys_root);
    let intel = gpu_intel::detect_intel_gpu(sys_root);
    let net_device = detect_net_device(commands);
    let disk_io_device = disk::detect_disk_io_device(proc_root, sys_root, "/");
    let has_backlight = detect_has_backlight(sys_root);
    let has_wifi = network::detect_has_wifi(sys_root);
    let disk_smart_drives = if cfg.disks.smart {
        power::detect_smart_disks(dbus, sys_root)
    } else {
        BTreeMap::new()
    };
    let (battery_mouse_id, battery_kbd_id) = find_peripherals(cfg, dbus);

    HardwareSnapshot {
        // `capabilities`/`metrics` are derived per-poll from config inside
        // `collect` (Python's HardwareInfo has no equivalent fields); left
        // empty so the snapshot only carries discovered hardware.
        capabilities: BTreeSet::new(),
        metrics: BTreeSet::new(),
        cpu_temp_path: cpu_paths.cpu_temp_path,
        cpu_freq_path: cpu_paths.cpu_freq_path,
        hd_temp_paths,
        fan_paths,
        battery_sys_ids,
        has_nvidia,
        intel_gpu_freq_path: intel.freq_path,
        intel_gpu_pci: intel.pci,
        net_device,
        disk_io_device,
        cpu_count: cpu_count.max(1),
        cpu_turbo_supported: cpu_paths.cpu_turbo_supported,
        has_backlight,
        has_wifi,
        battery_mouse_id,
        battery_kbd_id,
        disk_smart_drives,
        periph_scan_at: None,
    }
}

/// Retry discovery of hardware that can appear after startup: UPower
/// peripherals (mouse/keyboard) and the default-route net device.
///
/// Mirrors `rescan_peripherals` in `src/sensors.py`. Existing ids are retained
/// when the rescan finds nothing new; `periph_scan_at` advances to `clock`'s
/// monotonic instant regardless. The net device is retried only while still
/// `None` (the daemon started before the network came up).
pub fn rescan_peripherals(
    hw: &mut HardwareSnapshot,
    cfg: &Config,
    dbus: &mut dyn DbusFacade,
    commands: &mut dyn CommandRunner,
    clock: ClockSnapshot,
) {
    let (mouse, kbd) = find_peripherals(cfg, dbus);
    if mouse.is_some() {
        hw.battery_mouse_id = mouse;
    }
    if kbd.is_some() {
        hw.battery_kbd_id = kbd;
    }
    if hw.net_device.is_none() {
        hw.net_device = detect_net_device(commands);
    }
    hw.periph_scan_at = Some(clock.monotonic);
}

/// Returns `true` when a configured item still wants a peripheral that has not
/// appeared yet (mouse/keyboard UPower id, or the default-route net device).
///
/// Mirrors `needs_periph_rescan` in `src/sensors.py`. Bolt-configured devices
/// never need UPower discovery (they are addressed by index, not enumerated).
#[must_use]
pub fn needs_periph_rescan(hw: &HardwareSnapshot, cfg: &Config) -> bool {
    let wants_mouse = cfg.panel.has("battery_mouse") || cfg.tooltip.has("battery_mouse");
    let wants_kbd = cfg.panel.has("battery_kbd") || cfg.tooltip.has("battery_kbd");
    if wants_mouse && hw.battery_mouse_id.is_none() && cfg.battery.mouse_bolt.is_none() {
        return true;
    }
    if wants_kbd && hw.battery_kbd_id.is_none() && cfg.battery.kbd_bolt.is_none() {
        return true;
    }
    if hw.net_device.is_none()
        && NET_GATED_ITEMS
            .iter()
            .any(|name| cfg.panel.has(name) || cfg.tooltip.has(name))
    {
        return true;
    }
    false
}

// ── Per-poll collection ──────────────────────────────────────────────────────

/// Produces a fresh [`ReadingsSnapshot`] for one poll.
///
/// Mirrors `collect` in `src/sensors.py`. Work is demand-driven: only the
/// capabilities returned by [`needed_capabilities`] (derived from the resolved
/// config items, enabled notifications, and the `graphs` page) trigger reads,
/// and each shared read executes once. Section order matches Python exactly
/// where stateful or observable (history sampling, net-device adoption, call
/// trace). `skip_slow = true` skips the cache-cold slow sensors so the first
/// paint at startup is fast, matching Python's first-paint behavior.
///
/// Mutates only `lanes` (collector-owned lane state), `state`
/// (power/GPU/notification buffers), and `hw` (live net-device adoption). When
/// `timings` is `Some`, accumulates per-section wall-clock elapsed for the
/// profiling subcommand; `None` is zero-overhead and deterministic.
#[allow(clippy::too_many_lines)]
pub fn collect<'a>(
    lanes: &mut CollectorState,
    state: &mut DaemonStateSnapshot,
    hw: &mut HardwareSnapshot,
    cfg: &Config,
    ctx: &'a mut CollectCtx<'a>,
    timings: Option<&mut Timings>,
) -> ReadingsSnapshot {
    let proc_root = ctx.proc_root;
    let sys_root = ctx.sys_root;
    let clock = ctx.clock;
    let skip_slow = ctx.skip_slow;
    let mut timings = timings;

    let caps = resolve_capabilities(cfg);
    let mut readings = ReadingsSnapshot {
        collected_at: clock,
        ..ReadingsSnapshot::default()
    };

    // ── CPU (always read: feeds sparks/braille/graphs + baseline) ───────────
    let cpu_usage = timed(&mut timings, "cpu_usage", || {
        cpu::read_cpu_usage(proc_root, &mut lanes.cpu, cfg, clock)
    });
    readings.cpu_usage = Some(cpu_usage);
    readings.cpu_history = lanes.cpu.cpu_history.clone();
    if caps.contains(&Capability::CpuTemperature) {
        let path = hw.cpu_temp_path.clone();
        readings.cpu_temp = timed(&mut timings, "cpu_temp", || {
            hwmon::read_path_millidegrees_celsius(path.as_deref())
        });
    }
    if caps.contains(&Capability::CpuFrequency) {
        let path = hw.cpu_freq_path.clone();
        readings.cpu_freq_mhz = timed(&mut timings, "cpu_freq", || {
            cpu::read_cpu_frequency_mhz(proc_root, path.as_deref())
        });
    }
    if caps.contains(&Capability::CpuTurbo) {
        readings.cpu_turbo = timed(&mut timings, "cpu_turbo", || cpu::read_cpu_turbo(sys_root));
    }
    if caps.contains(&Capability::Uptime) {
        readings.uptime_seconds = timed(&mut timings, "uptime", || {
            cpu::read_uptime_seconds(proc_root)
        });
    }
    if caps.contains(&Capability::LoadAverage) {
        readings.load_average = timed(&mut timings, "load_avg", || {
            cpu::read_load_average(proc_root)
        })
        .map(|(one, five, fifteen)| LoadAverage { one, five, fifteen });
    }
    if caps.contains(&Capability::TopProcess) && !skip_slow {
        let full = timed(&mut timings, "top_process", || {
            process::read_top_process_cached(proc_root, &mut lanes.process, clock)
        });
        readings.top_process_full = full.clone();
        readings.top_process = full.map(|rows| {
            rows.into_iter()
                .take(process::TOP_PROCESS_COUNT)
                .map(|row| TopProcessSummary {
                    command: row.command,
                    cpu_percent: row.cpu_percent,
                })
                .collect()
        });
    }
    if cfg.pages.order.iter().any(|page| page == "cpu_cores") && !skip_slow {
        let (usage, history) = timed(&mut timings, "cpu_cores", || {
            let usage = cpu::read_cpu_cores(proc_root, &mut lanes.cpu, cfg, clock);
            (usage, lanes.cpu.cpu_core_history.clone())
        });
        readings.cpu_core_usage = usage;
        readings.cpu_core_history = if history.is_empty() {
            None
        } else {
            Some(history)
        };
    }

    // ── Memory (always read) ───────────────────────────────────────────────
    let mem = timed(&mut timings, "mem_usage", || {
        memory::read_memory_usage(proc_root, &mut lanes.memory, cfg, clock)
    });
    if let Some(mem) = mem {
        readings.mem_usage = Some(mem.percent);
        readings.mem_used_gib = Some(mem.used_gib);
        readings.mem_total_gib = Some(mem.total_gib);
    }
    readings.mem_history = lanes.memory.mem_history.clone();
    if caps.contains(&Capability::SwapUsage) {
        readings.swap_usage = timed(&mut timings, "swap_usage", || {
            memory::read_swap_usage(proc_root)
        });
    }

    // ── Network rates + identity ───────────────────────────────────────────
    if caps.contains(&Capability::NetworkSpeed)
        && let Some(device) = hw.net_device.clone()
    {
        let dev = device.clone();
        let (up, down) = timed(&mut timings, "net_speed", || {
            network::read_net_speed(sys_root, &mut lanes.network, &dev, clock)
        });
        readings.net_up_bps = up;
        readings.net_down_bps = down;
    }
    // History samples from the rate just read (or a prior sample); re-exposes
    // the buffer each poll even when the rate was absent this pass.
    network::sample_net_history(
        &mut lanes.network,
        cfg,
        clock,
        readings.net_up_bps,
        readings.net_down_bps,
    );
    readings.net_up_history = lanes.network.net_up_history().to_vec();
    readings.net_down_history = lanes.network.net_down_history().to_vec();

    if caps.contains(&Capability::NetworkInfo) {
        let info = timed(&mut timings, "net_info", || {
            network::read_net_info_cached(sys_root, &mut lanes.network, clock, &mut |p, a| {
                ctx.commands.run(p, a, NETWORK_COMMAND_TIMEOUT)
            })
        });
        readings.net_device = info.device.clone();
        readings.ip_address = info.ip_address;
        readings.wifi_ssid = info.ssid;
        readings.wifi_signal_percent = info.signal_pct;
        // hw.net_device follows the live route: adopt a new interface as soon
        // as the cached net_info sees it. The Rust rate reader resets its diff
        // state internally on the device change, so no explicit net_rate reset
        // is needed here (unlike Python, which resets state.net_rate inline).
        if let Some(device) = info.device.as_deref()
            && hw.net_device.as_deref() != Some(device)
        {
            hw.net_device = Some(device.to_owned());
        }
    }

    // ── Disk I/O + usage + SMART + hd_temp + fan ───────────────────────────
    if caps.contains(&Capability::DiskIo)
        && let Some(device) = hw.disk_io_device.clone()
    {
        let dev = device.clone();
        let (read_bps, write_bps) = timed(&mut timings, "disk_io", || {
            disk::read_disk_io(proc_root, &mut lanes.disk, &dev, clock)
        });
        readings.disk_read_bps = read_bps;
        readings.disk_write_bps = write_bps;
    }
    if caps.contains(&Capability::DiskUsage) {
        let mounts = disk::resolve_mounts(proc_root, cfg);
        for mount in mounts {
            let label = mount.clone();
            let usage = timed(&mut timings, &format!("disk_usage[{label}]"), || {
                disk::read_disk_usage(Path::new(&mount))
            });
            readings.disk_usage.insert(
                mount,
                usage.map(|usage| DiskUsageReading {
                    percent: usage.percent,
                    used_gib: usage.used_gb,
                    total_gib: usage.total_gb,
                }),
            );
        }
    }
    if caps.contains(&Capability::DiskSmart) && cfg.disks.smart && !skip_slow {
        for (label, drive) in hw.disk_smart_drives.clone() {
            let interval = if drive.rotational {
                Duration::from_secs_f64(cfg.disks.smart_interval_hdd)
            } else {
                Duration::from_secs_f64(cfg.disks.smart_interval)
            };
            let key = format!("disk_smart[{label}]");
            let drive_path = drive.object_path.clone();
            let kind = drive.interface;
            let healthy = timed(&mut timings, &key, || {
                power::read_disk_smart_cached(
                    state,
                    ctx.dbus,
                    &label,
                    &drive_path,
                    kind,
                    clock.monotonic,
                    interval,
                )
            });
            readings.disk_smart.insert(label, healthy);
        }
    }
    if caps.contains(&Capability::DiskTemperature) {
        for (label, path) in hw.hd_temp_paths.clone() {
            let key = format!("hd_temp[{label}]");
            let p = path.clone();
            let temp = timed(&mut timings, &key, || {
                disk::read_hd_temp_cached(&mut lanes.disk, clock, &label, &p)
            });
            readings.hd_temps.insert(label, temp);
        }
    }
    if caps.contains(&Capability::FanSpeed) {
        for (label, path) in hw.fan_paths.clone() {
            let key = format!("fan_speed[{label}]");
            let p = path.clone();
            let speed = timed(&mut timings, &key, || {
                disk::read_fan_speed_cached(&mut lanes.disk, clock, &label, &p)
            });
            readings.fan_speeds.insert(label, speed);
        }
    }

    // ── Batteries (sysfs/UPower + Bolt HID) ────────────────────────────────
    if caps.contains(&Capability::BatterySystem) {
        let ids = hw.battery_sys_ids.clone();
        readings.battery_sys = timed(&mut timings, "battery_sys", || {
            power::read_battery_sys(state, ctx.dbus, &ids, sys_root, clock)
        });
    }
    if caps.contains(&Capability::BatteryMouse) {
        if let Some(id) = hw.battery_mouse_id.clone() {
            let name = cfg.battery.mouse_name.clone();
            readings.battery_mouse = timed(&mut timings, "battery_mouse", || {
                power::read_battery_periph(
                    &mut state.battery_mouse_cache,
                    ctx.dbus,
                    &id,
                    name.as_deref(),
                    clock,
                )
            });
        } else if cfg.battery.mouse_bolt.is_some() && !skip_slow {
            let dev_idx = cfg.battery.mouse_bolt;
            let name = cfg.battery.mouse_name.clone();
            if let Some(bolt) = ctx.bolt.as_deref_mut() {
                readings.battery_mouse = timed(&mut timings, "battery_mouse", || {
                    power::read_battery_bolt(
                        &mut state.battery_mouse_cache,
                        bolt,
                        dev_idx.unwrap_or(0),
                        name.as_deref(),
                        clock,
                    )
                });
            }
        }
    }
    if caps.contains(&Capability::BatteryKeyboard) {
        if let Some(id) = hw.battery_kbd_id.clone() {
            let name = cfg.battery.kbd_name.clone();
            readings.battery_kbd = timed(&mut timings, "battery_kbd", || {
                power::read_battery_periph(
                    &mut state.battery_kbd_cache,
                    ctx.dbus,
                    &id,
                    name.as_deref(),
                    clock,
                )
            });
        } else if cfg.battery.kbd_bolt.is_some() && !skip_slow {
            let dev_idx = cfg.battery.kbd_bolt;
            let name = cfg.battery.kbd_name.clone();
            if let Some(bolt) = ctx.bolt.as_deref_mut() {
                readings.battery_kbd = timed(&mut timings, "battery_kbd", || {
                    power::read_battery_bolt(
                        &mut state.battery_kbd_cache,
                        bolt,
                        dev_idx.unwrap_or(0),
                        name.as_deref(),
                        clock,
                    )
                });
            }
        }
    }

    // ── GPU (NVIDIA then Intel) ────────────────────────────────────────────
    if caps.contains(&Capability::GpuNvidia) && hw.has_nvidia && !skip_slow {
        let metrics = timed(&mut timings, "gpu_nvidia", || {
            gpu_nvidia::read_nvidia(
                &mut state.gpu_cache,
                ctx.nvml.as_deref_mut(),
                ctx.commands,
                clock,
            )
        });
        readings.gpu_temp = metrics.temp_celsius;
        readings.gpu_usage = metrics.usage_percent;
        readings.gpu_mem = metrics.memory_percent;
        readings.gpu_dec = metrics.decoder_percent;
        readings.gpu_fan = metrics.fan_percent;
    }
    if caps.contains(&Capability::GpuIntelFrequency) && hw.intel_gpu_freq_path.is_some() {
        let path = hw.intel_gpu_freq_path.clone();
        readings.gpu_intel_freq = timed(&mut timings, "gpu_intel_freq", || {
            hwmon::read_path_int(path.as_deref())
        });
    }
    let wants_intel_usage = caps.contains(&Capability::GpuIntelUsage);
    let wants_intel_dec = caps.contains(&Capability::GpuIntelDecoder);
    if let Some(pci) = hw.intel_gpu_pci.clone()
        && (wants_intel_usage || wants_intel_dec)
        && !skip_slow
    {
        let metrics = timed(&mut timings, "gpu_intel_usage", || {
            gpu_intel::read_intel_gpu_metrics_cached(proc_root, &mut lanes.intel_gpu, &pci, clock)
        });
        if wants_intel_usage {
            readings.gpu_intel_usage = metrics.get("render").copied();
        }
        if wants_intel_dec {
            readings.gpu_intel_dec_usage = metrics.get("video").copied();
        }
    }
    gpu_nvidia::sample_gpu_history(state, cfg, hw, &mut readings, clock);

    // ── Brightness + external status files ─────────────────────────────────
    if caps.contains(&Capability::ScreenBrightness) {
        readings.screen_brightness = timed(&mut timings, "screen_brightness", || {
            read_brightness(sys_root)
        });
    }
    if caps.contains(&Capability::SystemUpdates) && !cfg.system_updates.file.is_empty() {
        let path = PathBuf::from(&cfg.system_updates.file);
        readings.system_updates = timed(&mut timings, "system_updates", || read_count_file(&path));
    }
    if caps.contains(&Capability::ServerCheck) && !cfg.server_check.file.is_empty() {
        let path = PathBuf::from(&cfg.server_check.file);
        readings.server_ok = timed(&mut timings, "server_check", || read_server_file(&path));
    }

    readings
}

/// Computes the capability set requested this poll from the resolved config.
///
/// Mirrors the body of Python's `needed_capabilities(cfg)`: union of every
/// configured item's metric capabilities, capabilities pulled by enabled
/// notification flags, and the `graphs` page's fixed capability set.
fn resolve_capabilities(cfg: &Config) -> BTreeSet<Capability> {
    let items = cfg
        .panel
        .sections
        .iter()
        .chain(cfg.tooltip.sections.iter())
        .flat_map(|section| section.items.iter())
        .filter_map(|token| ItemToken::from_str(token).ok());
    needed_capabilities(
        items,
        notify_flags(cfg).into_iter(),
        cfg.pages.order.iter().map(String::as_str),
    )
}

/// Notification flag names whose enabled config field keeps a sensor alive.
///
/// The names are the [`crate::config::NotificationConfig`] field keys and match
/// [`crate::domain::registry::NOTIFY_CAPABILITY_MAP`] verbatim.
fn notify_flags(cfg: &Config) -> Vec<&'static str> {
    let n = &cfg.notifications;
    let mut flags = Vec::new();
    if n.cpu_temp {
        flags.push("cpu_temp");
    }
    if n.gpu_nvidia_temp {
        flags.push("gpu_nvidia_temp");
    }
    if n.disk_usage {
        flags.push("disk_usage");
    }
    if n.disk_smart {
        flags.push("disk_smart");
    }
    if n.hd_temp {
        flags.push("hd_temp");
    }
    if n.battery_sys {
        flags.push("battery_sys");
    }
    if n.battery_mouse {
        flags.push("battery_mouse");
    }
    if n.battery_kbd {
        flags.push("battery_kbd");
    }
    if n.load_avg {
        flags.push("load_avg");
    }
    if n.server_check {
        flags.push("server_check");
    }
    flags
}

// ── Collector-owned readers (not owned by a sensor lane) ─────────────────────

/// Default-route net device via `ip route get` / `ip route show default`.
fn detect_net_device(commands: &mut dyn CommandRunner) -> Option<String> {
    network::detect_net_device(&mut |program, args| {
        commands.run(program, args, NETWORK_COMMAND_TIMEOUT)
    })
}

/// A backlight device exposing both `brightness` and `max_brightness`.
///
/// Mirrors `_detect_has_backlight` in `src/sensors.py`. Desktops/VMs have none
/// → `false`, gating `screen_brightness` off.
fn detect_has_backlight(sys_root: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(sys_root.join("class/backlight")) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.join("brightness").exists() && path.join("max_brightness").exists() {
            return true;
        }
    }
    false
}

/// Reads the screen brightness percentage from the first usable backlight.
///
/// Mirrors `_read_brightness` in `src/sensors.py`: `cur * 100 // max` over the
/// first backlight with both files and a non-zero max. A missing
/// `/sys/class/backlight` directory degrades to `None` (the contract's
/// "absent hardware maps to None/empty"; Python would raise on the missing
/// directory).
fn read_brightness(sys_root: &Path) -> Option<i32> {
    let entries = std::fs::read_dir(sys_root.join("class/backlight")).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let cur_f = path.join("brightness");
        let max_f = path.join("max_brightness");
        if !cur_f.exists() || !max_f.exists() {
            continue;
        }
        let cur = read_trimmed_i64(&cur_f);
        let max = read_trimmed_i64(&max_f);
        if let (Some(cur), Some(max)) = (cur, max)
            && max > 0
        {
            return Some(((cur * 100) / max) as i32);
        }
    }
    None
}

/// Reads an integer (e.g. a pending-updates count) from an externally-written
/// file. Mirrors `_read_count_file` in `src/sensors.py`: `None` when the file is
/// missing/unreadable/empty/not a valid integer.
fn read_count_file(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()
}

/// Reads a server-reachability flag from an externally-written file.
///
/// Mirrors `_read_server_file` in `src/sensors.py`: `"1"` → `true`,
/// `"0"` → `false`, anything else (or missing/unreadable) → `None`.
fn read_server_file(path: &Path) -> Option<bool> {
    let value = std::fs::read_to_string(path).ok()?;
    match value.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

/// Parses a sysfs file as a signed integer, tolerating surrounding whitespace.
fn read_trimmed_i64(path: &Path) -> Option<i64> {
    std::fs::read_to_string(path)
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()
}

// ── find_peripherals (collector-owned discovery) ─────────────────────────────

/// UPower type enum values used by the peripheral classifier.
const UPOWER_TYPE_MOUSE: i64 = 5;
/// UPower keyboard device type.
const UPOWER_TYPE_KEYBOARD: i64 = 6;

/// Discovers Logitech hidpp battery UPower paths for the mouse/keyboard.
///
/// Mirrors `_find_peripherals` in `src/sensors.py`: manual `[battery]` Unifying
/// overrides win; otherwise each `/battery_hidpp` path is classified by UPower
/// `Type` (mouse=5, keyboard=6) with model-name heuristics as a fallback for
/// devices that report `Type=0`. Returns `(mouse_id, kbd_id)`.
fn find_peripherals(cfg: &Config, dbus: &mut dyn DbusFacade) -> (Option<String>, Option<String>) {
    let mut mouse = cfg.battery.mouse_unifying.clone();
    let mut kbd = cfg.battery.kbd_unifying.clone();
    if mouse.is_some() && kbd.is_some() {
        return (mouse, kbd);
    }

    let hidpp: Vec<String> = power::upower_enumerate(dbus)
        .into_iter()
        .filter(|path| path.contains("/battery_hidpp"))
        .collect();
    for path in hidpp {
        if mouse.is_some() && kbd.is_some() {
            break;
        }
        let Some(props) = power::upower_device_props(dbus, &path) else {
            continue;
        };
        let model = props.model.as_deref().unwrap_or("").to_ascii_lowercase();
        let kind = props.kind;

        let is_kbd = kind == Some(UPOWER_TYPE_KEYBOARD)
            || ["keyboard", "keys", "ergo"]
                .iter()
                .any(|w| model.contains(w))
            || ["k4", "k8", "mx keys"].iter().any(|p| model.starts_with(p));
        let is_mouse = kind == Some(UPOWER_TYPE_MOUSE)
            || model.contains("mouse")
            || model.contains("master")
            || model.contains("mx m")
            || model.contains("trackball");

        if is_kbd && kbd.is_none() {
            kbd = Some(path);
        } else if is_mouse && mouse.is_none() {
            mouse = Some(path);
        }
    }
    (mouse, kbd)
}

#[cfg(all(test, feature = "test-support"))]
mod tests;
