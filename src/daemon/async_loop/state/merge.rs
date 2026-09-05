use super::*;

pub(super) fn apply_inventory_completion(
    completion: &JobCompletion,
    hw: &mut HardwareInventory,
    resolved_mounts: &mut Vec<String>,
) {
    match completion.ticket.job.kind {
        crate::scheduler::JobKind::NetworkIdentity => {
            hw.net_device.clone_from(&completion.hw.net_device);
        }
        crate::scheduler::JobKind::MountInventory => {
            resolved_mounts.clone_from(&completion.resolved_mounts);
        }
        crate::scheduler::JobKind::HardwareDiscovery => {
            let crate::scheduler::SourceIdentity::Inventory(family) = completion.ticket.job.source
            else {
                return;
            };
            apply_inventory_family(family, &completion.hw, hw);
        }
        _ => {}
    }
}

pub(super) fn apply_inventory_family(
    family: InventoryFamily,
    source: &HardwareInventory,
    target: &mut HardwareInventory,
) {
    match family {
        InventoryFamily::Cpu => {
            target.cpu_temp_path.clone_from(&source.cpu_temp_path);
            target.cpu_freq_path.clone_from(&source.cpu_freq_path);
            target.cpu_turbo_path.clone_from(&source.cpu_turbo_path);
            target.cpu_turbo_supported = source.cpu_turbo_supported;
        }
        InventoryFamily::Thermal => {
            target.hd_temp_paths.clone_from(&source.hd_temp_paths);
            target.fan_paths.clone_from(&source.fan_paths);
        }
        InventoryFamily::SystemBattery => {
            target.battery_sys_ids.clone_from(&source.battery_sys_ids);
        }
        InventoryFamily::Smart => {
            target
                .disk_smart_drives
                .clone_from(&source.disk_smart_drives);
        }
        InventoryFamily::Nvidia => target.has_nvidia = source.has_nvidia,
        InventoryFamily::Amd => target.amd_gpu.clone_from(&source.amd_gpu),
        InventoryFamily::Intel => {
            target
                .intel_gpu_freq_path
                .clone_from(&source.intel_gpu_freq_path);
            target.intel_gpu_pci.clone_from(&source.intel_gpu_pci);
        }
        InventoryFamily::Backlight => target.has_backlight = source.has_backlight,
        InventoryFamily::Network => {
            target.net_device.clone_from(&source.net_device);
            target.has_wifi = source.has_wifi;
        }
        InventoryFamily::DiskIo => {
            target.disk_io_device.clone_from(&source.disk_io_device);
        }
        InventoryFamily::Peripheral => {
            target.battery_mouse_id.clone_from(&source.battery_mouse_id);
            target.battery_kbd_id.clone_from(&source.battery_kbd_id);
        }
    }
}

pub(super) fn apply_owner_readings(
    owner: OwnerId,
    source: &DisplaySnapshot,
    target: &mut DisplaySnapshot,
) {
    match owner {
        OwnerId::Cpu => {
            target.cpu_usage = source.cpu_usage;
            target.cpu_temp = source.cpu_temp;
            target.cpu_freq_mhz = source.cpu_freq_mhz;
            target.cpu_turbo = source.cpu_turbo;
            target.cpu_history.clone_from(&source.cpu_history);
            target.uptime_seconds = source.uptime_seconds;
            target.load_average = source.load_average;
            target.cpu_core_usage.clone_from(&source.cpu_core_usage);
            target.cpu_core_history.clone_from(&source.cpu_core_history);
        }
        OwnerId::Process => {
            target.top_process.clone_from(&source.top_process);
            target.top_process_full.clone_from(&source.top_process_full);
        }
        OwnerId::Memory => {
            target.mem_history.clone_from(&source.mem_history);
            target.mem_usage = source.mem_usage;
            target.mem_used_gib = source.mem_used_gib;
            target.mem_total_gib = source.mem_total_gib;
            target.swap_usage = source.swap_usage;
        }
        OwnerId::Network => {
            target.net_up_bps = source.net_up_bps;
            target.net_down_bps = source.net_down_bps;
            target.net_device.clone_from(&source.net_device);
            target.ip_address.clone_from(&source.ip_address);
            target.wifi_ssid.clone_from(&source.wifi_ssid);
            target.wifi_signal_percent = source.wifi_signal_percent;
            target.net_up_history.clone_from(&source.net_up_history);
            target.net_down_history.clone_from(&source.net_down_history);
        }
        OwnerId::Disk => {
            target.disk_read_bps = source.disk_read_bps;
            target.disk_write_bps = source.disk_write_bps;
            target.disk_usage.clone_from(&source.disk_usage);
            target.disk_smart.clone_from(&source.disk_smart);
            target.hd_temps.clone_from(&source.hd_temps);
            target.fan_speeds.clone_from(&source.fan_speeds);
        }
        OwnerId::Power => {
            target.battery_sys.clone_from(&source.battery_sys);
            target.battery_mouse.clone_from(&source.battery_mouse);
            target.battery_kbd.clone_from(&source.battery_kbd);
        }
        OwnerId::Nvidia => {
            target.gpu_temp = source.gpu_temp;
            target.gpu_usage = source.gpu_usage;
            target.gpu_mem = source.gpu_mem;
            target.gpu_dec = source.gpu_dec;
            target.gpu_fan = source.gpu_fan;
        }
        OwnerId::AmdGpu => {
            target.gpu_amd_usage = source.gpu_amd_usage;
            target.gpu_amd_codec_usage = source.gpu_amd_codec_usage;
            target.gpu_amd_mem_usage = source.gpu_amd_mem_usage;
            target.gpu_amd_freq = source.gpu_amd_freq;
            target.gpu_amd_temp = source.gpu_amd_temp;
            target.gpu_amd_power = source.gpu_amd_power;
            target.gpu_amd_fan_speed = source.gpu_amd_fan_speed;
        }
        OwnerId::IntelGpu => {
            target.gpu_intel_freq = source.gpu_intel_freq;
            target.gpu_intel_usage = source.gpu_intel_usage;
            target.gpu_intel_dec_usage = source.gpu_intel_dec_usage;
        }
        OwnerId::GpuHistory => {
            target
                .gpu_usage_history
                .clone_from(&source.gpu_usage_history);
            target.gpu_dec_history.clone_from(&source.gpu_dec_history);
        }
        OwnerId::External => {
            target.screen_brightness = source.screen_brightness;
            target.system_updates = source.system_updates;
            target.server_ok = source.server_ok;
        }
        OwnerId::Page | OwnerId::Discovery => {}
    }
}
