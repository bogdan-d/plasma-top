//! Formatter-side token resolution and hardware gates.

use std::str::FromStr;

use crate::config::Config;
use crate::domain::{DisplaySnapshot, Form, HardwareInventory, ItemToken, Metric};

use super::traces::TraceMetric;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResolvedItem {
    pub(crate) token: ItemToken,
    pub(crate) form_token: Option<&'static str>,
}

pub(crate) fn resolve_item(token: &str, vertical: bool) -> Option<ResolvedItem> {
    let token = ItemToken::from_str(token).ok()?;
    Some(ResolvedItem {
        form_token: form_token(token.form(), vertical),
        token,
    })
}

pub(crate) const fn form_token(form: Option<Form>, vertical: bool) -> Option<&'static str> {
    match form {
        None => None,
        Some(Form::Bar) => Some(if vertical { "bar" } else { "column" }),
        Some(other) => Some(other.as_str()),
    }
}

pub(crate) const fn trace_metric(metric: Metric) -> Option<TraceMetric> {
    match metric {
        Metric::CpuUsage => Some(TraceMetric::Cpu),
        Metric::MemUsage => Some(TraceMetric::Memory),
        _ => None,
    }
}

pub(crate) fn item_gate(
    cfg: &Config,
    hw: &HardwareInventory,
    token: &ItemToken,
    readings: &DisplaySnapshot,
) -> bool {
    match token.metric() {
        Metric::CpuTemp => hw.cpu_temp_path.is_some(),
        Metric::CpuTurbo => hw.cpu_turbo_supported,
        Metric::NetSpeed | Metric::NetDevice | Metric::NetIp | Metric::NetDeviceIp => {
            hw.net_device.is_some()
        }
        Metric::DiskIo => hw.disk_io_device.is_some(),
        Metric::WifiSsid | Metric::WifiSignal | Metric::WifiSsidSignal => hw.has_wifi,
        Metric::FanSpeed => !hw.fan_paths.is_empty(),
        Metric::GpuNvidiaTemp
        | Metric::GpuNvidiaUsage
        | Metric::GpuNvidiaMemUsage
        | Metric::GpuNvidiaDecoderUsage
        | Metric::GpuNvidiaFanSpeed => hw.has_nvidia,
        Metric::GpuIntelFreq => hw.intel_gpu_freq_path.is_some(),
        Metric::GpuIntelUsage | Metric::GpuIntelDecoderUsage => hw.intel_gpu_pci.is_some(),
        Metric::BatterySystem => !hw.battery_sys_ids.is_empty(),
        Metric::BatteryMouse => hw.battery_mouse_id.is_some() || cfg.battery.mouse_bolt.is_some(),
        Metric::BatteryKeyboard => hw.battery_kbd_id.is_some() || cfg.battery.kbd_bolt.is_some(),
        Metric::ScreenBrightness => hw.has_backlight,
        Metric::SwapUsage => readings.swap_usage.is_some(),
        Metric::SystemUpdates => !cfg.system_updates.file.is_empty(),
        Metric::ServerCheck => !cfg.server_check.file.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod tests;
