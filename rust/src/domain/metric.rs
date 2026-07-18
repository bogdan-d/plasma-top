//! Metric and capability contracts.

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use crate::domain::form::{Form, Shape, SurfaceSet};

const HISTORIED_FORMS: &[Form] = &[
    Form::Value,
    Form::Bar,
    Form::Spark,
    Form::Braille,
    Form::SparkValue,
    Form::BrailleValue,
    Form::BarSpark,
    Form::BarBraille,
];
const VALUE_ONLY_FORMS: &[Form] = &[Form::Value];
const VALUE_OR_PAIR_FORMS: &[Form] = &[Form::Value, Form::Pair];
const PAIR_ONLY_FORMS: &[Form] = &[Form::Pair];
const NO_FORMS: &[Form] = &[];

const NO_CAPABILITIES: &[Capability] = &[];
const CAP_SWAP_USAGE: &[Capability] = &[Capability::SwapUsage];
const CAP_CPU_FREQ_TURBO: &[Capability] = &[Capability::CpuFrequency, Capability::CpuTurbo];
const CAP_CPU_TURBO: &[Capability] = &[Capability::CpuTurbo];
const CAP_CPU_TEMP: &[Capability] = &[Capability::CpuTemperature];
const CAP_HD_TEMP: &[Capability] = &[Capability::DiskTemperature];
const CAP_DISK_USAGE: &[Capability] = &[Capability::DiskUsage];
const CAP_DISK_SMART: &[Capability] = &[Capability::DiskSmart];
const CAP_GPU_NVIDIA: &[Capability] = &[Capability::GpuNvidia];
const CAP_GPU_INTEL_FREQ: &[Capability] = &[Capability::GpuIntelFrequency];
const CAP_GPU_INTEL_USAGE: &[Capability] = &[Capability::GpuIntelUsage];
const CAP_GPU_INTEL_DECODER: &[Capability] = &[Capability::GpuIntelDecoder];
const CAP_SCREEN_BRIGHTNESS: &[Capability] = &[Capability::ScreenBrightness];
const CAP_FAN_SPEED: &[Capability] = &[Capability::FanSpeed];
const CAP_BATTERY_SYSTEM: &[Capability] = &[Capability::BatterySystem];
const CAP_BATTERY_MOUSE: &[Capability] = &[Capability::BatteryMouse];
const CAP_BATTERY_KEYBOARD: &[Capability] = &[Capability::BatteryKeyboard];
const CAP_NET_SPEED: &[Capability] = &[Capability::NetworkSpeed];
const CAP_DISK_IO: &[Capability] = &[Capability::DiskIo];
const CAP_NET_INFO: &[Capability] = &[Capability::NetworkInfo];
const CAP_UPTIME: &[Capability] = &[Capability::Uptime];
const CAP_LOAD_AVERAGE: &[Capability] = &[Capability::LoadAverage];
const CAP_TOP_PROCESS: &[Capability] = &[Capability::TopProcess];
const CAP_SYSTEM_UPDATES: &[Capability] = &[Capability::SystemUpdates];
const CAP_SERVER_CHECK: &[Capability] = &[Capability::ServerCheck];

const ALL_METRICS: [Metric; 35] = [
    Metric::CpuUsage,
    Metric::MemUsage,
    Metric::SwapUsage,
    Metric::CpuFreq,
    Metric::CpuTurbo,
    Metric::CpuTemp,
    Metric::HdTemp,
    Metric::DiskUsage,
    Metric::DiskSmart,
    Metric::GpuNvidiaTemp,
    Metric::GpuNvidiaUsage,
    Metric::GpuNvidiaMemUsage,
    Metric::GpuNvidiaDecoderUsage,
    Metric::GpuNvidiaFanSpeed,
    Metric::GpuIntelFreq,
    Metric::GpuIntelUsage,
    Metric::GpuIntelDecoderUsage,
    Metric::ScreenBrightness,
    Metric::FanSpeed,
    Metric::BatterySystem,
    Metric::BatteryMouse,
    Metric::BatteryKeyboard,
    Metric::NetSpeed,
    Metric::DiskIo,
    Metric::NetDevice,
    Metric::NetIp,
    Metric::NetDeviceIp,
    Metric::WifiSsid,
    Metric::WifiSignal,
    Metric::WifiSsidSignal,
    Metric::Uptime,
    Metric::LoadAverage,
    Metric::TopProcess,
    Metric::SystemUpdates,
    Metric::ServerCheck,
];

