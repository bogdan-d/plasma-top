//! Formatter-side token resolution and hardware gates.

use std::str::FromStr;

use crate::config::Config;
use crate::domain::{Form, HardwareSnapshot, ItemToken, Metric, ReadingsSnapshot};

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
    hw: &HardwareSnapshot,
    token: &ItemToken,
    readings: &ReadingsSnapshot,
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
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;
    use crate::config::BatteryConfig;
    use crate::config::Config;
    use crate::domain::ItemRendering;

    fn bare_hw() -> HardwareSnapshot {
        HardwareSnapshot {
            net_device: Some(String::from("enp0s3")),
            disk_io_device: Some(String::from("sda")),
            cpu_count: 2,
            ..HardwareSnapshot::default()
        }
    }

    #[test]
    fn bar_form_css_token_depends_on_orientation() {
        assert_eq!(form_token(Some(Form::Bar), true), Some("bar"));
        assert_eq!(form_token(Some(Form::Bar), false), Some("column"));
    }

    #[test]
    fn resolve_item_parses_valid_tokens() {
        let cpu = resolve_item("cpu_usage:spark_value", false).expect("cpu token");
        let net = resolve_item("net_speed", false).expect("intrinsic token");

        assert_eq!(cpu.form_token, Some("spark_value"));
        assert!(matches!(
            cpu.token.rendering(),
            ItemRendering::Generic(Form::SparkValue)
        ));
        assert_eq!(net.form_token, None);
    }

    #[test]
    fn item_gates_match_python_rules() {
        let mut cfg = Config::default();
        let hw = bare_hw();
        let readings = ReadingsSnapshot::default();

        assert!(!item_gate(
            &cfg,
            &hw,
            &ItemToken::from_str("cpu_temp").expect("token"),
            &readings,
        ));
        assert!(item_gate(
            &cfg,
            &hw,
            &ItemToken::from_str("net_speed").expect("token"),
            &readings,
        ));

        cfg.battery = BatteryConfig {
            kbd_bolt: Some(1),
            ..BatteryConfig::default()
        };
        assert!(item_gate(
            &cfg,
            &hw,
            &ItemToken::from_str("battery_kbd").expect("token"),
            &readings,
        ));
    }
}
