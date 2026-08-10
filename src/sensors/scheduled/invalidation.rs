use crate::domain::readings::DisplaySnapshot;
use crate::scheduler::{JobId, JobKind, PeripheralRole, SourceIdentity};

use super::OwnerRefs;

pub(crate) fn invalidate_scheduled_job(
    job: &JobId,
    owners: OwnerRefs<'_>,
    readings: &mut DisplaySnapshot,
) {
    match job.kind {
        JobKind::Cpu => {
            owners.cpu.usage.invalidate();
            owners.cpu.temperature.invalidate();
            owners.cpu.frequency_mhz.invalidate();
            owners.cpu.turbo.invalidate();
            owners.cpu.uptime_seconds.invalidate();
            owners.cpu.load_average.invalidate();
            owners.cpu.cpu_prev_times.clear();
            readings.cpu_usage = None;
            readings.cpu_temp = None;
            readings.cpu_freq_mhz = None;
            readings.cpu_turbo = None;
            readings.uptime_seconds = None;
            readings.load_average = None;
        }
        JobKind::CpuCores => {
            owners.cpu.core_usage.invalidate();
            owners.cpu.cpu_core_prev_times.clear();
            readings.cpu_core_usage = None;
        }
        JobKind::CpuHistory => {
            owners.cpu.cpu_history.clear();
            owners.cpu.cpu_history_sample_at = None;
            readings.cpu_history.clear();
        }
        JobKind::CpuCoreHistory => {
            owners.cpu.cpu_core_history.clear();
            owners.cpu.cpu_core_history_sample_at = None;
            readings.cpu_core_history = None;
        }
        JobKind::Memory => {
            owners.memory.usage.invalidate();
            owners.memory.swap_usage.invalidate();
            readings.mem_usage = None;
            readings.mem_used_gib = None;
            readings.mem_total_gib = None;
            readings.swap_usage = None;
        }
        JobKind::MemoryHistory => {
            owners.memory.mem_history.clear();
            owners.memory.mem_history_sample_at = None;
            readings.mem_history.clear();
        }
        JobKind::NetworkRate => {
            owners.network.reset_rate();
            readings.net_up_bps = None;
            readings.net_down_bps = None;
        }
        JobKind::NetworkIdentity => {
            owners.network.reset_info();
            readings.net_device = None;
            readings.ip_address = None;
            readings.wifi_ssid = None;
            readings.wifi_signal_percent = None;
        }
        JobKind::NetworkHistory => {
            owners.network.reset_history();
            readings.net_up_history.clear();
            readings.net_down_history.clear();
        }
        JobKind::DiskIo => {
            owners.disk.reset_io();
            readings.disk_read_bps = None;
            readings.disk_write_bps = None;
        }
        JobKind::DiskUsage => {
            if let SourceIdentity::Mount(path) = &job.source {
                let key = path.to_string_lossy();
                owners.disk.usage.remove(key.as_ref());
                readings.disk_usage.remove(key.as_ref());
            }
        }
        JobKind::Smart => {
            if let SourceIdentity::SmartDrive { label, .. } = &job.source {
                owners.disk.smart_cache.remove(label);
                owners.disk.smart_sources.remove(label);
                readings.disk_smart.remove(label);
            }
        }
        JobKind::DiskTemperature => {
            if let SourceIdentity::NamedPath { name, .. } = &job.source {
                owners.disk.hd_temp_cache.remove(name);
                owners.disk.hd_temp_sources.remove(name);
                readings.hd_temps.remove(name);
            }
        }
        JobKind::FanSpeed => {
            if let SourceIdentity::NamedPath { name, .. } = &job.source {
                owners.disk.fan_speed_cache.remove(name);
                owners.disk.fan_speed_sources.remove(name);
                readings.fan_speeds.remove(name);
            }
        }
        JobKind::SystemBattery => {
            if let SourceIdentity::Battery(id) = &job.source {
                owners.power.battery_sys_cache.remove(id);
                readings.battery_sys.retain(|battery| battery.id != *id);
            }
        }
        JobKind::PeripheralBattery => {
            if let SourceIdentity::Peripheral { role, .. } = &job.source {
                match role {
                    PeripheralRole::Mouse => {
                        owners.power.battery_mouse_cache = Default::default();
                        readings.battery_mouse = None;
                    }
                    PeripheralRole::Keyboard => {
                        owners.power.battery_kbd_cache = Default::default();
                        readings.battery_kbd = None;
                    }
                }
            }
        }
        JobKind::NvidiaNvml | JobKind::NvidiaFallback => {
            owners.nvidia.cache = Default::default();
            readings.gpu_temp = None;
            readings.gpu_usage = None;
            readings.gpu_mem = None;
            readings.gpu_dec = None;
            readings.gpu_fan = None;
        }
        JobKind::IntelFrequency => {
            owners.intel_gpu.frequency.invalidate();
            readings.gpu_intel_freq = None;
        }
        JobKind::IntelUsage => {
            owners.intel_gpu.usage.invalidate();
            owners.intel_gpu.engine_prev.clear();
            owners.intel_gpu.prev_sample_at = None;
            owners.intel_gpu.usage_cache.clear();
            readings.gpu_intel_usage = None;
            readings.gpu_intel_dec_usage = None;
        }
        JobKind::GpuHistory => {
            *owners.gpu_history = Default::default();
            readings.gpu_usage_history.clear();
            readings.gpu_dec_history.clear();
        }
        JobKind::PanelProcesses => {
            owners.process.reset_panel();
            readings.top_process = None;
        }
        JobKind::PageProcesses => {
            owners.process.page.invalidate();
            owners.process.page_proc_prev_times.clear();
            owners.process.page_proc_prev_sample_at = None;
            readings.top_process_full = None;
        }
        JobKind::Brightness => {
            owners.external.brightness.invalidate();
            readings.screen_brightness = None;
        }
        JobKind::UpdatesFile => {
            owners.external.updates.invalidate();
            owners.external.updates_source = None;
            readings.system_updates = None;
        }
        JobKind::ServerFile => {
            owners.external.server.invalidate();
            owners.external.server_source = None;
            readings.server_ok = None;
        }
        JobKind::PageCommand
        | JobKind::PageRender
        | JobKind::HardwareDiscovery
        | JobKind::MountInventory => {}
    }
}

#[cfg(test)]
#[path = "invalidation/tests.rs"]
mod tests;