/// Closed set of known metrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Metric {
    /// CPU usage percentage.
    CpuUsage,
    /// Memory usage percentage.
    MemUsage,
    /// Swap usage percentage.
    SwapUsage,
    /// CPU frequency.
    CpuFreq,
    /// CPU turbo state.
    CpuTurbo,
    /// CPU temperature.
    CpuTemp,
    /// Disk temperatures.
    HdTemp,
    /// Disk usage percentage with auxiliary space data.
    DiskUsage,
    /// Disk SMART health.
    DiskSmart,
    /// NVIDIA GPU temperature.
    GpuNvidiaTemp,
    /// NVIDIA GPU usage.
    GpuNvidiaUsage,
    /// NVIDIA GPU memory usage.
    GpuNvidiaMemUsage,
    /// NVIDIA GPU decoder usage.
    GpuNvidiaDecoderUsage,
    /// NVIDIA GPU fan speed.
    GpuNvidiaFanSpeed,
    /// Intel GPU frequency.
    GpuIntelFreq,
    /// Intel GPU usage.
    GpuIntelUsage,
    /// Intel GPU decoder usage.
    GpuIntelDecoderUsage,
    /// Screen brightness percentage.
    ScreenBrightness,
    /// Fan speeds.
    FanSpeed,
    /// System battery state.
    BatterySystem,
    /// Mouse battery state.
    BatteryMouse,
    /// Keyboard battery state.
    BatteryKeyboard,
    /// Network throughput.
    NetSpeed,
    /// Disk throughput.
    DiskIo,
    /// Active network device name.
    NetDevice,
    /// Active network IP address.
    NetIp,
    /// Combined network device and IP.
    NetDeviceIp,
    /// Wi-Fi SSID.
    WifiSsid,
    /// Wi-Fi signal strength.
    WifiSignal,
    /// Combined Wi-Fi SSID and signal.
    WifiSsidSignal,
    /// System uptime.
    Uptime,
    /// System load average.
    LoadAverage,
    /// Top process row.
    TopProcess,
    /// System updates status.
    SystemUpdates,
    /// External server check status.
    ServerCheck,
}

/// Capability switches used to request hardware or command data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Capability {
    /// Swap usage support.
    SwapUsage,
    /// CPU frequency support.
    CpuFrequency,
    /// CPU turbo support.
    CpuTurbo,
    /// CPU temperature support.
    CpuTemperature,
    /// Disk temperature support.
    DiskTemperature,
    /// Disk usage support.
    DiskUsage,
    /// Disk SMART support.
    DiskSmart,
    /// NVIDIA GPU support.
    GpuNvidia,
    /// Intel GPU frequency support.
    GpuIntelFrequency,
    /// Intel GPU usage support.
    GpuIntelUsage,
    /// Intel GPU decoder support.
    GpuIntelDecoder,
    /// Screen brightness support.
    ScreenBrightness,
    /// Fan speed support.
    FanSpeed,
    /// System battery support.
    BatterySystem,
    /// Mouse battery support.
    BatteryMouse,
    /// Keyboard battery support.
    BatteryKeyboard,
    /// Network throughput support.
    NetworkSpeed,
    /// Disk throughput support.
    DiskIo,
    /// Network identity support.
    NetworkInfo,
    /// Uptime support.
    Uptime,
    /// Load average support.
    LoadAverage,
    /// Top-process support.
    TopProcess,
    /// System updates support.
    SystemUpdates,
    /// External server check support.
    ServerCheck,
}

/// Frozen metadata for a metric contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricSpec {
    /// Metric identity.
    pub metric: Metric,
    /// Capabilities needed to collect the metric.
    pub capabilities: &'static [Capability],
    /// Generic forms accepted by the metric.
    pub generic_forms: &'static [Form],
    /// Metric-level admitted surfaces before form intersection.
    pub surfaces: SurfaceSet,
    /// Intrinsic shape for metrics that do not accept generic forms.
    pub intrinsic_shape: Option<Shape>,
}

