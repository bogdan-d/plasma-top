//! Typed mutable daemon-state contracts.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::domain::boundary::ClockSnapshot;
use crate::domain::readings::{BatteryState, TopProcessDetails};

/// Cached value with the monotonic instant at which it was sampled.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TimedValue<T> {
    /// Cached value, or `None` when the latest refresh failed.
    pub value: Option<T>,
    /// Monotonic instant of the cached sample.
    pub sampled_at: Option<Duration>,
}

/// Cached system-battery reading retained between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatterySystemCache {
    /// Visible charge percentage.
    pub charge_percent: Option<i32>,
    /// Rounded power rate in watts.
    pub rate_watts: i32,
    /// Current charging state.
    pub state: BatteryState,
    /// Configured charge limit when known.
    pub charge_limit_percent: Option<i32>,
    /// Monotonic instant of the cached sample.
    pub sampled_at: Option<Duration>,
}

/// Cached peripheral-battery reading retained between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatteryPeripheralCache {
    /// Human-readable peripheral model name.
    pub name: String,
    /// Visible charge percentage.
    pub charge_percent: Option<i32>,
    /// Monotonic instant of the cached sample.
    pub sampled_at: Option<Duration>,
}

/// Cached network identity retained between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NetworkInfoCache {
    /// Active route device.
    pub device: Option<String>,
    /// Active route IP address.
    pub ip_address: Option<String>,
    /// Active wifi SSID.
    pub ssid: Option<String>,
    /// Active wifi signal quality percentage.
    pub signal_percent: Option<i32>,
    /// Monotonic instant of the cached sample.
    pub sampled_at: Option<Duration>,
}

/// Previous cumulative counters retained to compute byte-rate deltas.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CounterRateState {
    /// Previous `a` counter value.
    pub prev_a: u64,
    /// Previous `b` counter value.
    pub prev_b: u64,
    /// Monotonic instant of the previous sample.
    pub sampled_at: Option<Duration>,
}

/// Cached GPU snapshot retained between polls.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GpuCache {
    /// GPU temperature in °C.
    pub temp_celsius: Option<i32>,
    /// GPU usage percentage.
    pub usage_percent: Option<i32>,
    /// GPU memory usage percentage.
    pub memory_percent: Option<i32>,
    /// GPU decoder usage percentage.
    pub decoder_percent: Option<i32>,
    /// GPU fan percentage.
    pub fan_percent: Option<i32>,
    /// Monotonic instant of the cached sample.
    pub sampled_at: Option<Duration>,
}

/// Aggregate mutable daemon state shared across collection polls.
#[derive(Debug, Clone, PartialEq)]
pub struct DaemonStateSnapshot {
    /// Previous aggregate `/proc/stat` counters.
    pub cpu_prev_times: Vec<u64>,
    /// Shared CPU history for panel/tooltip traces and graphs.
    pub cpu_history: Vec<i32>,
    /// Monotonic instant of the last CPU-history sample.
    pub cpu_history_sample_at: Option<Duration>,
    /// Shared memory history for panel/tooltip traces and graphs.
    pub mem_history: Vec<i32>,
    /// Monotonic instant of the last memory-history sample.
    pub mem_history_sample_at: Option<Duration>,
    /// Active GPU usage history for the graphs page.
    pub gpu_usage_history: Vec<i32>,
    /// Active GPU decoder history for the graphs page.
    pub gpu_dec_history: Vec<i32>,
    /// Monotonic instant of the last GPU-history sample.
    pub gpu_history_sample_at: Option<Duration>,
    /// Network upload history for the graphs page.
    pub net_up_history: Vec<u64>,
    /// Network download history for the graphs page.
    pub net_down_history: Vec<u64>,
    /// Monotonic instant of the last network-history sample.
    pub net_history_sample_at: Option<Duration>,
    /// Previous per-core `/proc/stat` counters.
    pub cpu_core_prev_times: Vec<Vec<u64>>,
    /// Per-core CPU histories.
    pub cpu_core_history: Vec<Vec<i32>>,
    /// Monotonic instant of the last per-core-history sample.
    pub cpu_core_history_sample_at: Option<Duration>,
    /// Previous process CPU totals keyed by pid.
    pub proc_prev_times: BTreeMap<u32, u64>,
    /// Monotonic instant of the previous process sample.
    pub proc_prev_sample_at: Option<Duration>,
    /// TTL-cached panel top-process rows.
    pub top_process_cache: Option<Vec<TopProcessDetails>>,
    /// Monotonic instant of the cached top-process sample.
    pub top_process_cache_sample_at: Option<Duration>,
    /// Page-local previous process CPU totals keyed by pid.
    pub page_proc_prev_times: BTreeMap<u32, u64>,
    /// Monotonic instant of the previous page-process sample.
    pub page_proc_prev_sample_at: Option<Duration>,
    /// Previous Intel DRM engine counters keyed by DRM client id.
    pub intel_gpu_engine_prev: BTreeMap<u32, BTreeMap<String, u64>>,
    /// Monotonic instant of the previous Intel GPU sample.
    pub intel_gpu_prev_sample_at: Option<Duration>,
    /// TTL-cached Intel GPU usage keyed by engine name.
    pub intel_gpu_usage_cache: BTreeMap<String, i32>,
    /// Monotonic instant of the cached Intel GPU usage sample.
    pub intel_gpu_usage_cache_sample_at: Option<Duration>,
    /// Previous network byte counters.
    pub net_rate: CounterRateState,
    /// Previous disk byte counters.
    pub disk_rate: CounterRateState,
    /// Cached system battery readings keyed by battery id.
    pub battery_sys_cache: BTreeMap<String, BatterySystemCache>,
    /// Cached mouse-battery reading.
    pub battery_mouse_cache: BatteryPeripheralCache,
    /// Cached keyboard-battery reading.
    pub battery_kbd_cache: BatteryPeripheralCache,
    /// Cached network identity reading.
    pub net_info_cache: NetworkInfoCache,
    /// Cached disk temperatures keyed by disk label.
    pub hd_temp_cache: BTreeMap<String, TimedValue<i32>>,
    /// Cached SMART health keyed by disk label.
    pub disk_smart_cache: BTreeMap<String, TimedValue<bool>>,
    /// Cached fan speeds keyed by fan label.
    pub fan_speed_cache: BTreeMap<String, TimedValue<i32>>,
    /// Cached GPU reading.
    pub gpu_cache: GpuCache,
    /// Active tooltip page index.
    pub active_page: usize,
    /// Number of published pages.
    pub page_count: usize,
    /// Wall/monotonic timestamp of the most recent successful poll.
    pub last_poll: Option<ClockSnapshot>,
}

