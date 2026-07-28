//! UPower/UDisks2 battery and SMART readings.
//!
//! Ports the POWER-owned half of `src/sensors.py`:
//!
//! - [`upower_enumerate`] / [`find_battery_sys`] discover UPower device object
//!   paths (replaces the `upower -e` subprocess).
//! - [`detect_smart_disks`] enumerates SMART-capable whole disks via UDisks2
//!   `GetManagedObjects` (replaces the sysfs-only [`crate::sensors::disk::
//!   detect_disks`] which exposes label/kind/rotational but not the drive
//!   object path or SMART interface).
//! - `read_disk_smart` / [`read_disk_smart_cached`] query ATA/NVMe SMART
//!   health via UDisks2 `SmartUpdate` + `Properties.Get`.
//! - [`read_battery_sys`] reads system batteries via sysfs first, falling back
//!   to UPower when `/sys/class/power_supply` is unavailable for a battery.
//! - [`read_battery_periph`] reads a peripheral battery via UPower properties.
//! - [`read_battery_bolt`] reads a Logitech Bolt receiver battery through
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
use crate::domain::readings::{
    BatteryPeripheralReading, BatteryState, BatterySystemReading, DiskSmartInterface, SmartDisk,
};
use crate::domain::state::{
    BatteryPeripheralCache, BatterySystemCache, DaemonStateSnapshot, TimedValue,
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

// ── Cache TTLs (mirror `src/sensors.py`) ─────────────────────────────────────

/// System-battery sysfs/UPower refresh TTL.
const BAT_CACHE_TTL: Duration = Duration::from_secs(30);
/// Peripheral-battery UPower refresh TTL.
const PERIPH_CACHE_TTL: Duration = Duration::from_secs(30);
/// Bolt-receiver refresh TTL — a keyboard's charge changes over days, and every
/// Bolt query wakes the device from deep sleep (~900ms round-trip), so polling
/// it often buys nothing and needlessly drains the keyboard's own battery.
const BOLT_CACHE_TTL: Duration = Duration::from_secs(3600);
/// `SmartUpdate` is a real ioctl on the drive (slow on ATA).
const SMART_UPDATE_TIMEOUT: Duration = Duration::from_millis(15_000);

/// 1×10⁶ microwatts per watt, matching `/sys/class/power_supply/.../power_now`.
const MICROWATTS_PER_WATT: u128 = 1_000_000;

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
    dbus.call(DbusRequest {
        bus: BusKind::System,
        service: service.to_owned(),
        object_path: path.to_owned(),
        interface: iface.to_owned(),
        member: member.to_owned(),
        arguments,
        timeout,
    })
    .ok()
    .map(|output| output.body)
}

// ── UPower device discovery ──────────────────────────────────────────────────

