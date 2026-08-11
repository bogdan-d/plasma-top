//! Issue-02 collection characterization harness retained only for owner regression tests.
//!
//! Production cadence lives exclusively in [`crate::scheduler`]; this test-only pass preserves pre-cutover fixtures while they continue to verify one-attempt owner semantics.

use std::collections::BTreeSet;
use std::str::FromStr;
use std::time::Duration;

use crate::config::Config;
use crate::domain::item::ItemToken;
use crate::domain::metric::Capability;
use crate::domain::readings::{
    DiskUsageReading, DisplaySnapshot, HardwareInventory, MetricSample, SmartDisk,
};
use crate::domain::registry::needed_capabilities;

use super::attempts::{
    AttemptStatus, CpuCoreResult, CpuResult, DiskIoResult, DiskTemperatureResult, DiskUsageResult,
    ExternalResult, FanSpeedResult, IntelFrequencyResult, IntelUsageResult, MemoryResult,
    NetworkInfoResult, NetworkSpeedResult, NvidiaResult, PeripheralPowerResult, ProcessResult,
    SmartResult, SystemBatteryResult, Timings, attempt_bolt_peripheral, attempt_cpu,
    attempt_cpu_cores, attempt_disk_io, attempt_disk_temperature, attempt_disk_usage,
    attempt_external, attempt_fan_speed, attempt_intel_frequency, attempt_intel_usage,
    attempt_memory, attempt_network_info, attempt_network_speed, attempt_nvidia_fallback,
    attempt_nvml_nvidia, attempt_process, attempt_smart, attempt_system_battery,
    attempt_upower_peripheral, cached_attempt, cached_attempt_or_empty, cached_peripheral,
    cached_system_battery, nvidia_result, process_result, timed,
};
use super::coordinator::{CollectCtx, OwnerRefs};
use super::{cpu, disk, gpu_history, gpu_intel, gpu_nvidia, memory, network, power, process};

fn current_sample<T>(result: &super::attempts::AttemptResult<T>) -> Option<&MetricSample<T>> {
    (result.status == AttemptStatus::Captured
        || (result.status == AttemptStatus::Cached && !result.latest_attempt_failed))
        .then_some(result.sample.as_ref())
        .flatten()
}

fn flatten_cpu_notifications(result: &CpuResult, readings: &mut DisplaySnapshot) {
    readings.cpu_temp = current_sample(&result.temperature).map(|sample| sample.value);
    readings.load_average = current_sample(&result.load_average).map(|sample| sample.value);
}

fn flatten_disk_usage_notification(result: &DiskUsageResult, readings: &mut DisplaySnapshot) {
    let value = current_sample(&result.reading).map(|sample| DiskUsageReading {
        percent: sample.value.percent,
        used_gib: sample.value.used_gb,
        total_gib: sample.value.total_gb,
    });
    readings.disk_usage.insert(result.mount.clone(), value);
}

fn flatten_disk_temperature_notification(
    result: &DiskTemperatureResult,
    readings: &mut DisplaySnapshot,
) {
    readings.hd_temps.insert(
        result.label.clone(),
        current_sample(&result.reading).map(|sample| sample.value),
    );
}

fn flatten_smart_notification(result: &SmartResult, readings: &mut DisplaySnapshot) {
    readings.disk_smart.insert(
        result.label.clone(),
        current_sample(&result.reading).map(|sample| sample.value),
    );
}

fn flatten_system_battery_notification(
    result: &SystemBatteryResult,
    readings: &mut DisplaySnapshot,
) {
    if let Some(sample) = current_sample(&result.reading) {
        readings.battery_sys.push(sample.value.clone());
    }
}

fn flatten_peripheral_notification(
    result: &PeripheralPowerResult,
) -> Option<crate::domain::readings::BatteryPeripheralReading> {
    current_sample(&result.reading).map(|sample| sample.value.clone())
}

fn flatten_nvidia_notification(result: &NvidiaResult, readings: &mut DisplaySnapshot) {
    if let Some(sample) = current_sample(&result.reading) {
        readings.gpu_temp = sample.value.temp_celsius;
    }
}

fn flatten_external_notification(result: &ExternalResult, readings: &mut DisplaySnapshot) {
    readings.server_ok = current_sample(&result.server).map(|sample| sample.value);
}

