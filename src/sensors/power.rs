//! UPower/UDisks2 battery and SMART readings.
//!
//! Ports the battery boundaries and UDisks2 SMART boundary from `src/sensors.py`:
//!
//! - [`upower_enumerate`] / [`find_battery_sys`] discover UPower device object
//!   paths (replaces the `upower -e` subprocess).
//! - [`detect_smart_disks`] enumerates SMART-capable whole disks via UDisks2
//!   `GetManagedObjects` (replaces the sysfs-only [`crate::sensors::disk::
//!   detect_disks`] which exposes label/kind/rotational but not the drive
//!   object path or SMART interface).
//! - `read_disk_smart` queries ATA/NVMe SMART health via UDisks2 `SmartUpdate` + `Properties.Get`.
//! - [`read_battery_sys_once`] reads system batteries via sysfs first, falling back
//!   to UPower when `/sys/class/power_supply` is unavailable for a battery.
//! - [`read_battery_periph_once`] reads a peripheral battery via UPower properties.
//! - [`read_battery_bolt_once`] reads a Logitech Bolt receiver battery through
//!   [`BoltBatteryFacade`]; `sensors::hid` provides production hidraw I/O.
//!
//! All D-Bus work flows through the shared [`DbusFacade`] trait, and every sysfs
//! read takes an explicit sys root so tests never touch the host filesystem.
//! The body encoding of each D-Bus reply is documented on the helper that
//! consumes it; the production `busctl` adapter translates JSON replies into
//! the same `Vec<String>` shapes.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::domain::boundary::{
    BoundaryError, BusKind, ClockSnapshot, DbusArgument, DbusFacade, DbusRequest,
};
use crate::domain::metric::Capability;
use crate::domain::readings::{
    BatteryPeripheralReading, BatteryState, BatterySystemReading, DiskSmartInterface,
    HardwareInventory, SmartDisk,
};

use super::disk::is_rotational;

// ── D-Bus identity constants ─────────────────────────────────────────────────

/// `org.freedesktop.UPower` service and interface name.
const UPOWER_NAME: &str = "org.freedesktop.UPower";
/// `/org/freedesktop/UPower` well-known object path.
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
/// `org.freedesktop.UPower` manager interface (EnumerateDevices lives here).
const UPOWER_IFACE: &str = "org.freedesktop.UPower";
/// `org.freedesktop.UPower.Device` per-device property interface.
const UPOWER_DEV_IFACE: &str = "org.freedesktop.UPower.Device";

/// `org.freedesktop.UDisks2` service name.
const UDISKS_NAME: &str = "org.freedesktop.UDisks2";
/// `/org/freedesktop/UDisks2` manager object path.
const UDISKS_PATH: &str = "/org/freedesktop/UDisks2";
/// `org.freedesktop.DBus.ObjectManager` interface (GetManagedObjects).
const OBJ_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";
/// `org.freedesktop.UDisks2.Block` interface name.
const UDISKS_BLOCK: &str = "org.freedesktop.UDisks2.Block";
/// `org.freedesktop.UDisks2.Partition` interface name.
const UDISKS_PARTITION: &str = "org.freedesktop.UDisks2.Partition";
/// `org.freedesktop.UDisks2.NVMe.Controller` interface name.
const UDISKS_NVME: &str = "org.freedesktop.UDisks2.NVMe.Controller";
/// `org.freedesktop.UDisks2.Drive.Ata` interface name.
const UDISKS_ATA: &str = "org.freedesktop.UDisks2.Drive.Ata";

/// Prefix used to encode the `Block.Drive` property inside a GetManagedObjects
/// object chunk (avoids a second map layer in the flat body).
const BLOCK_DRIVE_PREFIX: &str = "Block.Drive=";

// ── Compatibility freshness budgets ─────────────────────────────────────────

/// System-battery sysfs/UPower freshness budget.
pub(super) const BAT_CACHE_TTL: Duration = Duration::from_secs(30);
/// Peripheral-battery UPower freshness budget.
pub(super) const PERIPH_CACHE_TTL: Duration = Duration::from_secs(30);
/// Bolt-receiver freshness budget; a keyboard's charge changes over days, and every Bolt query wakes the device from deep sleep (~900ms round-trip), so frequent sampling buys nothing and needlessly drains the keyboard's own battery.
pub(super) const BOLT_CACHE_TTL: Duration = Duration::from_secs(3600);
/// `SmartUpdate` is a real ioctl on the drive (slow on ATA).
const SMART_UPDATE_TIMEOUT: Duration = Duration::from_millis(15_000);