/// Enumerates UPower device object paths (replaces `upower -e`).
///
/// Issues `EnumerateDevices` on the system bus and decodes the
/// `[path1, path2, ...]` reply body. Returns an empty list when the bus,
/// UPower service, or the call is unavailable — matching Python's silent
/// degradation.
#[must_use]
pub fn upower_enumerate(dbus: &mut dyn DbusFacade) -> Vec<String> {
    let Some(body) = dbus_call(
        dbus,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        Vec::new(),
        None,
    ) else {
        return Vec::new();
    };
    parse_object_paths(&body)
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
    let body = dbus_call(
        dbus,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        vec![DbusArgument::String(UPOWER_DEV_IFACE.to_owned())],
        None,
    )?;
    let map = parse_property_map(&body);
    Some(UpowerDeviceProps {
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
#[must_use]
pub fn find_battery_sys(dbus: &mut dyn DbusFacade) -> Vec<String> {
    let mut paths: Vec<String> = upower_enumerate(dbus)
        .into_iter()
        .filter(|p| p.contains("/battery_BAT"))
        .collect();
    paths.sort();
    paths
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
#[must_use]
pub fn detect_smart_disks(
    dbus: &mut dyn DbusFacade,
    sys_root: &Path,
) -> BTreeMap<String, SmartDisk> {
    let mut result = BTreeMap::new();
    let Some(body) = dbus_call(
        dbus,
        UDISKS_NAME,
        UDISKS_PATH,
        OBJ_MANAGER_IFACE,
        "GetManagedObjects",
        Vec::new(),
        None,
    ) else {
        return result;
    };
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
    result
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
/// Returns `Some(true)` = healthy, `Some(false)` = failing, `None` = D-Bus call
/// failed or unsupported. NVMe reads `SmartCriticalWarning` (healthy iff
/// empty); ATA reads `SmartFailing` (healthy iff false). A `SmartUpdate` ioctl
/// is triggered first so the values are current.
fn read_disk_smart(
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
            Some(warning.is_empty())
        }
        DiskSmartInterface::Ata => {
            let raw = udisks_get(dbus, drive_path, iface, "SmartFailing")?;
            let failing = parse_bool(&raw)?;
            Some(!failing)
        }
    }
}

/// Cached SMART read keyed by label. `ttl` is per-drive: spinning HDDs use the
/// longer interval because their `SmartUpdate` is slow and wakes the disk.
#[must_use]
pub fn read_disk_smart_cached(
    state: &mut DaemonStateSnapshot,
    dbus: &mut dyn DbusFacade,
    label: &str,
    drive_path: &str,
    kind: DiskSmartInterface,
    now: Duration,
    ttl: Duration,
) -> Option<bool> {
    cached_smart(&mut state.disk_smart_cache, label, now, ttl, || {
        read_disk_smart(dbus, drive_path, kind)
    })
}

/// Label-keyed SMART cache helper mirroring Python's `_cached_by_label`.
fn cached_smart(
    cache: &mut BTreeMap<String, TimedValue<bool>>,
    label: &str,
    now: Duration,
    ttl: Duration,
    read_fn: impl FnOnce() -> Option<bool>,
) -> Option<bool> {
    let needs_refresh = cache
        .get(label)
        .and_then(|entry| entry.sampled_at)
        .is_none_or(|previous| now.saturating_sub(previous) >= ttl);
    if !needs_refresh {
        return cache.get(label).and_then(|entry| entry.value);
    }
    let value = read_fn();
    cache.insert(
        label.to_owned(),
        TimedValue {
            value,
            sampled_at: Some(now),
        },
    );
    value
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

/// Reads all system batteries using cached values where fresh.
///
/// For each battery id in `battery_sys_ids`:
///
/// 1. If the cache is older than `BAT_CACHE_TTL`, try sysfs first; on
///    failure fall back to a UPower `GetAll` property read.
/// 2. Append a [`BatterySystemReading`] only when a non-empty charge is known
///    (matches Python's `if cache.perc:` truthiness gate).
///
/// `state.battery_sys_cache` is updated in place.
#[must_use]
pub fn read_battery_sys(
    state: &mut DaemonStateSnapshot,
    dbus: &mut dyn DbusFacade,
    battery_sys_ids: &[String],
    sys_root: &Path,
    clock: ClockSnapshot,
) -> Vec<BatterySystemReading> {
    let now = clock.monotonic;
    let mut result = Vec::new();
    for bat_id in battery_sys_ids {
        let cache = state.battery_sys_cache.entry(bat_id.clone()).or_default();
        let stale = cache
            .sampled_at
            .is_none_or(|previous| now.saturating_sub(previous) >= BAT_CACHE_TTL);
        if stale {
            refresh_battery_sys_cache(cache, bat_id, dbus, sys_root, now);
        }
        if let Some(charge) = cache.charge_percent {
            result.push(BatterySystemReading {
                id: bat_id.clone(),
                charge_percent: charge,
                rate_watts: cache.rate_watts,
                state: cache.state,
                charge_limit_percent: cache.charge_limit_percent,
            });
        }
    }
    result
}

/// Refreshes one system-battery cache entry: sysfs first, UPower on failure.
fn refresh_battery_sys_cache(
    cache: &mut BatterySystemCache,
    bat_id: &str,
    dbus: &mut dyn DbusFacade,
    sys_root: &Path,
    now: Duration,
) {
    if let Some((capacity, rate, state)) = sysfs_bat_read(sys_root, bat_id) {
        cache.charge_percent = Some(capacity);
        cache.rate_watts = rate;
        cache.state = state;
        cache.charge_limit_percent = sysfs_bat_charge_limit(sys_root, bat_id);
        cache.sampled_at = Some(now);
        return;
    }
    // sysfs unavailable: fall back to UPower over GDBus.
    let Some(props) = upower_device_props(dbus, bat_id) else {
        return;
    };
    if props.percentage.is_none() {
        return;
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
}

// ── Peripheral battery (UPower) ──────────────────────────────────────────────

/// Reads one peripheral-battery reading via UPower, cached for
/// `PERIPH_CACHE_TTL`. Returns `None` when the charge is empty/missing so
/// the row disappears from the tooltip.
#[must_use]
pub fn read_battery_periph(
    cache: &mut BatteryPeripheralCache,
    dbus: &mut dyn DbusFacade,
    upower_path: &str,
    name_override: Option<&str>,
    clock: ClockSnapshot,
) -> Option<BatteryPeripheralReading> {
    let now = clock.monotonic;
    let stale = cache
        .sampled_at
        .is_none_or(|previous| now.saturating_sub(previous) >= PERIPH_CACHE_TTL);
    if stale {
        refresh_periph_cache(cache, dbus, upower_path, now);
    }
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
) {
    let props = upower_device_props(dbus, upower_path);
    cache.sampled_at = Some(now);
    let Some(props) = props else {
        cache.charge_percent = None;
        return;
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
/// semantics in [`read_battery_bolt`]. Tests use a trivial fake. Mirrors
/// Python's `_bolt_query(dev_idx, want_name)` contract.
pub trait BoltBatteryFacade {
    /// Queries the Bolt receiver at `dev_idx`, optionally fetching the device
    /// name. Returns `Ok(None)` when HID++ yields no battery level, including
    /// unsupported-feature, timeout, short-response, and mismatched-response
    /// cases. Returns `Err` when discovery, open, or report write fails.
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

/// Reads one peripheral-battery reading via a Bolt receiver, cached for
/// `BOLT_CACHE_TTL`.
///
/// Mirrors Python's `_read_battery_bolt`: the device name is fetched only until
/// cached (it costs ~10× the battery read), and a `level=None` response still
/// advances the cache timestamp so the slow wake-up doesn't retry every poll.
#[must_use]
pub fn read_battery_bolt(
    cache: &mut BatteryPeripheralCache,
    bolt: &mut dyn BoltBatteryFacade,
    dev_idx: i32,
    name_override: Option<&str>,
    clock: ClockSnapshot,
) -> Option<BatteryPeripheralReading> {
    let now = clock.monotonic;
    let stale = cache
        .sampled_at
        .is_none_or(|previous| now.saturating_sub(previous) >= BOLT_CACHE_TTL);
    if stale {
        let want_name = name_override.is_none() && cache.name.is_empty();
        match bolt.query(dev_idx, want_name) {
            Ok(Some(battery)) => {
                cache.name = name_override
                    .map(String::from)
                    .or_else(|| (!battery.name.is_empty()).then_some(battery.name))
                    .unwrap_or_else(|| cache.name.clone());
                cache.charge_percent = Some(i32::from(battery.level));
                cache.sampled_at = Some(now);
            }
            Ok(None) => {
                // Query succeeded but device has no battery: advance the
                // timestamp to suppress the wake-up cost until the TTL elapses.
                cache.sampled_at = Some(now);
                return None;
            }
            Err(_) => {
                // HID failure: do NOT advance the timestamp (retry next poll).
                return None;
            }
        }
    }
    let charge = cache.charge_percent?;
    Some(BatteryPeripheralReading {
        name: name_override
            .map(String::from)
            .unwrap_or_else(|| cache.name.clone()),
        charge_percent: charge,
    })
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
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use crate::domain::boundary::{BusKind, DbusOutput};
    use crate::test_support::FakeDbus;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    /// Helper to build a `DbusOutput` body tagged with the call signature so
    /// the fake can echo it. Production adapters build this from `busctl` JSON
    /// replies.
    fn dbus_body(
        bus: BusKind,
        service: &str,
        path: &str,
        iface: &str,
        member: &str,
        body: Vec<String>,
    ) -> DbusOutput {
        DbusOutput {
            bus,
            service: service.to_owned(),
            object_path: path.to_owned(),
            interface: iface.to_owned(),
            member: member.to_owned(),
            body,
        }
    }

    const SYSTEM: BusKind = BusKind::System;

    /// monotonic(t) → ClockSnapshot with a zero wall clock (tests only use
    /// monotonic time for TTL gates).
    fn clock(seconds: u64) -> ClockSnapshot {
        ClockSnapshot {
            monotonic: Duration::from_secs(seconds),
            wall: SystemTime::UNIX_EPOCH,
        }
    }

    fn upath(member: &str, body: Vec<String>) -> DbusOutput {
        dbus_body(
            SYSTEM,
            UPOWER_NAME,
            "/org/freedesktop/UPower",
            UPOWER_IFACE,
            member,
            body,
        )
    }

    fn battery_props_reply(path: &str, props: &[(&str, &str)]) -> DbusOutput {
        let body: Vec<String> = props
            .iter()
            .flat_map(|(k, v)| [(*k).to_owned(), (*v).to_owned()])
            .collect();
        dbus_body(
            SYSTEM,
            UPOWER_NAME,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            body,
        )
    }

    /// In-memory fake Bolt facade with FIFO replies keyed by `(dev_idx,
    /// want_name)`. Each call pops the next queued reply.
    #[derive(Default)]
    struct FakeBolt {
        ok_replies: Vec<(i32, bool, Option<BoltBattery>)>,
        err_replies: Vec<(i32, bool)>,
        calls: Vec<(i32, bool)>,
    }

    impl FakeBolt {
        fn push_ok(
            &mut self,
            dev_idx: i32,
            want_name: bool,
            battery: Option<BoltBattery>,
        ) -> &mut Self {
            self.ok_replies.push((dev_idx, want_name, battery));
            self
        }

        fn push_err(&mut self, dev_idx: i32, want_name: bool) -> &mut Self {
            self.err_replies.push((dev_idx, want_name));
            self
        }

        fn calls(&self) -> &[(i32, bool)] {
            &self.calls
        }
    }

    impl BoltBatteryFacade for FakeBolt {
        fn query(
            &mut self,
            dev_idx: i32,
            want_name: bool,
        ) -> Result<Option<BoltBattery>, BoundaryError> {
            self.calls.push((dev_idx, want_name));
            if let Some(idx) = self
                .err_replies
                .iter()
                .position(|(d, w)| *d == dev_idx && *w == want_name)
            {
                self.err_replies.swap_remove(idx);
                return Err(BoundaryError::DbusCallFailed {
                    bus: BusKind::Session,
                    service: "bolt".to_owned(),
                    path: "/dev/hidraw0".to_owned(),
                    interface: "HIDPP".to_owned(),
                    member: "query".to_owned(),
                    detail: "hid read timeout".to_owned(),
                });
            }
            if let Some(idx) = self
                .ok_replies
                .iter()
                .position(|(d, w, _)| *d == dev_idx && *w == want_name)
            {
                let (_, _, battery) = self.ok_replies.swap_remove(idx);
                Ok(battery)
            } else {
                Ok(None)
            }
        }
    }

    /// Minimal temp-directory helper for sysfs fixture trees (mirrors
    /// `sensors::disk::tests::TempTree`).
    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let root = std::env::temp_dir()
                .join(format!("pirostats-power-{}-{unique}", std::process::id(),));
            fs::create_dir_all(&root).expect("temp root");
            Self { root }
        }

        fn sys(&self) -> PathBuf {
            self.root.join("sys")
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("parent");
            }
            fs::write(&path, content).expect("write");
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    // ── parse helpers ────────────────────────────────────────────────────────

    #[test]
    fn parse_object_paths_skips_empty_strings() {
        let paths = parse_object_paths(&["/a".to_owned(), String::new(), "/b".to_owned()]);

        assert_eq!(paths, ["/a", "/b"]);
    }

    #[test]
    fn parse_property_map_decodes_interleaved_pairs() {
        let map = parse_property_map(&[
            "Percentage".to_owned(),
            "85".to_owned(),
            "State".to_owned(),
            "2".to_owned(),
        ]);

        assert_eq!(map.get("Percentage").map(String::as_str), Some("85"));
        assert_eq!(map.get("State").map(String::as_str), Some("2"));
    }

    #[test]
    fn parse_property_map_ignores_stray_trailing_key() {
        let map = parse_property_map(&["Orphan".to_owned()]);

        assert!(map.is_empty());
    }

    #[test]
    fn parse_managed_objects_splits_on_empty_strings() {
        let objects = parse_managed_objects(&[
            "/block_devices/nvme0n1".to_owned(),
            UDISKS_BLOCK.to_owned(),
            format!("{BLOCK_DRIVE_PREFIX}/drives/NVMe_1"),
            String::new(),
            "/drives/NVMe_1".to_owned(),
            UDISKS_NVME.to_owned(),
        ]);

        assert_eq!(objects.len(), 2);
        assert!(objects[0].is_block);
        assert_eq!(objects[0].drive_path.as_deref(), Some("/drives/NVMe_1"));
        assert!(objects[1].has_nvme);
    }

    // ── upower_enumerate ─────────────────────────────────────────────────────

    #[test]
    fn upower_enumerate_returns_paths_on_success() {
        let mut dbus = FakeDbus::new();
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            UPOWER_PATH,
            UPOWER_IFACE,
            "EnumerateDevices",
            upath(
                "EnumerateDevices",
                vec!["/battery_BAT0".to_owned(), "/battery_BAT1".to_owned()],
            ),
        );

        let paths = upower_enumerate(&mut dbus);

        assert_eq!(
            paths,
            ["/battery_BAT0".to_owned(), "/battery_BAT1".to_owned()]
        );
    }

    #[test]
    fn upower_enumerate_empty_when_bus_unavailable() {
        let mut dbus = FakeDbus::new();

        let paths = upower_enumerate(&mut dbus);

        assert!(paths.is_empty());
    }

    // ── find_battery_sys ─────────────────────────────────────────────────────

    #[test]
    fn find_battery_sys_filters_and_sorts_battery_paths() {
        let mut dbus = FakeDbus::new();
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            UPOWER_PATH,
            UPOWER_IFACE,
            "EnumerateDevices",
            upath(
                "EnumerateDevices",
                vec![
                    "/org/freedesktop/UPower/devices/battery_BAT1".to_owned(),
                    "/org/freedesktop/UPower/devices/battery_hidpp_mouse".to_owned(),
                    "/org/freedesktop/UPower/devices/battery_BAT0".to_owned(),
                ],
            ),
        );

        let batteries = find_battery_sys(&mut dbus);

        assert_eq!(
            batteries,
            [
                "/org/freedesktop/UPower/devices/battery_BAT0".to_owned(),
                "/org/freedesktop/UPower/devices/battery_BAT1".to_owned(),
            ]
        );
    }

    // ── detect_smart_disks ───────────────────────────────────────────────────

    fn managed_objects_reply(objects: &[Vec<&str>]) -> DbusOutput {
        let mut body = Vec::new();
        for (idx, obj) in objects.iter().enumerate() {
            if idx > 0 {
                body.push(String::new());
            }
            body.extend(obj.iter().map(|s| (*s).to_owned()));
        }
        dbus_body(
            SYSTEM,
            UDISKS_NAME,
            UDISKS_PATH,
            OBJ_MANAGER_IFACE,
            "GetManagedObjects",
            body,
        )
    }

    fn write_rotational(sys: &TempTree, label: &str, rotational: bool) {
        sys.write(
            &format!("sys/block/{label}/queue/rotational"),
            if rotational { "1" } else { "0" },
        );
    }

    #[test]
    fn detect_smart_disks_finds_nvme_and_ata_drives() {
        let tmp = TempTree::new();
        write_rotational(&tmp, "nvme0n1", false);
        write_rotational(&tmp, "sda", true);
        let mut dbus = FakeDbus::new();
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            UDISKS_PATH,
            OBJ_MANAGER_IFACE,
            "GetManagedObjects",
            managed_objects_reply(&[
                vec![
                    "/org/freedesktop/UDisks2/block_devices/nvme0n1",
                    UDISKS_BLOCK,
                    &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/NVMe_1234"),
                ],
                vec!["/org/freedesktop/UDisks2/drives/NVMe_1234", UDISKS_NVME],
                vec![
                    "/org/freedesktop/UDisks2/block_devices/sda",
                    UDISKS_BLOCK,
                    &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/SATA_1"),
                ],
                vec!["/org/freedesktop/UDisks2/drives/SATA_1", UDISKS_ATA],
            ]),
        );

        let disks = detect_smart_disks(&mut dbus, &tmp.sys());

        let nvme = disks.get("nvme0n1").expect("nvme present");
        assert_eq!(
            nvme.object_path,
            "/org/freedesktop/UDisks2/drives/NVMe_1234"
        );
        assert_eq!(nvme.interface, DiskSmartInterface::Nvme);
        assert!(!nvme.rotational);
        let sata = disks.get("sda").expect("sata present");
        assert_eq!(sata.interface, DiskSmartInterface::Ata);
        assert!(sata.rotational);
    }

    #[test]
    fn detect_smart_disks_skips_partitions() {
        let tmp = TempTree::new();
        write_rotational(&tmp, "nvme0n1", false);
        write_rotational(&tmp, "nvme0n1p1", false);
        let mut dbus = FakeDbus::new();
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            UDISKS_PATH,
            OBJ_MANAGER_IFACE,
            "GetManagedObjects",
            managed_objects_reply(&[
                vec![
                    "/org/freedesktop/UDisks2/block_devices/nvme0n1",
                    UDISKS_BLOCK,
                    &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/NVMe_1"),
                ],
                vec![
                    "/org/freedesktop/UDisks2/block_devices/nvme0n1p1",
                    UDISKS_BLOCK,
                    UDISKS_PARTITION,
                    &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/NVMe_1"),
                ],
                vec!["/org/freedesktop/UDisks2/drives/NVMe_1", UDISKS_NVME],
            ]),
        );

        let disks = detect_smart_disks(&mut dbus, &tmp.sys());

        assert_eq!(disks.len(), 1);
        assert!(disks.contains_key("nvme0n1"));
    }

    #[test]
    fn detect_smart_disks_skips_optical_and_missing_drive_and_unsupported() {
        let tmp = TempTree::new();
        write_rotational(&tmp, "sr0", false);
        write_rotational(&tmp, "sdb", false);
        write_rotational(&tmp, "sdc", false);
        let mut dbus = FakeDbus::new();
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            UDISKS_PATH,
            OBJ_MANAGER_IFACE,
            "GetManagedObjects",
            managed_objects_reply(&[
                // optical drive — skipped by sr* prefix
                vec![
                    "/org/freedesktop/UDisks2/block_devices/sr0",
                    UDISKS_BLOCK,
                    &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/Odd"),
                ],
                vec!["/org/freedesktop/UDisks2/drives/Odd", UDISKS_ATA],
                // block with empty drive ref — skipped
                vec![
                    "/org/freedesktop/UDisks2/block_devices/sdb",
                    UDISKS_BLOCK,
                    &format!("{BLOCK_DRIVE_PREFIX}/"),
                ],
                // block whose drive is absent from the reply — skipped
                vec![
                    "/org/freedesktop/UDisks2/block_devices/sdc",
                    UDISKS_BLOCK,
                    &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/Ghost"),
                ],
            ]),
        );

        let disks = detect_smart_disks(&mut dbus, &tmp.sys());

        assert!(disks.is_empty(), "no drive qualifies: {disks:?}");
    }

    #[test]
    fn detect_smart_disks_empty_when_bus_unavailable() {
        let tmp = TempTree::new();
        let mut dbus = FakeDbus::new();

        let disks = detect_smart_disks(&mut dbus, &tmp.sys());

        assert!(disks.is_empty());
    }

    // ── read_disk_smart ──────────────────────────────────────────────────────

    #[test]
    fn read_disk_smart_nvme_healthy_when_warning_empty() {
        let mut dbus = FakeDbus::new();
        let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            drive,
            UDISKS_NVME,
            "SmartUpdate",
            dbus_body(
                SYSTEM,
                UDISKS_NAME,
                drive,
                UDISKS_NVME,
                "SmartUpdate",
                Vec::new(),
            ),
        );
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            drive,
            "org.freedesktop.DBus.Properties",
            "Get",
            dbus_body(
                SYSTEM,
                UDISKS_NAME,
                drive,
                "org.freedesktop.DBus.Properties",
                "Get",
                vec![String::new()],
            ),
        );

        let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Nvme);

        assert_eq!(health, Some(true));
        let trace = dbus.call_trace();
        assert_eq!(trace[0].arguments, [DbusArgument::EmptyStringVariantDict]);
        assert_eq!(trace[0].timeout, Some(SMART_UPDATE_TIMEOUT));
        assert_eq!(trace[1].interface, "org.freedesktop.DBus.Properties");
        assert_eq!(trace[1].member, "Get");
        assert_eq!(
            trace[1].arguments,
            [
                DbusArgument::String(UDISKS_NVME.to_owned()),
                DbusArgument::String("SmartCriticalWarning".to_owned()),
            ]
        );
    }

    #[test]
    fn read_disk_smart_nvme_failing_when_warning_present() {
        let mut dbus = FakeDbus::new();
        let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            drive,
            UDISKS_NVME,
            "SmartUpdate",
            dbus_body(
                SYSTEM,
                UDISKS_NAME,
                drive,
                UDISKS_NVME,
                "SmartUpdate",
                Vec::new(),
            ),
        );
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            drive,
            "org.freedesktop.DBus.Properties",
            "Get",
            dbus_body(
                SYSTEM,
                UDISKS_NAME,
                drive,
                "org.freedesktop.DBus.Properties",
                "Get",
                vec!["available spare".to_owned()],
            ),
        );

        let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Nvme);

        assert_eq!(health, Some(false));
    }

    #[test]
    fn read_disk_smart_ata_healthy_when_not_failing() {
        let mut dbus = FakeDbus::new();
        let drive = "/org/freedesktop/UDisks2/drives/SATA_1";
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            drive,
            UDISKS_ATA,
            "SmartUpdate",
            dbus_body(
                SYSTEM,
                UDISKS_NAME,
                drive,
                UDISKS_ATA,
                "SmartUpdate",
                Vec::new(),
            ),
        );
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            drive,
            "org.freedesktop.DBus.Properties",
            "Get",
            dbus_body(
                SYSTEM,
                UDISKS_NAME,
                drive,
                "org.freedesktop.DBus.Properties",
                "Get",
                vec!["false".to_owned()],
            ),
        );

        let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Ata);

        assert_eq!(health, Some(true));
    }

    #[test]
    fn read_disk_smart_ata_failing_when_smart_failing_true() {
        let mut dbus = FakeDbus::new();
        let drive = "/org/freedesktop/UDisks2/drives/SATA_1";
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            drive,
            UDISKS_ATA,
            "SmartUpdate",
            dbus_body(
                SYSTEM,
                UDISKS_NAME,
                drive,
                UDISKS_ATA,
                "SmartUpdate",
                Vec::new(),
            ),
        );
        dbus.enqueue(
            SYSTEM,
            UDISKS_NAME,
            drive,
            "org.freedesktop.DBus.Properties",
            "Get",
            dbus_body(
                SYSTEM,
                UDISKS_NAME,
                drive,
                "org.freedesktop.DBus.Properties",
                "Get",
                vec!["true".to_owned()],
            ),
        );

        let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Ata);

        assert_eq!(health, Some(false));
    }

    #[test]
    fn read_disk_smart_returns_none_when_smart_update_unreachable() {
        let mut dbus = FakeDbus::new();
        let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";

        // SmartUpdate fails → no property read attempted → None.
        let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Nvme);

        assert_eq!(health, None);
    }

    // ── read_disk_smart_cached ───────────────────────────────────────────────

    #[test]
    fn read_disk_smart_cached_refreshes_after_ttl_expires() {
        let mut state = DaemonStateSnapshot::default();
        let mut dbus = FakeDbus::new();
        let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";

        let enqueue_reply = |dbus: &mut FakeDbus, healthy: bool| {
            dbus.enqueue(
                SYSTEM,
                UDISKS_NAME,
                drive,
                UDISKS_NVME,
                "SmartUpdate",
                dbus_body(
                    SYSTEM,
                    UDISKS_NAME,
                    drive,
                    UDISKS_NVME,
                    "SmartUpdate",
                    Vec::new(),
                ),
            );
            dbus.enqueue(
                SYSTEM,
                UDISKS_NAME,
                drive,
                "org.freedesktop.DBus.Properties",
                "Get",
                dbus_body(
                    SYSTEM,
                    UDISKS_NAME,
                    drive,
                    "org.freedesktop.DBus.Properties",
                    "Get",
                    vec![if healthy {
                        String::new()
                    } else {
                        "available spare".to_owned()
                    }],
                ),
            );
        };

        let ttl = Duration::from_secs(60);
        enqueue_reply(&mut dbus, false);
        let first = read_disk_smart_cached(
            &mut state,
            &mut dbus,
            "nvme0n1",
            drive,
            DiskSmartInterface::Nvme,
            Duration::from_secs(0),
            ttl,
        );
        assert_eq!(first, Some(false));

        // Within TTL: returns cached value, makes no D-Bus calls.
        let cached = read_disk_smart_cached(
            &mut state,
            &mut dbus,
            "nvme0n1",
            drive,
            DiskSmartInterface::Nvme,
            Duration::from_secs(30),
            ttl,
        );
        assert_eq!(cached, Some(false));
        assert_eq!(dbus.call_trace().len(), 2);

        // After TTL: refreshes with a new reply.
        enqueue_reply(&mut dbus, true);
        let refreshed = read_disk_smart_cached(
            &mut state,
            &mut dbus,
            "nvme0n1",
            drive,
            DiskSmartInterface::Nvme,
            Duration::from_secs(61),
            ttl,
        );
        assert_eq!(refreshed, Some(true));
    }

    #[test]
    fn read_disk_smart_cached_caches_failure_until_ttl_expires() {
        let mut state = DaemonStateSnapshot::default();
        let mut dbus = FakeDbus::new();
        let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";

        // SmartUpdate unreachable: read returns None, but cached as a sampled
        // None so the next call within TTL doesn't retry.
        let first = read_disk_smart_cached(
            &mut state,
            &mut dbus,
            "nvme0n1",
            drive,
            DiskSmartInterface::Nvme,
            Duration::from_secs(0),
            Duration::from_secs(60),
        );
        assert_eq!(first, None);

        // Python stops immediately when SmartUpdate fails.
        assert_eq!(dbus.call_trace().len(), 1);

        let cached = read_disk_smart_cached(
            &mut state,
            &mut dbus,
            "nvme0n1",
            drive,
            DiskSmartInterface::Nvme,
            Duration::from_secs(10),
            Duration::from_secs(60),
        );
        assert_eq!(cached, None);
        // No additional calls: failure is cached until TTL elapses.
        assert_eq!(dbus.call_trace().len(), 1);
    }

    // ── read_battery_sys ─────────────────────────────────────────────────────

    #[test]
    fn read_battery_sys_reads_sysfs_first() {
        let tmp = TempTree::new();
        tmp.write("sys/class/power_supply/BAT0/capacity", "85\n");
        tmp.write("sys/class/power_supply/BAT0/status", "Discharging\n");
        tmp.write("sys/class/power_supply/BAT0/power_now", "12500000\n");
        tmp.write(
            "sys/class/power_supply/BAT0/charge_control_end_threshold",
            "80\n",
        );

        let mut state = DaemonStateSnapshot::default();
        let mut dbus = FakeDbus::new();

        let readings = read_battery_sys(
            &mut state,
            &mut dbus,
            &["/org/freedesktop/UPower/devices/battery_BAT0".to_owned()],
            &tmp.sys(),
            clock(0),
        );

        let battery = readings.first().expect("battery read");
        assert_eq!(battery.id, "/org/freedesktop/UPower/devices/battery_BAT0");
        assert_eq!(battery.charge_percent, 85);
        // 12_500_000 µW → 12.5 W → banker's rounding → 12.
        assert_eq!(battery.rate_watts, 12);
        assert_eq!(battery.state, BatteryState::Discharging);
        assert_eq!(battery.charge_limit_percent, Some(80));
        // No D-Bus calls: sysfs path succeeded.
        assert!(dbus.call_trace().is_empty());
    }

    #[test]
    fn read_battery_sys_falls_back_to_upower_when_sysfs_absent() {
        let tmp = TempTree::new();
        let mut state = DaemonStateSnapshot::default();
        let mut dbus = FakeDbus::new();
        let path = "/org/freedesktop/UPower/devices/battery_BAT0";
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            battery_props_reply(
                path,
                &[("Percentage", "90"), ("State", "1"), ("EnergyRate", "15.5")],
            ),
        );

        let readings = read_battery_sys(
            &mut state,
            &mut dbus,
            &[path.to_owned()],
            &tmp.sys(),
            clock(0),
        );

        let battery = readings.first().expect("fallback battery");
        assert_eq!(battery.charge_percent, 90);
        assert_eq!(battery.state, BatteryState::Charging);
        // 15.5 → banker's rounding → 16.
        assert_eq!(battery.rate_watts, 16);
        let request = dbus.call_trace().first().expect("GetAll request");
        assert_eq!(request.interface, "org.freedesktop.DBus.Properties");
        assert_eq!(request.member, "GetAll");
        assert_eq!(
            request.arguments,
            [DbusArgument::String(UPOWER_DEV_IFACE.to_owned())]
        );
    }

    #[test]
    fn read_battery_sys_upower_zero_rate_falls_back_to_sysfs_power_now() {
        let tmp = TempTree::new();
        // Sysfs has no capacity (so the sysfs primary path fails and we drop to
        // UPower), but power_now IS readable for the rate fallback.
        tmp.write("sys/class/power_supply/BAT0/power_now", "5000000\n");
        let mut state = DaemonStateSnapshot::default();
        let mut dbus = FakeDbus::new();
        let path = "/org/freedesktop/UPower/devices/battery_BAT0";
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            battery_props_reply(
                path,
                &[("Percentage", "70"), ("State", "2"), ("EnergyRate", "0")],
            ),
        );

        let readings = read_battery_sys(
            &mut state,
            &mut dbus,
            &[path.to_owned()],
            &tmp.sys(),
            clock(0),
        );

        let battery = readings.first().expect("battery");
        // EnergyRate 0 + discharging → fallback to sysfs 5_000_000 µW = 5 W.
        assert_eq!(battery.rate_watts, 5);
    }

    #[test]
    fn read_battery_sys_skips_batteries_without_percentage() {
        let tmp = TempTree::new();
        let mut state = DaemonStateSnapshot::default();
        let mut dbus = FakeDbus::new();
        let path = "/org/freedesktop/UPower/devices/battery_BAT0";
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            battery_props_reply(path, &[("State", "2")]),
        );

        let readings = read_battery_sys(
            &mut state,
            &mut dbus,
            &[path.to_owned()],
            &tmp.sys(),
            clock(0),
        );

        assert!(readings.is_empty(), "no percentage → no row");
    }

    #[test]
    fn read_battery_sys_uses_cache_within_ttl() {
        let tmp = TempTree::new();
        tmp.write("sys/class/power_supply/BAT0/capacity", "50\n");
        tmp.write("sys/class/power_supply/BAT0/status", "Charging\n");

        let mut state = DaemonStateSnapshot::default();
        let mut dbus = FakeDbus::new();

        let _ = read_battery_sys(
            &mut state,
            &mut dbus,
            &["/org/freedesktop/UPower/devices/battery_BAT0".to_owned()],
            &tmp.sys(),
            clock(0),
        );

        // Remove sysfs to prove the second read is cached.
        let _ = fs::remove_file(tmp.sys().join("class/power_supply/BAT0/capacity"));

        let readings = read_battery_sys(
            &mut state,
            &mut dbus,
            &["/org/freedesktop/UPower/devices/battery_BAT0".to_owned()],
            &tmp.sys(),
            clock(10),
        );

        assert_eq!(readings.first().expect("cached").charge_percent, 50);
    }

    #[test]
    fn read_battery_sys_charge_limit_100_treated_as_unset() {
        let tmp = TempTree::new();
        tmp.write("sys/class/power_supply/BAT0/capacity", "99\n");
        tmp.write("sys/class/power_supply/BAT0/status", "Full\n");
        tmp.write(
            "sys/class/power_supply/BAT0/charge_control_end_threshold",
            "100\n",
        );

        let mut state = DaemonStateSnapshot::default();
        let mut dbus = FakeDbus::new();

        let readings = read_battery_sys(
            &mut state,
            &mut dbus,
            &["/org/freedesktop/UPower/devices/battery_BAT0".to_owned()],
            &tmp.sys(),
            clock(0),
        );

        assert_eq!(
            readings.first().expect("battery").charge_limit_percent,
            None
        );
    }

    // ── read_battery_periph ──────────────────────────────────────────────────

    #[test]
    fn read_battery_periph_returns_reading_on_success() {
        let mut cache = BatteryPeripheralCache::default();
        let mut dbus = FakeDbus::new();
        let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            battery_props_reply(path, &[("Percentage", "75"), ("Model", "MX Master 3S")]),
        );

        let reading =
            read_battery_periph(&mut cache, &mut dbus, path, None, clock(0)).expect("present");

        assert_eq!(reading.name, "MX Master 3S");
        assert_eq!(reading.charge_percent, 75);
    }

    #[test]
    fn read_battery_periph_none_when_percentage_zero_or_missing() {
        let mut cache = BatteryPeripheralCache::default();
        let mut dbus = FakeDbus::new();
        let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            battery_props_reply(path, &[("Percentage", "0"), ("Model", "MX Keys")]),
        );

        let reading = read_battery_periph(&mut cache, &mut dbus, path, None, clock(0));

        assert!(reading.is_none(), "0% → device disconnected");
        // The name was still cached while we had the props.
        assert_eq!(cache.name, "MX Keys");
    }

    #[test]
    fn read_battery_periph_none_when_upower_unreachable() {
        let mut cache = BatteryPeripheralCache::default();
        let mut dbus = FakeDbus::new();
        let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";

        let reading = read_battery_periph(&mut cache, &mut dbus, path, None, clock(0));

        assert!(reading.is_none());
    }

    #[test]
    fn read_battery_periph_name_override_wins_over_cached_model() {
        let mut cache = BatteryPeripheralCache::default();
        let mut dbus = FakeDbus::new();
        let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            battery_props_reply(path, &[("Percentage", "60"), ("Model", "Internal Name")]),
        );

        let reading = read_battery_periph(&mut cache, &mut dbus, path, Some("Override"), clock(0))
            .expect("present");

        assert_eq!(reading.name, "Override");
    }

    #[test]
    fn read_battery_periph_uses_cache_within_ttl() {
        let mut cache = BatteryPeripheralCache::default();
        let mut dbus = FakeDbus::new();
        let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";
        dbus.enqueue(
            SYSTEM,
            UPOWER_NAME,
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            battery_props_reply(path, &[("Percentage", "80"), ("Model", "Mouse")]),
        );

        let _ = read_battery_periph(&mut cache, &mut dbus, path, None, clock(0));
        assert_eq!(dbus.call_trace().len(), 1);

        // Second call within TTL: no new D-Bus call, cached charge returned.
        let reading = read_battery_periph(&mut cache, &mut dbus, path, None, clock(10));
        assert_eq!(reading.expect("cached").charge_percent, 80);
        assert_eq!(dbus.call_trace().len(), 1);
    }

    // ── read_battery_bolt ────────────────────────────────────────────────────

    #[test]
    fn read_battery_bolt_caches_name_and_level() {
        let mut cache = BatteryPeripheralCache::default();
        let mut bolt = FakeBolt::default();
        bolt.push_ok(
            1,
            true,
            Some(BoltBattery {
                name: String::from("MX Keys S"),
                level: 90,
            }),
        );

        let reading = read_battery_bolt(&mut cache, &mut bolt, 1, None, clock(0)).expect("present");

        assert_eq!(reading.name, "MX Keys S");
        assert_eq!(reading.charge_percent, 90);
        assert_eq!(bolt.calls(), &[(1, true)]);

        // Within the 1h TTL: no new query, cached values returned.
        let reading_cached =
            read_battery_bolt(&mut cache, &mut bolt, 1, None, clock(60)).expect("cached");
        assert_eq!(reading_cached.charge_percent, 90);
        assert_eq!(bolt.calls().len(), 1);

        // After the TTL: fetch level only (name stays cached → want_name=false).
        bolt.push_ok(
            1,
            false,
            Some(BoltBattery {
                name: String::new(),
                level: 80,
            }),
        );
        let reading2 =
            read_battery_bolt(&mut cache, &mut bolt, 1, None, clock(3601)).expect("refreshed");
        assert_eq!(reading2.charge_percent, 80);
        assert_eq!(reading2.name, "MX Keys S");
        assert_eq!(bolt.calls(), &[(1, true), (1, false)]);
    }

    #[test]
    fn read_battery_bolt_returns_none_when_level_is_none_but_advances_timestamp() {
        let mut cache = BatteryPeripheralCache::default();
        let mut bolt = FakeBolt::default();
        bolt.push_ok(2, true, None);

        let reading = read_battery_bolt(&mut cache, &mut bolt, 2, None, clock(0));
        assert!(reading.is_none());

        // Cache timestamp advanced → second call within TTL makes no query.
        let reading2 = read_battery_bolt(&mut cache, &mut bolt, 2, None, clock(60));
        assert!(reading2.is_none());
        assert_eq!(bolt.calls().len(), 1);
    }

    #[test]
    fn read_battery_bolt_no_level_hides_stale_charge_for_refresh_call() {
        let mut cache = BatteryPeripheralCache {
            name: "Keyboard".to_owned(),
            charge_percent: Some(80),
            sampled_at: Some(Duration::ZERO),
        };
        let mut bolt = FakeBolt::default();
        bolt.push_ok(2, false, None);

        let refreshed = read_battery_bolt(&mut cache, &mut bolt, 2, None, clock(3601));

        assert!(refreshed.is_none());
        // Python retains the old value in the cache but hides it on the
        // refresh call that reported no level.
        assert_eq!(cache.charge_percent, Some(80));
        assert!(read_battery_bolt(&mut cache, &mut bolt, 2, None, clock(3602)).is_some());
        assert_eq!(bolt.calls().len(), 1);
    }

    #[test]
    fn read_battery_bolt_returns_none_on_hid_failure_without_advancing_timestamp() {
        let mut cache = BatteryPeripheralCache::default();
        let mut bolt = FakeBolt::default();
        bolt.push_err(3, true);

        let reading = read_battery_bolt(&mut cache, &mut bolt, 3, None, clock(0));
        assert!(reading.is_none());

        // Timestamp NOT advanced → next call retries.
        bolt.push_ok(
            3,
            true,
            Some(BoltBattery {
                name: String::from("Recovered"),
                level: 50,
            }),
        );
        let reading2 =
            read_battery_bolt(&mut cache, &mut bolt, 3, None, clock(1)).expect("retry ok");
        assert_eq!(reading2.charge_percent, 50);
        assert_eq!(reading2.name, "Recovered");
    }

    #[test]
    fn read_battery_bolt_name_override_suppresses_name_fetch() {
        let mut cache = BatteryPeripheralCache::default();
        let mut bolt = FakeBolt::default();
        // want_name should be false because name_override is provided.
        bolt.push_ok(
            1,
            false,
            Some(BoltBattery {
                name: String::new(),
                level: 70,
            }),
        );

        let reading =
            read_battery_bolt(&mut cache, &mut bolt, 1, Some("Custom"), clock(0)).expect("ok");

        assert_eq!(reading.name, "Custom");
        assert_eq!(reading.charge_percent, 70);
        assert_eq!(bolt.calls(), &[(1, false)]);
    }

    // ── numeric helpers ──────────────────────────────────────────────────────

    #[test]
    fn round_half_even_ratio_matches_python_bankers_rounding() {
        // 1.5 → 2 (even), 2.5 → 2 (even), 0.5 → 0 (even).
        assert_eq!(round_half_even_ratio(1_500_000, 1_000_000), 2);
        assert_eq!(round_half_even_ratio(2_500_000, 1_000_000), 2);
        assert_eq!(round_half_even_ratio(500_000, 1_000_000), 0);
        // Not-at-half rounds normally.
        assert_eq!(round_half_even_ratio(1_600_000, 1_000_000), 2);
        assert_eq!(round_half_even_ratio(1_400_000, 1_000_000), 1);
    }

    #[test]
    fn round_half_even_f64_handles_halfway_and_non_finite() {
        assert_eq!(round_half_even_f64(15.5), 16);
        assert_eq!(round_half_even_f64(14.5), 14);
        assert_eq!(round_half_even_f64(0.5), 0);
        assert_eq!(round_half_even_f64(15.0), 15);
        assert_eq!(round_half_even_f64(15.4), 15);
        assert_eq!(round_half_even_f64(15.6), 16);
        assert_eq!(round_half_even_f64(f64::NAN), 0);
        assert_eq!(round_half_even_f64(f64::INFINITY), 0);
    }

    #[test]
    fn bat_name_from_id_extracts_power_supply_name() {
        assert_eq!(
            bat_name_from_id("/org/freedesktop/UPower/devices/battery_BAT0"),
            "BAT0",
        );
        assert_eq!(bat_name_from_id("BAT0"), "BAT0");
        assert_eq!(bat_name_from_id("battery_BAT1"), "BAT1");
    }

    #[test]
    fn parse_bool_accepts_case_insensitive_true_false() {
        assert_eq!(parse_bool("true"), Some(true));
        assert_eq!(parse_bool("FALSE"), Some(false));
        assert_eq!(parse_bool("  True  "), Some(true));
        assert_eq!(parse_bool("yes"), None);
    }
}