fn flatten_cpu(result: CpuResult, readings: &mut DisplaySnapshot) {
    readings.cpu_usage = result.usage.sample.map(|sample| sample.value);
    readings.cpu_temp = result.temperature.sample.map(|sample| sample.value);
    readings.cpu_freq_mhz = result.frequency_mhz.sample.map(|sample| sample.value);
    readings.cpu_turbo = result.turbo.sample.map(|sample| sample.value);
    if let Some(history) = result.history {
        readings.cpu_history = history.value;
    }
    readings.uptime_seconds = result.uptime_seconds.sample.map(|sample| sample.value);
    readings.load_average = result.load_average.sample.map(|sample| sample.value);
}

fn flatten_cpu_cores(result: CpuCoreResult, readings: &mut DisplaySnapshot) {
    readings.cpu_core_usage = result.usage.sample.map(|sample| sample.value);
    readings.cpu_core_history = result.history.map(|sample| sample.value);
}

fn flatten_process(result: ProcessResult, readings: &mut DisplaySnapshot) {
    readings.top_process = result
        .reading
        .sample
        .as_ref()
        .map(|sample| sample.value.summary.clone());
    readings.top_process_full = result.reading.sample.map(|sample| sample.value.full);
}

fn flatten_nvidia(result: NvidiaResult, readings: &mut DisplaySnapshot) {
    let Some(sample) = result.reading.sample else {
        return;
    };
    let value = sample.value;
    readings.gpu_temp = value.temp_celsius;
    readings.gpu_usage = value.usage_percent;
    readings.gpu_mem = value.memory_percent;
    readings.gpu_dec = value.decoder_percent;
    readings.gpu_fan = value.fan_percent;
}

fn flatten_intel_frequency(result: IntelFrequencyResult, readings: &mut DisplaySnapshot) {
    readings.gpu_intel_freq = result.reading.sample.map(|sample| sample.value);
}

fn flatten_intel_usage(
    result: IntelUsageResult,
    wants_usage: bool,
    wants_decoder: bool,
    readings: &mut DisplaySnapshot,
) {
    let Some(sample) = result.reading.sample else {
        return;
    };
    if wants_usage {
        readings.gpu_intel_usage = sample.value.get("render").copied();
    }
    if wants_decoder {
        readings.gpu_intel_dec_usage = sample.value.get("video").copied();
    }
}

fn intel_decoder_outcome(result: &IntelUsageResult) -> gpu_history::DecoderOutcome {
    match result.reading.status {
        AttemptStatus::Failed => gpu_history::DecoderOutcome::TransientFailure,
        AttemptStatus::Absent => gpu_history::DecoderOutcome::ConfirmedAbsent,
        AttemptStatus::Baseline => gpu_history::DecoderOutcome::Unmeasured,
        AttemptStatus::Captured | AttemptStatus::Cached => result
            .reading
            .sample
            .as_ref()
            .and_then(|sample| sample.value.get("video"))
            .copied()
            .map_or_else(
                || {
                    if result.reading.status == AttemptStatus::Captured {
                        gpu_history::DecoderOutcome::ConfirmedAbsent
                    } else {
                        gpu_history::DecoderOutcome::Unmeasured
                    }
                },
                gpu_history::DecoderOutcome::Value,
            ),
    }
}

fn flatten_external(result: ExternalResult, readings: &mut DisplaySnapshot) {
    readings.screen_brightness = result.brightness.sample.map(|sample| sample.value);
    readings.system_updates = result.updates.sample.map(|sample| sample.value);
    readings.server_ok = result.server.sample.map(|sample| sample.value);
}

fn flatten_memory(result: MemoryResult, readings: &mut DisplaySnapshot) {
    if let Some(sample) = result.usage.sample {
        readings.mem_usage = Some(sample.value.percent);
        readings.mem_used_gib = Some(sample.value.used_gib);
        readings.mem_total_gib = Some(sample.value.total_gib);
    }
    readings.swap_usage = result.swap_usage.sample.map(|sample| sample.value);
    if let Some(history) = result.history {
        readings.mem_history = history.value;
    }
}

fn flatten_network_speed(result: NetworkSpeedResult, readings: &mut DisplaySnapshot) {
    let (up, down) = result.reading.sample.map_or((None, None), |sample| {
        (Some(sample.value.0), Some(sample.value.1))
    });
    readings.net_up_bps = up;
    readings.net_down_bps = down;
    if let Some(history) = result.up_history {
        readings.net_up_history = history.value;
    }
    if let Some(history) = result.down_history {
        readings.net_down_history = history.value;
    }
}