/// 1×10⁶ microwatts per watt, matching `/sys/class/power_supply/.../power_now`.
const MICROWATTS_PER_WATT: u128 = 1_000_000;

/// Cached system-battery reading retained by the power owner.
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
    /// Monotonic instant of the latest attempt, successful or not.
    pub attempted_at: Option<Duration>,
    /// Monotonic instant of the latest failed attempt, cleared by success.
    pub failed_at: Option<Duration>,
}

/// Cached peripheral-battery reading retained by the power owner.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BatteryPeripheralCache {
    /// Human-readable peripheral model name.
    pub name: String,
    /// Visible charge percentage.
    pub charge_percent: Option<i32>,
    /// Monotonic instant of the cached sample.
    pub sampled_at: Option<Duration>,
    /// Monotonic instant of the latest attempt, successful or not.
    pub attempted_at: Option<Duration>,
    /// Monotonic instant of the latest failed attempt, cleared by a completed query.
    pub failed_at: Option<Duration>,
    /// Monotonic instant of the latest completed Bolt query, including a confirmed unsupported battery feature.
    pub bolt_completed_at: Option<Duration>,
    /// Hardware/config identity that produced the retained sample.
    pub source: Option<String>,
}

/// Mutable cache state owned by power-domain reads.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PowerState {
    /// Cached system batteries keyed by stable id.
    pub battery_sys_cache: BTreeMap<String, BatterySystemCache>,
    /// Cached mouse battery.
    pub battery_mouse_cache: BatteryPeripheralCache,
    /// Cached keyboard battery.
    pub battery_kbd_cache: BatteryPeripheralCache,
}

impl PowerState {
    pub(crate) fn reconcile_sources(
        &mut self,
        hw: &HardwareInventory,
        cfg: &crate::config::Config,
        capabilities: &std::collections::BTreeSet<Capability>,
    ) {
        if capabilities.contains(&Capability::BatterySystem) {
            self.battery_sys_cache
                .retain(|id, _| hw.battery_sys_ids.contains(id));
        } else {
            self.battery_sys_cache.clear();
        }

        reconcile_peripheral_source(
            &mut self.battery_mouse_cache,
            capabilities.contains(&Capability::BatteryMouse),
            hw.battery_mouse_id.as_deref(),
            cfg.battery.mouse_bolt,
        );
        reconcile_peripheral_source(
            &mut self.battery_kbd_cache,
            capabilities.contains(&Capability::BatteryKeyboard),
            hw.battery_kbd_id.as_deref(),
            cfg.battery.kbd_bolt,
        );
    }
}

fn reconcile_peripheral_source(
    cache: &mut BatteryPeripheralCache,
    demanded: bool,
    upower_id: Option<&str>,
    bolt_index: Option<i32>,
) {
    let source = if demanded {
        upower_id
            .map(|id| format!("upower:{id}"))
            .or_else(|| bolt_index.map(|index| format!("bolt:{index}")))
    } else {
        None
    };
    if cache.source != source {
        *cache = BatteryPeripheralCache {
            source,
            ..BatteryPeripheralCache::default()
        };
    }
}

/// Decoded UPower device properties retained for battery reads.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UpowerDeviceProps {
    /// `Percentage` (0–100) when reported.
    pub(crate) percentage: Option<f64>,
    /// `State` enum (1=charging, 2=discharging, 4=fully-charged).
    pub(crate) state: Option<i64>,
    /// `EnergyRate` in watts when reported.
    pub(crate) energy_rate: Option<f64>,
    /// `Model` device name when reported.
    pub(crate) model: Option<String>,
    /// `Type` enum (5=mouse, 6=keyboard).
    pub(crate) kind: Option<i64>,
}

impl UpowerDeviceProps {
    /// Returns the battery state token for a UPower `State` value, matching
    /// Python's `_UPOWER_STATE_MAP` (unknown states map to `BatteryState::
    /// Unknown`).
    const fn state_from_value(state: Option<i64>) -> BatteryState {
        match state {
            Some(1) => BatteryState::Charging,
            Some(2) => BatteryState::Discharging,
            Some(4) => BatteryState::FullyCharged,
            _ => BatteryState::Unknown,
        }
    }
}