impl Default for DaemonStateSnapshot {
    fn default() -> Self {
        Self {
            cpu_prev_times: Vec::new(),
            cpu_history: Vec::new(),
            cpu_history_sample_at: None,
            mem_history: Vec::new(),
            mem_history_sample_at: None,
            gpu_usage_history: Vec::new(),
            gpu_dec_history: Vec::new(),
            gpu_history_sample_at: None,
            net_up_history: Vec::new(),
            net_down_history: Vec::new(),
            net_history_sample_at: None,
            cpu_core_prev_times: Vec::new(),
            cpu_core_history: Vec::new(),
            cpu_core_history_sample_at: None,
            proc_prev_times: BTreeMap::new(),
            proc_prev_sample_at: None,
            top_process_cache: None,
            top_process_cache_sample_at: None,
            page_proc_prev_times: BTreeMap::new(),
            page_proc_prev_sample_at: None,
            intel_gpu_engine_prev: BTreeMap::new(),
            intel_gpu_prev_sample_at: None,
            intel_gpu_usage_cache: BTreeMap::new(),
            intel_gpu_usage_cache_sample_at: None,
            net_rate: CounterRateState::default(),
            disk_rate: CounterRateState::default(),
            battery_sys_cache: BTreeMap::new(),
            battery_mouse_cache: BatteryPeripheralCache::default(),
            battery_kbd_cache: BatteryPeripheralCache::default(),
            net_info_cache: NetworkInfoCache::default(),
            hd_temp_cache: BTreeMap::new(),
            disk_smart_cache: BTreeMap::new(),
            fan_speed_cache: BTreeMap::new(),
            gpu_cache: GpuCache::default(),
            active_page: 0,
            page_count: 1,
            last_poll: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_state_defaults_match_empty_python_state_shape() {
        let state = DaemonStateSnapshot::default();

        assert!(state.cpu_prev_times.is_empty());
        assert!(state.cpu_history.is_empty());
        assert!(state.proc_prev_times.is_empty());
        assert!(state.hd_temp_cache.is_empty());
        assert_eq!(state.active_page, 0);
        assert_eq!(state.page_count, 1);
        assert_eq!(state.last_poll, None);
    }

    #[test]
    fn timed_value_default_is_empty_and_unsampled() {
        let cached = TimedValue::<i32>::default();

        assert_eq!(cached.value, None);
        assert_eq!(cached.sampled_at, None);
    }

    #[test]
    fn battery_cache_defaults_are_unset() {
        let cache = BatterySystemCache::default();

        assert_eq!(cache.charge_percent, None);
        assert_eq!(cache.rate_watts, 0);
        assert_eq!(cache.state, BatteryState::Unknown);
        assert_eq!(cache.sampled_at, None);
    }
}