fn flatten_network_info(result: NetworkInfoResult, readings: &mut DisplaySnapshot) {
    let Some(sample) = result.reading.sample else {
        return;
    };
    readings.net_device = sample.value.device.clone();
    readings.ip_address = sample.value.ip_address;
    readings.wifi_ssid = None;
    readings.wifi_signal_percent = None;
    if let Some(wifi) = result.wifi.sample {
        if Some(wifi.value.device.as_str()) == sample.value.device.as_deref() {
            readings.wifi_ssid = wifi.value.ssid;
            readings.wifi_signal_percent = wifi.value.signal_pct;
        }
    }
}

fn flatten_disk_io(result: DiskIoResult, readings: &mut DisplaySnapshot) {
    let (read, write) = result.reading.sample.map_or((None, None), |sample| {
        (Some(sample.value.0), Some(sample.value.1))
    });
    readings.disk_read_bps = read;
    readings.disk_write_bps = write;
}

fn flatten_disk_usage(result: DiskUsageResult, readings: &mut DisplaySnapshot) {
    let value = result.reading.sample.map(|sample| DiskUsageReading {
        percent: sample.value.percent,
        used_gib: sample.value.used_gb,
        total_gib: sample.value.total_gb,
    });
    readings.disk_usage.insert(result.mount, value);
}

fn flatten_disk_temperature(result: DiskTemperatureResult, readings: &mut DisplaySnapshot) {
    readings.hd_temps.insert(
        result.label,
        result.reading.sample.map(|sample| sample.value),
    );
}

fn flatten_fan_speed(result: FanSpeedResult, readings: &mut DisplaySnapshot) {
    readings.fan_speeds.insert(
        result.label,
        result.reading.sample.map(|sample| sample.value),
    );
}

fn flatten_smart(result: SmartResult, readings: &mut DisplaySnapshot) {
    readings.disk_smart.insert(
        result.label,
        result.reading.sample.map(|sample| sample.value),
    );
}

fn flatten_system_battery(result: SystemBatteryResult, readings: &mut DisplaySnapshot) {
    if let Some(sample) = result.reading.sample {
        readings.battery_sys.push(sample.value);
    }
}

fn flatten_mouse_battery(result: PeripheralPowerResult, readings: &mut DisplaySnapshot) {
    readings.battery_mouse = result.reading.sample.map(|sample| sample.value);
}

fn flatten_keyboard_battery(result: PeripheralPowerResult, readings: &mut DisplaySnapshot) {
    readings.battery_kbd = result.reading.sample.map(|sample| sample.value);
}

fn flatten_gpu_history(result: gpu_history::GpuHistoryResult, readings: &mut DisplaySnapshot) {
    if let Some(history) = result.usage {
        readings.gpu_usage_history = history.value;
    }
    if let Some(history) = result.decoder {
        readings.gpu_dec_history = history.value;
    }
}

/// Display values plus current-attempt values eligible for synchronous notifications.
pub(crate) struct CollectionOutput {
    pub(crate) display: DisplaySnapshot,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) notifications: DisplaySnapshot,
}

pub(super) fn sample_due(sampled_at: Option<Duration>, now: Duration, cadence: Duration) -> bool {
    sampled_at.is_none_or(|previous| now.saturating_sub(previous) >= cadence)
}

// ── Per-poll collection ──────────────────────────────────────────────────────

/// Produces a fresh [`DisplaySnapshot`] for one poll.
///
/// Mirrors `collect` in `src/sensors.py`. Work is demand-driven: only the
/// capabilities returned by [`needed_capabilities`] (derived from the resolved
/// config items, enabled notifications, and the `graphs` page) trigger reads,
/// and each shared read executes once. Section order matches Python exactly
/// where stateful or observable (history sampling, net-device adoption, call
/// trace). `skip_slow = true` skips slow sources without a retained sample so the first paint at startup is fast, matching Python's first-paint behavior.
///
/// Mutates only the borrowed domain owners and `hw` (live net-device adoption). When `timings` is `Some`, accumulates per-section wall-clock elapsed for the profiling subcommand; `None` is zero-overhead and deterministic.
#[allow(clippy::too_many_lines)]
pub fn collect(
    owners: OwnerRefs<'_>,
    hw: &mut HardwareInventory,
    cfg: &Config,
    ctx: &mut CollectCtx<'_, '_>,
    timings: Option<&mut Timings>,
) -> DisplaySnapshot {
    collect_with_notifications(owners, hw, cfg, ctx, timings).display
}