/// One managed object decoded from UDisks2 `GetManagedObjects`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ManagedObject {
    /// Object path.
    path: String,
    /// Whether the object exposes `org.freedesktop.UDisks2.Block`.
    is_block: bool,
    /// Whether the object exposes `org.freedesktop.UDisks2.Partition`.
    is_partition: bool,
    /// `Block.Drive` property when the object is a block device.
    drive_path: Option<String>,
    /// Whether the referenced drive exposes NVMe SMART.
    has_nvme: bool,
    /// Whether the referenced drive exposes ATA SMART.
    has_ata: bool,
}

// ── Body-decoding helpers ────────────────────────────────────────────────────

/// Decodes a flat `[path1, path2, ...]` body into owned paths (UPower
/// `EnumerateDevices` reply shape).
fn parse_object_paths(body: &[String]) -> Vec<String> {
    body.iter().filter(|s| !s.is_empty()).cloned().collect()
}

/// Decodes an interleaved `[key, val, key, val, ...]` body into a map. Stray
/// keys without a value are ignored.
fn parse_property_map(body: &[String]) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut iter = body.iter();
    while let Some(key) = iter.next() {
        if key.is_empty() {
            continue;
        }
        let Some(value) = iter.next() else {
            break;
        };
        map.insert(key.clone(), value.clone());
    }
    map
}

/// Decodes a GetManagedObjects body where each object is one chunk separated
/// from the next by an empty string. The first element of each chunk is the
/// object path; remaining elements are interface names, plus a special
/// `Block.Drive=<path>` entry encoding the Block interface's Drive property.
fn parse_managed_objects(body: &[String]) -> Vec<ManagedObject> {
    body.split(|s| s.is_empty())
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| {
            let mut iter = chunk.iter();
            let path = iter.next().cloned().unwrap_or_default();
            let mut is_block = false;
            let mut is_partition = false;
            let mut drive_path = None;
            let mut has_nvme = false;
            let mut has_ata = false;
            for iface in iter {
                if iface == UDISKS_BLOCK {
                    is_block = true;
                } else if iface == UDISKS_PARTITION {
                    is_partition = true;
                } else if iface == UDISKS_NVME {
                    has_nvme = true;
                } else if iface == UDISKS_ATA {
                    has_ata = true;
                } else if let Some(drive) = iface.strip_prefix(BLOCK_DRIVE_PREFIX) {
                    drive_path = Some(drive.to_owned());
                }
            }
            ManagedObject {
                path,
                is_block,
                is_partition,
                drive_path,
                has_nvme,
                has_ata,
            }
        })
        .collect()
}

/// Dispatches a D-Bus call through `dbus` and unwraps the body on success,
/// returning `None` on any boundary failure. Mirrors Python's blanket
/// `except Exception: return None` around every GDBus call.
fn dbus_call(
    dbus: &mut dyn DbusFacade,
    service: &str,
    path: &str,
    iface: &str,
    member: &str,
    arguments: Vec<DbusArgument>,
    timeout: Option<Duration>,
) -> Option<Vec<String>> {
    dbus_call_result(dbus, service, path, iface, member, arguments, timeout).ok()
}

fn dbus_call_result(
    dbus: &mut dyn DbusFacade,
    service: &str,
    path: &str,
    iface: &str,
    member: &str,
    arguments: Vec<DbusArgument>,
    timeout: Option<Duration>,
) -> Result<Vec<String>, BoundaryError> {
    dbus.call(DbusRequest {
        bus: BusKind::System,
        service: service.to_owned(),
        object_path: path.to_owned(),
        interface: iface.to_owned(),
        member: member.to_owned(),
        arguments,
        timeout,
    })
    .map(|output| output.body)
}

// ── UPower device discovery ──────────────────────────────────────────────────

/// Enumerates UPower device object paths (replaces `upower -e`).
///
/// Issues `EnumerateDevices` on the system bus and decodes the
/// `[path1, path2, ...]` reply body.
///
/// # Errors
///
/// Returns the D-Bus boundary failure so inventory owners do not confuse an
/// unavailable service with a successful empty enumeration.
pub fn upower_enumerate(dbus: &mut dyn DbusFacade) -> Result<Vec<String>, BoundaryError> {
    let body = dbus_call_result(
        dbus,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        Vec::new(),
        None,
    )?;
    Ok(parse_object_paths(&body))
}