impl Metric {
    /// Returns all currently known metrics in a deterministic order.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &ALL_METRICS
    }

    /// Returns the stable snake_case token used by config and CSS.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CpuUsage => "cpu_usage",
            Self::MemUsage => "mem_usage",
            Self::SwapUsage => "swap_usage",
            Self::CpuFreq => "cpu_freq",
            Self::CpuTurbo => "cpu_turbo",
            Self::CpuTemp => "cpu_temp",
            Self::HdTemp => "hd_temp",
            Self::DiskUsage => "disk_usage",
            Self::DiskSmart => "disk_smart",
            Self::GpuNvidiaTemp => "gpu_nvidia_temp",
            Self::GpuNvidiaUsage => "gpu_nvidia_usage",
            Self::GpuNvidiaMemUsage => "gpu_nvidia_mem_usage",
            Self::GpuNvidiaDecoderUsage => "gpu_nvidia_dec_usage",
            Self::GpuNvidiaFanSpeed => "gpu_nvidia_fan_speed",
            Self::GpuIntelFreq => "gpu_intel_freq",
            Self::GpuIntelUsage => "gpu_intel_usage",
            Self::GpuIntelDecoderUsage => "gpu_intel_dec_usage",
            Self::ScreenBrightness => "screen_brightness",
            Self::FanSpeed => "fan_speed",
            Self::BatterySystem => "battery_sys",
            Self::BatteryMouse => "battery_mouse",
            Self::BatteryKeyboard => "battery_kbd",
            Self::NetSpeed => "net_speed",
            Self::DiskIo => "disk_io",
            Self::NetDevice => "net_device",
            Self::NetIp => "net_ip",
            Self::NetDeviceIp => "net_device_ip",
            Self::WifiSsid => "wifi_ssid",
            Self::WifiSignal => "wifi_signal",
            Self::WifiSsidSignal => "wifi_ssid_signal",
            Self::Uptime => "uptime",
            Self::LoadAverage => "load_avg",
            Self::TopProcess => "top_process",
            Self::SystemUpdates => "system_updates",
            Self::ServerCheck => "server_check",
        }
    }

    /// Returns the frozen scaffold metadata for the metric.
    #[must_use]
    pub fn spec(self) -> MetricSpec {
        match self {
            Self::CpuUsage => MetricSpec {
                metric: self,
                capabilities: NO_CAPABILITIES,
                generic_forms: HISTORIED_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::MemUsage => MetricSpec {
                metric: self,
                capabilities: NO_CAPABILITIES,
                generic_forms: HISTORIED_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::SwapUsage => MetricSpec {
                metric: self,
                capabilities: CAP_SWAP_USAGE,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::CpuFreq => MetricSpec {
                metric: self,
                capabilities: CAP_CPU_FREQ_TURBO,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::CpuTurbo => MetricSpec {
                metric: self,
                capabilities: CAP_CPU_TURBO,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::CpuTemp => MetricSpec {
                metric: self,
                capabilities: CAP_CPU_TEMP,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::HdTemp => MetricSpec {
                metric: self,
                capabilities: CAP_HD_TEMP,
                generic_forms: VALUE_OR_PAIR_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::DiskUsage => MetricSpec {
                metric: self,
                capabilities: CAP_DISK_USAGE,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::DiskSmart => MetricSpec {
                metric: self,
                capabilities: CAP_DISK_SMART,
                generic_forms: PAIR_ONLY_FORMS,
                surfaces: SurfaceSet::TOOLTIP,
                intrinsic_shape: None,
            },
            Self::GpuNvidiaTemp => MetricSpec {
                metric: self,
                capabilities: CAP_GPU_NVIDIA,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::GpuNvidiaUsage => MetricSpec {
                metric: self,
                capabilities: CAP_GPU_NVIDIA,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::GpuNvidiaMemUsage => MetricSpec {
                metric: self,
                capabilities: CAP_GPU_NVIDIA,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::GpuNvidiaDecoderUsage => MetricSpec {
                metric: self,
                capabilities: CAP_GPU_NVIDIA,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::GpuNvidiaFanSpeed => MetricSpec {
                metric: self,
                capabilities: CAP_GPU_NVIDIA,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::GpuIntelFreq => MetricSpec {
                metric: self,
                capabilities: CAP_GPU_INTEL_FREQ,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::GpuIntelUsage => MetricSpec {
                metric: self,
                capabilities: CAP_GPU_INTEL_USAGE,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::GpuIntelDecoderUsage => MetricSpec {
                metric: self,
                capabilities: CAP_GPU_INTEL_DECODER,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::ScreenBrightness => MetricSpec {
                metric: self,
                capabilities: CAP_SCREEN_BRIGHTNESS,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::FanSpeed => MetricSpec {
                metric: self,
                capabilities: CAP_FAN_SPEED,
                generic_forms: VALUE_OR_PAIR_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::BatterySystem => MetricSpec {
                metric: self,
                capabilities: CAP_BATTERY_SYSTEM,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::BatteryMouse => MetricSpec {
                metric: self,
                capabilities: CAP_BATTERY_MOUSE,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::BatteryKeyboard => MetricSpec {
                metric: self,
                capabilities: CAP_BATTERY_KEYBOARD,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::NetSpeed => MetricSpec {
                metric: self,
                capabilities: CAP_NET_SPEED,
                generic_forms: NO_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: Some(Shape::Duo),
            },
            Self::DiskIo => MetricSpec {
                metric: self,
                capabilities: CAP_DISK_IO,
                generic_forms: NO_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: Some(Shape::Duo),
            },
            Self::NetDevice => MetricSpec {
                metric: self,
                capabilities: CAP_NET_INFO,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::TOOLTIP,
                intrinsic_shape: None,
            },
            Self::NetIp => MetricSpec {
                metric: self,
                capabilities: CAP_NET_INFO,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::TOOLTIP,
                intrinsic_shape: None,
            },
            Self::NetDeviceIp => MetricSpec {
                metric: self,
                capabilities: CAP_NET_INFO,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::TOOLTIP,
                intrinsic_shape: None,
            },
            Self::WifiSsid => MetricSpec {
                metric: self,
                capabilities: CAP_NET_INFO,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::TOOLTIP,
                intrinsic_shape: None,
            },
            Self::WifiSignal => MetricSpec {
                metric: self,
                capabilities: CAP_NET_INFO,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::WifiSsidSignal => MetricSpec {
                metric: self,
                capabilities: CAP_NET_INFO,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::TOOLTIP,
                intrinsic_shape: None,
            },
            Self::Uptime => MetricSpec {
                metric: self,
                capabilities: CAP_UPTIME,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::TOOLTIP,
                intrinsic_shape: None,
            },
            Self::LoadAverage => MetricSpec {
                metric: self,
                capabilities: CAP_LOAD_AVERAGE,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::TOOLTIP,
                intrinsic_shape: None,
            },
            Self::TopProcess => MetricSpec {
                metric: self,
                capabilities: CAP_TOP_PROCESS,
                generic_forms: NO_FORMS,
                surfaces: SurfaceSet::TOOLTIP,
                intrinsic_shape: Some(Shape::TripleL),
            },
            Self::SystemUpdates => MetricSpec {
                metric: self,
                capabilities: CAP_SYSTEM_UPDATES,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
            Self::ServerCheck => MetricSpec {
                metric: self,
                capabilities: CAP_SERVER_CHECK,
                generic_forms: VALUE_ONLY_FORMS,
                surfaces: SurfaceSet::ALL,
                intrinsic_shape: None,
            },
        }
    }

    /// Returns `true` when the metric accepts the provided generic form.
    #[must_use]
    pub fn supports_form(self, form: Form) -> bool {
        self.spec().generic_forms.contains(&form)
    }

    /// Returns the metric-level surfaces before form intersection.
    #[must_use]
    pub fn surfaces(self) -> SurfaceSet {
        self.spec().surfaces
    }

    /// Returns the metric's intrinsic shape, if any.
    #[must_use]
    pub fn intrinsic_shape(self) -> Option<Shape> {
        self.spec().intrinsic_shape
    }

    /// Returns the metric's capability requirements.
    #[must_use]
    pub fn capabilities(self) -> &'static [Capability] {
        self.spec().capabilities
    }
}

impl Display for Metric {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl Display for Capability {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::SwapUsage => "swap_usage",
            Self::CpuFrequency => "cpu_freq",
            Self::CpuTurbo => "cpu_turbo",
            Self::CpuTemperature => "cpu_temp",
            Self::DiskTemperature => "hd_temp",
            Self::DiskUsage => "disk_usage",
            Self::DiskSmart => "disk_smart",
            Self::GpuNvidia => "gpu_nvidia",
            Self::GpuIntelFrequency => "gpu_intel_freq",
            Self::GpuIntelUsage => "gpu_intel_usage",
            Self::GpuIntelDecoder => "gpu_intel_dec",
            Self::ScreenBrightness => "screen_brightness",
            Self::FanSpeed => "fan_speed",
            Self::BatterySystem => "battery_sys",
            Self::BatteryMouse => "battery_mouse",
            Self::BatteryKeyboard => "battery_kbd",
            Self::NetworkSpeed => "net_speed",
            Self::DiskIo => "disk_io",
            Self::NetworkInfo => "net_info",
            Self::Uptime => "uptime",
            Self::LoadAverage => "load_avg",
            Self::TopProcess => "top_process",
            Self::SystemUpdates => "system_updates",
            Self::ServerCheck => "server_check",
        };

        formatter.write_str(text)
    }
}

impl FromStr for Metric {
    type Err = MetricParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "cpu_usage" => Ok(Self::CpuUsage),
            "mem_usage" => Ok(Self::MemUsage),
            "swap_usage" => Ok(Self::SwapUsage),
            "cpu_freq" => Ok(Self::CpuFreq),
            "cpu_turbo" => Ok(Self::CpuTurbo),
            "cpu_temp" => Ok(Self::CpuTemp),
            "hd_temp" => Ok(Self::HdTemp),
            "disk_usage" => Ok(Self::DiskUsage),
            "disk_smart" => Ok(Self::DiskSmart),
            "gpu_nvidia_temp" => Ok(Self::GpuNvidiaTemp),
            "gpu_nvidia_usage" => Ok(Self::GpuNvidiaUsage),
            "gpu_nvidia_mem_usage" => Ok(Self::GpuNvidiaMemUsage),
            "gpu_nvidia_dec_usage" => Ok(Self::GpuNvidiaDecoderUsage),
            "gpu_nvidia_fan_speed" => Ok(Self::GpuNvidiaFanSpeed),
            "gpu_intel_freq" => Ok(Self::GpuIntelFreq),
            "gpu_intel_usage" => Ok(Self::GpuIntelUsage),
            "gpu_intel_dec_usage" => Ok(Self::GpuIntelDecoderUsage),
            "screen_brightness" => Ok(Self::ScreenBrightness),
            "fan_speed" => Ok(Self::FanSpeed),
            "battery_sys" => Ok(Self::BatterySystem),
            "battery_mouse" => Ok(Self::BatteryMouse),
            "battery_kbd" => Ok(Self::BatteryKeyboard),
            "net_speed" => Ok(Self::NetSpeed),
            "disk_io" => Ok(Self::DiskIo),
            "net_device" => Ok(Self::NetDevice),
            "net_ip" => Ok(Self::NetIp),
            "net_device_ip" => Ok(Self::NetDeviceIp),
            "wifi_ssid" => Ok(Self::WifiSsid),
            "wifi_signal" => Ok(Self::WifiSignal),
            "wifi_ssid_signal" => Ok(Self::WifiSsidSignal),
            "uptime" => Ok(Self::Uptime),
            "load_avg" => Ok(Self::LoadAverage),
            "top_process" => Ok(Self::TopProcess),
            "system_updates" => Ok(Self::SystemUpdates),
            "server_check" => Ok(Self::ServerCheck),
            _ => Err(MetricParseError {
                value: value.to_owned(),
            }),
        }
    }
}

/// An invalid metric token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetricParseError {
    /// The rejected metric token.
    pub value: String,
}

impl Display for MetricParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "unknown metric: {}", self.value)
    }
}

impl std::error::Error for MetricParseError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::form::Surface;

    #[test]
    fn cpu_usage_keeps_historied_forms() {
        let spec = Metric::CpuUsage.spec();

        assert!(spec.generic_forms.contains(&Form::Bar));
        assert!(spec.generic_forms.contains(&Form::SparkValue));
        assert!(!spec.generic_forms.contains(&Form::Pair));
    }

    #[test]
    fn tooltip_only_metric_stays_out_of_panel() {
        let spec = Metric::Uptime.spec();

        assert!(spec.surfaces.contains(Surface::Tooltip));
        assert!(!spec.surfaces.contains(Surface::PanelHorizontal));
        assert!(!spec.surfaces.contains(Surface::PanelVertical));
    }

    #[test]
    fn intrinsic_metrics_keep_shapes() {
        assert_eq!(Metric::NetSpeed.intrinsic_shape(), Some(Shape::Duo));
        assert_eq!(Metric::TopProcess.intrinsic_shape(), Some(Shape::TripleL));
        assert!(!Metric::NetSpeed.supports_form(Form::Value));
    }

    #[test]
    fn parses_known_metric_tokens() {
        assert_eq!("cpu_usage".parse::<Metric>(), Ok(Metric::CpuUsage));
        assert_eq!("server_check".parse::<Metric>(), Ok(Metric::ServerCheck));
    }
}