pub(crate) fn collect_with_notifications(
    owners: OwnerRefs<'_>,
    hw: &mut HardwareInventory,
    cfg: &Config,
    ctx: &mut CollectCtx<'_, '_>,
    timings: Option<&mut Timings>,
) -> CollectionOutput {
    let proc_root = ctx.proc_root;
    let sys_root = ctx.sys_root;
    let mut clock;
    let skip_slow = ctx.skip_slow;
    let mut timings = timings;

    let caps = resolve_capabilities(cfg);
    let has_core_page = cfg.pages.order.iter().any(|page| page == "cpu_cores");
    let has_graphs_page = cfg.pages.order.iter().any(|page| page == "graphs");
    owners.cpu.reconcile_core_page(has_core_page);
    owners
        .process
        .reconcile_panel(caps.contains(&Capability::TopProcess));
    owners.network.reconcile_sources(
        hw.net_device.as_deref(),
        caps.contains(&Capability::NetworkSpeed),
        caps.contains(&Capability::NetworkInfo),
        has_graphs_page,
    );
    owners.disk.reconcile_sources(hw, cfg, &caps);
    owners.power.reconcile_sources(hw, cfg, &caps);
    owners
        .nvidia
        .reconcile_source(caps.contains(&Capability::GpuNvidia) && hw.has_nvidia);
    owners.intel_gpu.reconcile_sources(
        hw.intel_gpu_pci.as_deref(),
        hw.intel_gpu_freq_path.as_ref(),
        caps.contains(&Capability::GpuIntelUsage) || caps.contains(&Capability::GpuIntelDecoder),
        caps.contains(&Capability::GpuIntelFrequency),
    );
    owners.external.reconcile_sources(
        cfg,
        hw,
        caps.contains(&Capability::SystemUpdates),
        caps.contains(&Capability::ServerCheck),
        caps.contains(&Capability::ScreenBrightness),
    );
    let mut readings = DisplaySnapshot::default();
    let mut notification_readings = DisplaySnapshot::default();

    // ── CPU (always read: feeds sparks/braille/graphs + baseline) ───────────
    let mut cpu_result = attempt_cpu(
        owners.cpu,
        proc_root,
        sys_root,
        hw,
        &caps,
        &mut *ctx.clock,
        &mut timings,
    );
    if cpu_result.usage.status != AttemptStatus::Baseline {
        if let Some(usage) = cpu_result.usage.sample.as_ref() {
            if let Some(history_at) = cpu_result.usage.attempted_at {
                if sample_due(
                    owners.cpu.cpu_history_sample_at,
                    history_at,
                    cfg.display.history_interval.duration(),
                ) {
                    cpu::append_cpu_history(owners.cpu, cfg, history_at, usage.value);
                    cpu_result.history = Some(MetricSample::new(
                        owners.cpu.cpu_history.clone(),
                        history_at,
                    ));
                }
            }
        }
    }
    flatten_cpu_notifications(&cpu_result, &mut notification_readings);
    flatten_cpu(cpu_result, &mut readings);
    if caps.contains(&Capability::TopProcess) && !skip_slow {
        clock = (ctx.clock)();
        let result = timed(&mut timings, "top_process", || {
            let fresh = owners.process.panel.latest.is_some()
                && !sample_due(
                    owners
                        .process
                        .panel
                        .latest
                        .as_ref()
                        .map(|sample| sample.captured_at),
                    clock.monotonic,
                    process::TOP_PROCESS_TTL,
                );
            if fresh {
                return process_result(owners.process, AttemptStatus::Cached);
            }
            attempt_process(owners.process, proc_root, clock)
        });
        flatten_process(result, &mut readings);
    }
    if cfg.pages.order.iter().any(|page| page == "cpu_cores") && !skip_slow {
        clock = (ctx.clock)();
        let mut result = attempt_cpu_cores(owners.cpu, proc_root, clock, &mut timings);
        if result.usage.status != AttemptStatus::Baseline {
            if let Some(usage) = result.usage.sample.as_ref() {
                if let Some(history_at) = result.usage.attempted_at {
                    if sample_due(
                        owners.cpu.cpu_core_history_sample_at,
                        history_at,
                        cfg.display.history_interval.duration(),
                    ) {
                        cpu::append_cpu_core_history(owners.cpu, cfg, history_at, &usage.value);
                        result.history = Some(MetricSample::new(
                            owners.cpu.cpu_core_history.clone(),
                            history_at,
                        ));
                    }
                }
            }
        }
        flatten_cpu_cores(result, &mut readings);
    }

    // ── Memory (always read) ───────────────────────────────────────────────
    let mut memory_result = attempt_memory(
        owners.memory,
        proc_root,
        caps.contains(&Capability::SwapUsage),
        &mut *ctx.clock,
        &mut timings,
    );
    if let Some(usage) = memory_result.usage.sample.as_ref() {
        if let Some(history_at) = memory_result.usage.attempted_at {
            if sample_due(
                owners.memory.mem_history_sample_at,
                history_at,
                cfg.display.history_interval.duration(),
            ) {
                memory::append_memory_history(owners.memory, cfg, history_at, usage.value.percent);
                memory_result.history = Some(MetricSample::new(
                    owners.memory.mem_history.clone(),
                    history_at,
                ));
            }
        }
    }
    flatten_memory(memory_result, &mut readings);
    // ── Network rates + identity ───────────────────────────────────────────
    let mut network_speed_result = if caps.contains(&Capability::NetworkSpeed) {
        if let Some(device) = hw.net_device.as_deref() {
            clock = (ctx.clock)();
            Some(attempt_network_speed(
                owners.network,
                sys_root,
                device,
                clock,
                &mut timings,
            ))
        } else {
            None
        }
    } else {
        None
    };

    let mut network_source_changed = false;
    if caps.contains(&Capability::NetworkInfo) {
        clock = (ctx.clock)();
        let result = if sample_due(
            owners.network.info.attempted_at,
            clock.monotonic,
            network::NET_INFO_TTL,
        ) {
            attempt_network_info(
                owners.network,
                sys_root,
                ctx.commands,
                &mut *ctx.clock,
                &mut timings,
            )
        } else {
            NetworkInfoResult {
                reading: cached_attempt(&owners.network.info),
                wifi: cached_attempt(&owners.network.wifi),
                route_status: AttemptStatus::Cached,
            }
        };
        let discovered_device = (result.route_status == AttemptStatus::Captured).then(|| {
            result
                .reading
                .sample
                .as_ref()
                .and_then(|sample| sample.value.device.clone())
        });
        flatten_network_info(result, &mut readings);
        if let Some(device) = discovered_device {
            if hw.net_device != device {
                hw.net_device = device;
                owners.network.reset_rate();
                owners.network.reset_history();
                network_source_changed = true;
            }
        }
    }

    // A refreshed route can invalidate the just-read old-device rate, so append history and flatten only after identity reconciliation.
    if let Some(result) = network_speed_result.as_mut() {
        if network_source_changed {
            result.reading = cached_attempt(&owners.network.rate);
            result.up_history = None;
            result.down_history = None;
        }
        if result.reading.status != AttemptStatus::Baseline {
            if let Some(sample) = result.reading.sample.as_ref() {
                if let Some(history_at) = result.reading.attempted_at {
                    if cfg.pages.order.iter().any(|page| page == "graphs")
                        && sample_due(
                            network::net_history_sample_at(owners.network),
                            history_at,
                            cfg.display.history_interval.duration(),
                        )
                    {
                        network::append_net_history(
                            owners.network,
                            cfg,
                            history_at,
                            Some(sample.value.0),
                            Some(sample.value.1),
                        );
                        result.up_history = Some(MetricSample::new(
                            owners.network.net_up_history().to_vec(),
                            history_at,
                        ));
                        result.down_history = Some(MetricSample::new(
                            owners.network.net_down_history().to_vec(),
                            history_at,
                        ));
                    }
                }
            }
        }
    }
    if let Some(result) = network_speed_result {
        flatten_network_speed(result, &mut readings);
    }
    // ── Disk I/O + usage + SMART + hd_temp + fan ───────────────────────────
    clock = (ctx.clock)();
    if caps.contains(&Capability::DiskIo) {
        if let Some(device) = hw.disk_io_device.as_deref() {
            let result = attempt_disk_io(owners.disk, proc_root, device, clock, &mut timings);
            flatten_disk_io(result, &mut readings);
        }
    }
    if caps.contains(&Capability::DiskUsage) {
        let mounts = disk::resolve_mounts(proc_root, cfg);
        owners.disk.reconcile_mounts(&mounts);
        for mount in mounts {
            let captured_at = (ctx.clock)();
            let result = attempt_disk_usage(owners.disk, &mount, captured_at, &mut timings);
            flatten_disk_usage_notification(&result, &mut notification_readings);
            flatten_disk_usage(result, &mut readings);
        }
    }
    if caps.contains(&Capability::DiskSmart) && cfg.disks.smart && !skip_slow {
        for (label, drive) in hw.disk_smart_drives.clone() {
            let interval = if drive.rotational {
                cfg.disks.smart_interval_hdd.duration()
            } else {
                cfg.disks.smart_interval.duration()
            };
            let key = format!("disk_smart[{label}]");
            let drive_path = drive.object_path.clone();
            let kind = drive.interface;
            let healthy = timed(&mut timings, &key, || {
                let decision_at = (ctx.clock)().monotonic;
                let refresh =
                    owners.disk.smart_cache.get(&label).is_none_or(|sample| {
                        sample_due(sample.attempted_at, decision_at, interval)
                    });
                if refresh {
                    let attempted_at = (ctx.clock)().monotonic;
                    let result = attempt_smart(
                        owners.disk,
                        ctx.dbus,
                        &label,
                        &SmartDisk {
                            object_path: drive_path.clone(),
                            interface: kind,
                            rotational: drive.rotational,
                        },
                        attempted_at,
                    );
                    return result;
                }
                SmartResult {
                    label: label.clone(),
                    reading: cached_attempt_or_empty(owners.disk.smart_cache.get(&label)),
                }
            });
            flatten_smart_notification(&healthy, &mut notification_readings);
            flatten_smart(healthy, &mut readings);
        }
    }
    if caps.contains(&Capability::DiskTemperature) {
        for (label, path) in hw.hd_temp_paths.clone() {
            let key = format!("hd_temp[{label}]");
            let p = path.clone();
            let temp = timed(&mut timings, &key, || {
                let decision_at = (ctx.clock)().monotonic;
                let refresh = owners.disk.hd_temp_cache.get(&label).is_none_or(|sample| {
                    sample_due(sample.attempted_at, decision_at, disk::HD_TEMP_CACHE_TTL)
                });
                if refresh {
                    let attempted_at = (ctx.clock)().monotonic;
                    return attempt_disk_temperature(owners.disk, &label, &p, attempted_at);
                }
                DiskTemperatureResult {
                    label: label.clone(),
                    reading: cached_attempt_or_empty(owners.disk.hd_temp_cache.get(&label)),
                }
            });
            flatten_disk_temperature_notification(&temp, &mut notification_readings);
            flatten_disk_temperature(temp, &mut readings);
        }
    }
    if caps.contains(&Capability::FanSpeed) {
        for (label, path) in hw.fan_paths.clone() {
            let key = format!("fan_speed[{label}]");
            let p = path.clone();
            let speed = timed(&mut timings, &key, || {
                let decision_at = (ctx.clock)().monotonic;
                let refresh = owners
                    .disk
                    .fan_speed_cache
                    .get(&label)
                    .is_none_or(|sample| {
                        sample_due(sample.attempted_at, decision_at, disk::FAN_SPEED_CACHE_TTL)
                    });
                if refresh {
                    let attempted_at = (ctx.clock)().monotonic;
                    return attempt_fan_speed(owners.disk, &label, &p, attempted_at);
                }
                FanSpeedResult {
                    label: label.clone(),
                    reading: cached_attempt_or_empty(owners.disk.fan_speed_cache.get(&label)),
                }
            });
            flatten_fan_speed(speed, &mut readings);
        }
    }
    // ── Batteries (sysfs/UPower + Bolt HID) ────────────────────────────────
    if caps.contains(&Capability::BatterySystem) {
        let ids = hw.battery_sys_ids.clone();
        for id in ids {
            let decision_at = (ctx.clock)().monotonic;
            let due = owners.power.battery_sys_cache.get(&id).is_none_or(|cache| {
                sample_due(cache.attempted_at, decision_at, power::BAT_CACHE_TTL)
            });
            let result = if due {
                let captured_at = (ctx.clock)();
                attempt_system_battery(
                    owners.power,
                    ctx.dbus,
                    &id,
                    sys_root,
                    captured_at,
                    &mut timings,
                )
            } else {
                cached_system_battery(&id, owners.power.battery_sys_cache.get(&id))
            };
            flatten_system_battery_notification(&result, &mut notification_readings);
            flatten_system_battery(result, &mut readings);
        }
    }
    if caps.contains(&Capability::BatteryMouse) {
        if let Some(id) = hw.battery_mouse_id.clone() {
            let decision_at = (ctx.clock)().monotonic;
            let name = cfg.battery.mouse_name.clone();
            let cache = &mut owners.power.battery_mouse_cache;
            let result = if sample_due(cache.attempted_at, decision_at, power::PERIPH_CACHE_TTL) {
                let captured_at = (ctx.clock)();
                attempt_upower_peripheral(
                    cache,
                    ctx.dbus,
                    &id,
                    name.as_deref(),
                    captured_at,
                    "battery_mouse",
                    &mut timings,
                )
            } else {
                cached_peripheral(cache, name.as_deref())
            };
            notification_readings.battery_mouse = flatten_peripheral_notification(&result);
            flatten_mouse_battery(result, &mut readings);
        } else if cfg.battery.mouse_bolt.is_some() && !skip_slow {
            let dev_idx = cfg.battery.mouse_bolt;
            let name = cfg.battery.mouse_name.clone();
            if let Some(bolt) = ctx.bolt.as_deref_mut() {
                let decision_at = (ctx.clock)().monotonic;
                let cache = &mut owners.power.battery_mouse_cache;
                let result =
                    if sample_due(cache.bolt_completed_at, decision_at, power::BOLT_CACHE_TTL) {
                        let captured_at = (ctx.clock)();
                        attempt_bolt_peripheral(
                            cache,
                            bolt,
                            dev_idx.unwrap_or(0),
                            name.as_deref(),
                            captured_at,
                            "battery_mouse",
                            &mut timings,
                        )
                    } else {
                        cached_peripheral(cache, name.as_deref())
                    };
                notification_readings.battery_mouse = flatten_peripheral_notification(&result);
                flatten_mouse_battery(result, &mut readings);
            }
        }
    }
    if caps.contains(&Capability::BatteryKeyboard) {
        if let Some(id) = hw.battery_kbd_id.clone() {
            let decision_at = (ctx.clock)().monotonic;
            let name = cfg.battery.kbd_name.clone();
            let cache = &mut owners.power.battery_kbd_cache;
            let result = if sample_due(cache.attempted_at, decision_at, power::PERIPH_CACHE_TTL) {
                let captured_at = (ctx.clock)();
                attempt_upower_peripheral(
                    cache,
                    ctx.dbus,
                    &id,
                    name.as_deref(),
                    captured_at,
                    "battery_kbd",
                    &mut timings,
                )
            } else {
                cached_peripheral(cache, name.as_deref())
            };
            notification_readings.battery_kbd = flatten_peripheral_notification(&result);
            flatten_keyboard_battery(result, &mut readings);
        } else if cfg.battery.kbd_bolt.is_some() && !skip_slow {
            let dev_idx = cfg.battery.kbd_bolt;
            let name = cfg.battery.kbd_name.clone();
            if let Some(bolt) = ctx.bolt.as_deref_mut() {
                let decision_at = (ctx.clock)().monotonic;
                let cache = &mut owners.power.battery_kbd_cache;
                let result =
                    if sample_due(cache.bolt_completed_at, decision_at, power::BOLT_CACHE_TTL) {
                        let captured_at = (ctx.clock)();
                        attempt_bolt_peripheral(
                            cache,
                            bolt,
                            dev_idx.unwrap_or(0),
                            name.as_deref(),
                            captured_at,
                            "battery_kbd",
                            &mut timings,
                        )
                    } else {
                        cached_peripheral(cache, name.as_deref())
                    };
                notification_readings.battery_kbd = flatten_peripheral_notification(&result);
                flatten_keyboard_battery(result, &mut readings);
            }
        }
    }
    // ── GPU (NVIDIA then Intel) ────────────────────────────────────────────
    let mut decoder_outcome = gpu_history::DecoderOutcome::Unmeasured;
    if caps.contains(&Capability::GpuNvidia) && hw.has_nvidia && !skip_slow {
        let mut nvml = ctx.nvml.take();
        let nvml_available = nvml.is_some() && !owners.nvidia.cache.nvml_init_failed;
        let nvml_status = if nvml_available {
            let decision_at = (ctx.clock)().monotonic;
            if sample_due(
                owners.nvidia.cache.nvml_attempted_at,
                decision_at,
                gpu_nvidia::GPU_CACHE_TTL_NVML,
            ) {
                let captured_at = (ctx.clock)().monotonic;
                if let Some(facade) = nvml.as_mut() {
                    timed(&mut timings, "gpu_nvidia", || {
                        attempt_nvml_nvidia(owners.nvidia, &mut **facade, captured_at)
                    })
                } else {
                    AttemptStatus::Cached
                }
            } else {
                AttemptStatus::Cached
            }
        } else {
            AttemptStatus::Cached
        };
        let fallback_active = !nvml_available || nvml_status == AttemptStatus::Failed;
        let fallback_status = if fallback_active {
            let decision_at = (ctx.clock)().monotonic;
            if sample_due(
                owners.nvidia.cache.fallback_attempted_at,
                decision_at,
                gpu_nvidia::GPU_CACHE_TTL,
            ) {
                let captured_at = (ctx.clock)().monotonic;
                timed(&mut timings, "gpu_nvidia", || {
                    attempt_nvidia_fallback(owners.nvidia, ctx.commands, captured_at)
                })
            } else {
                AttemptStatus::Cached
            }
        } else {
            AttemptStatus::Cached
        };
        let status = if nvml_status == AttemptStatus::Captured {
            AttemptStatus::Captured
        } else if fallback_active {
            fallback_status
        } else {
            nvml_status
        };
        let result = nvidia_result(owners.nvidia, status);
        decoder_outcome = result.decoder;
        ctx.nvml = nvml;
        flatten_nvidia_notification(&result, &mut notification_readings);
        flatten_nvidia(result, &mut readings);
    }
    if caps.contains(&Capability::GpuIntelFrequency) {
        if let Some(path) = hw.intel_gpu_freq_path.as_deref() {
            clock = (ctx.clock)();
            let result = attempt_intel_frequency(owners.intel_gpu, path, clock, &mut timings);
            flatten_intel_frequency(result, &mut readings);
        }
    }
    let wants_intel_usage = caps.contains(&Capability::GpuIntelUsage);
    let wants_intel_dec = caps.contains(&Capability::GpuIntelDecoder);
    let mut intel_history_comparable = true;
    if let Some(pci) = hw.intel_gpu_pci.clone() {
        if (wants_intel_usage || wants_intel_dec) && !skip_slow {
            clock = (ctx.clock)();
            let result = if owners.intel_gpu.usage_needs_comparable
                || owners.intel_gpu.usage.latest.is_none()
                || sample_due(
                    owners.intel_gpu.usage.attempted_at,
                    clock.monotonic,
                    gpu_intel::INTEL_GPU_USAGE_TTL,
                ) {
                attempt_intel_usage(owners.intel_gpu, proc_root, &pci, clock, &mut timings)
            } else {
                IntelUsageResult {
                    reading: cached_attempt(&owners.intel_gpu.usage),
                }
            };
            intel_history_comparable = !owners.intel_gpu.usage_needs_comparable;
            if !hw.has_nvidia {
                decoder_outcome = intel_decoder_outcome(&result);
            }
            flatten_intel_usage(result, wants_intel_usage, wants_intel_dec, &mut readings);
        }
    }
    clock = (ctx.clock)();
    let history_due =
        owners
            .gpu_history
            .is_due(hw, clock.monotonic, cfg.display.history_interval.duration());
    if let Some(history) = owners.gpu_history.sample(
        cfg,
        hw,
        &readings,
        decoder_outcome,
        clock,
        history_due && (hw.has_nvidia || intel_history_comparable),
    ) {
        flatten_gpu_history(history, &mut readings);
    }

    // ── Brightness + external status files ─────────────────────────────────
    let result = attempt_external(
        owners.external,
        sys_root,
        cfg,
        &caps,
        &mut *ctx.clock,
        &mut timings,
    );
    flatten_external_notification(&result, &mut notification_readings);
    flatten_external(result, &mut readings);

    readings.assembled_at = (ctx.clock)();
    notification_readings.assembled_at = readings.assembled_at;
    CollectionOutput {
        display: readings,
        notifications: notification_readings,
    }
}

/// Computes the capability set requested this poll from the resolved config.
///
/// Mirrors the body of Python's `needed_capabilities(cfg)`: union of every
/// configured item's metric capabilities, capabilities pulled by enabled
/// notification flags, and the `graphs` page's fixed capability set.
pub(super) fn resolve_capabilities(cfg: &Config) -> BTreeSet<Capability> {
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