/// Reads the requested UPower device properties for one object path.
///
/// Models Python's per-device `Gio.DBusProxy.new_sync` + `get_cached_property`
/// round-trip as one `GetAll` call; the reply body is the interleaved
/// `[key, val, ...]` of all device properties, from which this function picks
/// `Percentage`/`State`/`EnergyRate`/`Model`/`Type`. Returns `None` on any
/// D-Bus failure, matching Python's blanket exception handler.
pub(crate) fn upower_device_props(
    dbus: &mut dyn DbusFacade,
    path: &str,
) -> Option<UpowerDeviceProps> {
    upower_device_props_result(dbus, path).ok()
}

pub(crate) fn upower_device_props_result(
    dbus: &mut dyn DbusFacade,
    path: &str,
) -> Result<UpowerDeviceProps, BoundaryError> {
    let body = dbus_call_result(
        dbus,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        vec![DbusArgument::String(UPOWER_DEV_IFACE.to_owned())],
        None,
    )?;
    let map = parse_property_map(&body);
    Ok(UpowerDeviceProps {
        percentage: map
            .get("Percentage")
            .and_then(|v| v.trim().parse::<f64>().ok()),
        state: map.get("State").and_then(|v| v.trim().parse::<i64>().ok()),
        energy_rate: map
            .get("EnergyRate")
            .and_then(|v| v.trim().parse::<f64>().ok()),
        model: map.get("Model").cloned(),
        kind: map.get("Type").and_then(|v| v.trim().parse::<i64>().ok()),
    })
}

/// Returns sorted UPower object paths containing `/battery_BAT`.
pub fn find_battery_sys(dbus: &mut dyn DbusFacade) -> Result<Vec<String>, BoundaryError> {
    let mut paths: Vec<String> = upower_enumerate(dbus)?
        .into_iter()
        .filter(|p| p.contains("/battery_BAT"))
        .collect();
    paths.sort();
    Ok(paths)
}

// ── UDisks2 SMART discovery ──────────────────────────────────────────────────

/// Discovers SMART-capable whole disks via UDisks2 `GetManagedObjects`.
///
/// Walks the decoded object set the same way Python's `_detect_disks` does:
///
/// 1. keep only objects whose path contains `/block_devices/`;
/// 2. skip partitions and objects without a `Block` interface;
/// 3. skip block devices with an empty/root Drive reference;
/// 4. skip when the referenced drive object is absent from the reply;
/// 5. skip optical drives (`sr*` labels);
/// 6. record `(drive_path, "nvme"|"ata", rotational)` for drives that expose
///    either SMART interface.
///
/// The rotational flag is read from sysfs (`queue/rotational`), matching
/// Python's `_is_rotational` call.
pub fn detect_smart_disks(
    dbus: &mut dyn DbusFacade,
    sys_root: &Path,
) -> Result<BTreeMap<String, SmartDisk>, BoundaryError> {
    let mut result = BTreeMap::new();
    let body = dbus_call_result(
        dbus,
        UDISKS_NAME,
        UDISKS_PATH,
        OBJ_MANAGER_IFACE,
        "GetManagedObjects",
        Vec::new(),
        None,
    )?;
    let objects = parse_managed_objects(&body);

    for obj in &objects {
        if !obj.path.contains("/block_devices/") {
            continue;
        }
        if !obj.is_block || obj.is_partition {
            continue;
        }
        let Some(drive_path) = obj.drive_path.as_deref() else {
            continue;
        };
        if drive_path.is_empty() || drive_path == "/" {
            continue;
        }
        let Some(drive) = objects
            .iter()
            .find(|candidate| candidate.path == drive_path)
        else {
            continue;
        };
        let label = obj.path.rsplit('/').next().unwrap_or_default();
        if label.is_empty() || label.starts_with("sr") {
            continue;
        }
        let rotational = is_rotational(sys_root, label);
        let interface = if drive.has_nvme {
            DiskSmartInterface::Nvme
        } else if drive.has_ata {
            DiskSmartInterface::Ata
        } else {
            continue;
        };
        result.insert(
            label.to_owned(),
            SmartDisk {
                object_path: drive_path.to_owned(),
                interface,
                rotational,
            },
        );
    }
    Ok(result)
}

