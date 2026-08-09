//! Mutable state shared outside metric owners.

use std::collections::BTreeMap;
use std::time::Duration;

/// Edge and hold state for one sustained notification.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NotificationLatch {
    /// Whether the current threshold episode has already emitted.
    pub active: bool,
    /// Monotonic instant when the value first reached the trip point.
    pub since: Option<Duration>,
}

/// Cross-poll notification latches retained separately from metric-owner state.
///
/// Device-keyed entries intentionally remain when a device disappears so a later reading for the same stable id resumes its previous episode.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NotificationState {
    /// Disk-usage alert state keyed by mountpoint.
    pub disk: BTreeMap<String, bool>,
    /// SMART-failure alert state keyed by disk label.
    pub disk_smart: BTreeMap<String, bool>,
    /// System-battery alert state keyed by stable battery id.
    pub battery_sys: BTreeMap<String, bool>,
    /// Mouse-battery alert state.
    pub battery_mouse: bool,
    /// Keyboard-battery alert state.
    pub battery_kbd: bool,
    /// Server-down alert state.
    pub server: bool,
    /// CPU-temperature sustained-alert state.
    pub cpu_temp: NotificationLatch,
    /// NVIDIA-temperature sustained-alert state.
    pub gpu_nvidia_temp: NotificationLatch,
    /// Disk-temperature sustained-alert state keyed by disk label.
    pub hd_temp: BTreeMap<String, NotificationLatch>,
    /// Fifteen-minute load sustained-alert state.
    pub load_avg: NotificationLatch,
}
