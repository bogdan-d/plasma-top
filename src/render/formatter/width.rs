//! Canonical tooltip-width input widening.

use crate::domain::{BatteryState, DisplaySnapshot, LoadAverage};

use super::PanelFormatter;

impl PanelFormatter<'_> {
    pub(super) fn maxed_readings(&self, readings: &DisplaySnapshot) -> DisplaySnapshot {
        let mut widened = readings.clone();
        widened.cpu_usage = Some(100);
        widened.mem_usage = Some(100);
        widened.swap_usage = Some(100);
        widened.cpu_temp = Some(100);
        widened.gpu_temp = Some(100);
        widened.gpu_usage = Some(100);
        widened.gpu_mem = Some(100);
        widened.gpu_dec = Some(100);
        widened.gpu_fan = Some(100);
        widened.gpu_intel_usage = Some(100);
        widened.gpu_intel_dec_usage = Some(100);
        widened.gpu_intel_freq = Some(9999);
        widened.cpu_freq_mhz = Some(9999.0);
        widened.screen_brightness = Some(100);
        widened.wifi_signal_percent = Some(100);
        widened.net_up_bps = Some(999_000_000);
        widened.net_down_bps = Some(999_000_000);
        widened.disk_read_bps = Some(999_000_000);
        widened.disk_write_bps = Some(999_000_000);
        widened.ip_address = Some(String::from("255.255.255.255"));
        if widened.net_device.is_none() {
            widened.net_device = self.hw.net_device.clone();
        }
        if let Some(total) = widened.mem_total_gib {
            widened.mem_used_gib = Some(total);
        }
        for usage in widened.disk_usage.values_mut().flatten() {
            usage.percent = 100;
            usage.used_gib = usage.total_gib;
        }
        widened.hd_temps = self
            .hw
            .hd_temp_paths
            .keys()
            .cloned()
            .map(|label| (label, Some(100)))
            .collect();
        widened.fan_speeds = self
            .hw
            .fan_paths
            .keys()
            .cloned()
            .map(|label| (label, Some(9999)))
            .collect();
        widened.disk_smart = self
            .hw
            .disk_smart_drives
            .keys()
            .cloned()
            .map(|label| (label, Some(true)))
            .collect();
        for battery in &mut widened.battery_sys {
            battery.charge_percent = 100;
            battery.rate_watts = 99;
            battery.state = BatteryState::Discharging;
            if battery.charge_limit_percent.is_none() {
                battery.charge_limit_percent = Some(80);
            }
        }
        if let Some(mouse) = &mut widened.battery_mouse {
            mouse.charge_percent = 100;
        }
        if let Some(keyboard) = &mut widened.battery_kbd {
            keyboard.charge_percent = 100;
        }
        if let Some(processes) = &mut widened.top_process {
            for process in processes {
                process.command = "X".repeat(15);
                process.cpu_percent = 100;
            }
        }
        let cores = self.hw.cpu_count.max(1) as f64;
        widened.load_average = Some(LoadAverage {
            one: cores,
            five: cores,
            fifteen: cores,
        });
        widened.uptime_seconds = Some(999 * 86_400 + 23 * 3600 + 59 * 60);
        widened
    }
}