// ── Disk SMART health ────────────────────────────────────────────────────────

/// Reads one UDisks2 drive property via `Properties.Get`, returning the raw
/// decoded variant as a string. The body is a single-element `[value]`.
///
/// Mirrors Python's `_udisks_prop`: a fresh `Properties.Get` (not a proxy
/// cache) so `SmartUpdate`'s new value is visible immediately.
fn udisks_get(
    dbus: &mut dyn DbusFacade,
    drive_path: &str,
    iface: &str,
    prop: &str,
) -> Option<String> {
    let body = dbus_call(
        dbus,
        UDISKS_NAME,
        drive_path,
        "org.freedesktop.DBus.Properties",
        "Get",
        vec![
            DbusArgument::String(iface.to_owned()),
            DbusArgument::String(prop.to_owned()),
        ],
        None,
    )?;
    body.into_iter().next()
}

/// Reads SMART health for one drive.
///
/// Returns `Some(true)` = healthy, `Some(false)` = failing, `None` = D-Bus call failed or unsupported. NVMe reads `SmartCriticalWarning` (healthy iff the decoded warning array is empty); ATA reads `SmartFailing` (healthy iff false). A `SmartUpdate` ioctl is triggered first so the values are current.
pub(super) fn read_disk_smart(
    dbus: &mut dyn DbusFacade,
    drive_path: &str,
    kind: DiskSmartInterface,
) -> Option<bool> {
    let iface = kind.as_str_api();
    // SmartUpdate(options a{sv}) — empty options. This is a real ioctl on the
    // drive (slow on ATA), hence the long TTL upstream. Failure here means we
    // cannot trust any cached property, so return None.
    dbus.call(DbusRequest {
        bus: BusKind::System,
        service: UDISKS_NAME.to_owned(),
        object_path: drive_path.to_owned(),
        interface: iface.to_owned(),
        member: "SmartUpdate".to_owned(),
        arguments: vec![DbusArgument::EmptyStringVariantDict],
        timeout: Some(SMART_UPDATE_TIMEOUT),
    })
    .ok()?;

    match kind {
        DiskSmartInterface::Nvme => {
            let warning = udisks_get(dbus, drive_path, iface, "SmartCriticalWarning")?;
            nvme_warning_is_empty(&warning)
        }
        DiskSmartInterface::Ata => {
            let raw = udisks_get(dbus, drive_path, iface, "SmartFailing")?;
            let failing = parse_bool(&raw)?;
            Some(!failing)
        }
    }
}

fn nvme_warning_is_empty(warning: &str) -> Option<bool> {
    if warning.is_empty() {
        return Some(true);
    }
    if !warning.starts_with('[') {
        return Some(false);
    }
    serde_json::from_str::<Vec<String>>(warning)
        .ok()
        .map(|warnings| warnings.is_empty())
}

// ── System battery (sysfs with UPower fallback) ──────────────────────────────

/// Extracts the power_supply name from a UPower battery id (e.g.
/// `/org/freedesktop/UPower/devices/battery_BAT0` → `BAT0`).
fn bat_name_from_id(bat_id: &str) -> &str {
    bat_id.rsplit("battery_").next().unwrap_or(bat_id)
}

/// `/sys/class/power_supply/<name>/power_now` in microwatts → rounded watts.
///
/// Uses banker's rounding to match Python's `round(uw / 1_000_000)`. Returns 0
/// on any read/parse failure (Python's `_sysfs_bat_rate` catches `OSError`).
fn sysfs_bat_rate(sys_root: &Path, bat_id: &str) -> i32 {
    let name = bat_name_from_id(bat_id);
    let path = sys_root
        .join("class")
        .join("power_supply")
        .join(name)
        .join("power_now");
    let Ok(text) = fs::read_to_string(&path) else {
        return 0;
    };
    let Ok(microwatts) = text.trim().parse::<u128>() else {
        return 0;
    };
    round_half_even_ratio(microwatts, MICROWATTS_PER_WATT) as i32
}

/// `/sys/class/power_supply/<name>/charge_control_end_threshold`, or `None`
/// when the file is absent or reports 100 (no meaningful limit).
fn sysfs_bat_charge_limit(sys_root: &Path, bat_id: &str) -> Option<i32> {
    let name = bat_name_from_id(bat_id);
    let path = sys_root
        .join("class")
        .join("power_supply")
        .join(name)
        .join("charge_control_end_threshold");
    let limit = fs::read_to_string(&path).ok()?.trim().parse::<i32>().ok()?;
    (limit < 100).then_some(limit)
}

/// Reads `(capacity%, rate_watts, state)` directly from sysfs
/// (`capacity`/`status`/`power_now`). Returns `None` if sysfs is unavailable
/// for this battery (mirrors Python's `OSError` path that triggers the UPower
/// fallback in `_read_battery_sys`).
fn sysfs_bat_read(sys_root: &Path, bat_id: &str) -> Option<(i32, i32, BatteryState)> {
    let name = bat_name_from_id(bat_id);
    let base = sys_root.join("class").join("power_supply").join(name);
    let capacity = fs::read_to_string(base.join("capacity"))
        .ok()?
        .trim()
        .parse::<i32>()
        .ok()?;
    let status = fs::read_to_string(base.join("status"))
        .ok()?
        .trim()
        .to_owned();
    let state = sysfs_status_to_state(&status);
    let rate = if matches!(state, BatteryState::Charging | BatteryState::Discharging) {
        sysfs_bat_rate(sys_root, bat_id)
    } else {
        0
    };
    Some((capacity, rate, state))
}

/// Maps a sysfs status string to a [`BatteryState`], matching Python's
/// `_SYSFS_BAT_STATUS_MAP` (anything else, e.g. `Not charging` at a charge
/// limit, maps to [`BatteryState::Unknown`] — the charge-limit/100% check in
/// the formatter already covers that case).
fn sysfs_status_to_state(status: &str) -> BatteryState {
    match status {
        "Full" => BatteryState::FullyCharged,
        "Charging" => BatteryState::Charging,
        "Discharging" => BatteryState::Discharging,
        _ => BatteryState::Unknown,
    }
}

/// Performs one system-battery source attempt.
pub(super) fn refresh_battery_sys(
    cache: &mut BatterySystemCache,
    dbus: &mut dyn DbusFacade,
    battery_id: &str,
    sys_root: &Path,
    clock: ClockSnapshot,
) -> bool {
    let succeeded = refresh_battery_sys_cache(cache, battery_id, dbus, sys_root, clock.monotonic);
    cache.failed_at = (!succeeded).then_some(clock.monotonic);
    succeeded
}

/// Performs one source attempt for every requested system battery.
#[must_use]
pub fn read_battery_sys_once(
    state: &mut PowerState,
    dbus: &mut dyn DbusFacade,
    battery_ids: &[String],
    sys_root: &Path,
    clock: ClockSnapshot,
) -> Vec<BatterySystemReading> {
    let mut readings = Vec::new();
    for id in battery_ids {
        let cache = state.battery_sys_cache.entry(id.clone()).or_default();
        let _ = refresh_battery_sys(cache, dbus, id, sys_root, clock);
        if let Some(reading) = battery_sys_from_cache(id, cache) {
            readings.push(reading);
        }
    }
    readings
}

/// Converts one retained system-battery cache into its display reading.
pub(super) fn battery_sys_from_cache(
    battery_id: &str,
    cache: &BatterySystemCache,
) -> Option<BatterySystemReading> {
    Some(BatterySystemReading {
        id: battery_id.to_owned(),
        charge_percent: cache.charge_percent?,
        rate_watts: cache.rate_watts,
        state: cache.state,
        charge_limit_percent: cache.charge_limit_percent,
    })
}

/// Refreshes one system-battery cache entry: sysfs first, UPower on failure.
fn refresh_battery_sys_cache(
    cache: &mut BatterySystemCache,
    bat_id: &str,
    dbus: &mut dyn DbusFacade,
    sys_root: &Path,
    now: Duration,
) -> bool {
    cache.attempted_at = Some(now);
    if let Some((capacity, rate, state)) = sysfs_bat_read(sys_root, bat_id) {
        cache.charge_percent = Some(capacity);
        cache.rate_watts = rate;
        cache.state = state;
        cache.charge_limit_percent = sysfs_bat_charge_limit(sys_root, bat_id);
        cache.sampled_at = Some(now);
        return true;
    }
    // sysfs unavailable: fall back to UPower over GDBus.
    let Some(props) = upower_device_props(dbus, bat_id) else {
        return false;
    };
    if props.percentage.is_none() {
        return false;
    }
    let percentage = props.percentage.unwrap_or(0.0);
    cache.charge_percent = Some(percentage as i32);
    cache.state = UpowerDeviceProps::state_from_value(props.state);
    let mut rate = round_half_even_f64(props.energy_rate.unwrap_or(0.0));
    if rate == 0
        && matches!(
            cache.state,
            BatteryState::Charging | BatteryState::Discharging
        )
    {
        rate = sysfs_bat_rate(sys_root, bat_id);
    }
    cache.rate_watts = rate;
    cache.charge_limit_percent = sysfs_bat_charge_limit(sys_root, bat_id);
    cache.sampled_at = Some(now);
    true
}

// ── Peripheral battery (UPower) ──────────────────────────────────────────────

/// Performs one peripheral-battery UPower attempt.
#[must_use]
pub fn read_battery_periph_once(
    cache: &mut BatteryPeripheralCache,
    dbus: &mut dyn DbusFacade,
    upower_path: &str,
    name_override: Option<&str>,
    clock: ClockSnapshot,
) -> Option<BatteryPeripheralReading> {
    attempt_battery_periph_once(cache, dbus, upower_path, name_override, clock).0
}

pub(super) fn attempt_battery_periph_once(
    cache: &mut BatteryPeripheralCache,
    dbus: &mut dyn DbusFacade,
    upower_path: &str,
    name_override: Option<&str>,
    clock: ClockSnapshot,
) -> (Option<BatteryPeripheralReading>, bool) {
    let now = clock.monotonic;
    let succeeded = refresh_periph_cache(cache, dbus, upower_path, now);
    (battery_periph_from_cache(cache, name_override), succeeded)
}

/// Converts retained peripheral state into its display reading.
#[must_use]
pub(super) fn battery_periph_from_cache(
    cache: &BatteryPeripheralCache,
    name_override: Option<&str>,
) -> Option<BatteryPeripheralReading> {
    let charge = cache.charge_percent?;
    Some(BatteryPeripheralReading {
        name: name_override
            .map(String::from)
            .unwrap_or_else(|| cache.name.clone()),
        charge_percent: charge,
    })
}

/// Updates `cache` with a fresh UPower property read for one peripheral.
fn refresh_periph_cache(
    cache: &mut BatteryPeripheralCache,
    dbus: &mut dyn DbusFacade,
    upower_path: &str,
    now: Duration,
) -> bool {
    let props = upower_device_props(dbus, upower_path);
    cache.attempted_at = Some(now);
    let Some(props) = props else {
        cache.failed_at = Some(now);
        return false;
    };
    if cache.name.is_empty() {
        if let Some(model) = props.model.as_deref() {
            if !model.is_empty() {
                cache.name = model.to_owned();
            }
        }
    }
    // 0% (or missing) = device disconnected: leave charge None so it disappears
    // from the tooltip (matches Python's `f"{int(pct)}%" if pct else ""`).
    cache.charge_percent = props
        .percentage
        .filter(|&pct| pct > 0.0)
        .map(|pct| pct as i32);
    cache.sampled_at = Some(now);
    cache.failed_at = None;
    true
}

// ── Bolt receiver battery ────────────────────────────────────────────────────

/// Result of a successful Bolt HID++ battery query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoltBattery {
    /// Device name (empty when not requested).
    pub name: String,
    /// Battery level percentage (0–100).
    pub level: u8,
}

/// Facade for the Logitech Bolt HID++ battery query.
///
/// `sensors::hid` owns production hidraw I/O; this module owns cache and retry
/// semantics in [`read_battery_bolt_once`]. Tests use a trivial fake. Mirrors
/// Python's `_bolt_query(dev_idx, want_name)` contract.
pub trait BoltBatteryFacade {
    /// Queries the Bolt receiver at `dev_idx`, optionally fetching the device
    /// name. Returns `Ok(None)` only when HID++ confirms that the battery
    /// feature is unsupported. Transport and no-response outcomes return
    /// `Err` so cadence owners can retry them.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] when the adapter cannot reach the device.
    fn query(
        &mut self,
        dev_idx: i32,
        want_name: bool,
    ) -> Result<Option<BoltBattery>, BoundaryError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoltAttemptOutcome {
    Captured,
    Unsupported,
    Failed,
}

/// Performs one peripheral-battery Bolt attempt.
///
/// The device name is fetched only until cached because it costs substantially more than the battery read. `Ok(None)` confirms an unsupported battery feature, clears any retained reading, and completes the normal Bolt cadence.
#[must_use]
pub fn read_battery_bolt_once(
    cache: &mut BatteryPeripheralCache,
    bolt: &mut dyn BoltBatteryFacade,
    dev_idx: i32,
    name_override: Option<&str>,
    clock: ClockSnapshot,
) -> Option<BatteryPeripheralReading> {
    attempt_battery_bolt_once(cache, bolt, dev_idx, name_override, clock).0
}

pub(super) fn attempt_battery_bolt_once(
    cache: &mut BatteryPeripheralCache,
    bolt: &mut dyn BoltBatteryFacade,
    dev_idx: i32,
    name_override: Option<&str>,
    clock: ClockSnapshot,
) -> (Option<BatteryPeripheralReading>, BoltAttemptOutcome) {
    let now = clock.monotonic;
    cache.attempted_at = Some(now);
    let want_name = name_override.is_none() && cache.name.is_empty();
    match bolt.query(dev_idx, want_name) {
        Ok(Some(battery)) => {
            cache.name = name_override
                .map(String::from)
                .or_else(|| (!battery.name.is_empty()).then_some(battery.name))
                .unwrap_or_else(|| cache.name.clone());
            cache.charge_percent = Some(i32::from(battery.level));
            cache.sampled_at = Some(now);
            cache.bolt_completed_at = Some(now);
            cache.failed_at = None;
        }
        Ok(None) => {
            cache.charge_percent = None;
            cache.sampled_at = None;
            cache.bolt_completed_at = Some(now);
            cache.failed_at = None;
            return (
                battery_periph_from_cache(cache, name_override),
                BoltAttemptOutcome::Unsupported,
            );
        }
        Err(_) => {
            cache.failed_at = Some(now);
            return (
                battery_periph_from_cache(cache, name_override),
                BoltAttemptOutcome::Failed,
            );
        }
    }
    (
        battery_periph_from_cache(cache, name_override),
        BoltAttemptOutcome::Captured,
    )
}

// ── Small numeric helpers ────────────────────────────────────────────────────

impl DiskSmartInterface {
    /// Returns the UDisks2 drive interface name for this SMART family.
    const fn as_str_api(self) -> &'static str {
        match self {
            Self::Ata => UDISKS_ATA,
            Self::Nvme => UDISKS_NVME,
        }
    }
}

/// Parses a loose boolean string (`"true"`/`"false"`, case-insensitive).
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// Banker's rounding of `numerator / denominator` for non-negative integers,
/// matching Python 3's `round()` on the equivalent float division without the
/// precision loss. Local duplication keeps power rounding independent of
/// sensor-specific private helpers.
fn round_half_even_ratio(numerator: u128, denominator: u128) -> u128 {
    if denominator == 0 {
        return 0;
    }
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let doubled = remainder.saturating_mul(2);
    if doubled > denominator {
        quotient.saturating_add(1)
    } else if doubled < denominator || quotient % 2 == 0 {
        quotient
    } else {
        quotient.saturating_add(1)
    }
}

/// Banker's rounding of an `f64` to `i32`, matching Python 3's `round()` on a
/// float. Non-negative inputs only (battery rates are never negative); `NaN`/
/// infinite values return 0.
fn round_half_even_f64(value: f64) -> i32 {
    if !value.is_finite() || value.is_sign_negative() {
        return 0;
    }
    let floor = value.floor();
    let frac = value - floor;
    let floor_int = if floor >= i32::MAX as f64 {
        i32::MAX
    } else {
        floor as i32
    };
    if frac < 0.5 {
        floor_int
    } else if frac > 0.5 {
        floor_int.saturating_add(1)
    } else {
        // Exactly halfway: round to even.
        if floor_int % 2 == 0 {
            floor_int
        } else {
            floor_int.saturating_add(1)
        }
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests;
