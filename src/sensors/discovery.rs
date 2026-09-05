use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::Path;

use crate::config::Config;
use crate::domain::boundary::{CommandRunner, CommandStatus, DbusFacade};
use crate::domain::readings::{AmdGpuSource, HardwareInventory, InventoryFamily, SmartDisk};

use super::{NETWORK_COMMAND_TIMEOUT, cpu, disk, gpu_amd, gpu_intel, gpu_nvidia, network, power};

/// Items whose formatter hardware gate reads `hw.net_device`; as long as one is
/// configured and the device is `None`, peripheral rescan is worth retrying.
/// Mirrors `_NET_GATED_ITEMS` in `src/sensors.py`.
const NET_GATED_ITEMS: &[&str] = &["net_speed", "net_device_ip", "net_ip", "net_device"];

/// One inventory probe result that preserves the difference between a
/// successful empty result and a boundary failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoveryOutcome<T> {
    /// The boundary completed and this is its current value, which may be empty.
    Confirmed(T),
    /// The boundary did not complete; retained inventory must not be changed.
    Failed,
}

impl<T> DiscoveryOutcome<T> {
    fn from_result<E>(result: Result<T, E>) -> Self {
        result.map_or(Self::Failed, Self::Confirmed)
    }

    fn map<U>(self, map: impl FnOnce(T) -> U) -> DiscoveryOutcome<U> {
        match self {
            Self::Confirmed(value) => DiscoveryOutcome::Confirmed(map(value)),
            Self::Failed => DiscoveryOutcome::Failed,
        }
    }
}

/// Scheduler-facing result of reconciling one demanded inventory family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReconciliationOutcome {
    /// Every required probe completed, including confirmed absence.
    Captured,
    /// At least one required probe failed and its previous inventory was retained.
    Failed,
}

impl ReconciliationOutcome {
    const fn and(self, other: Self) -> Self {
        if matches!(self, Self::Failed) || matches!(other, Self::Failed) {
            Self::Failed
        } else {
            Self::Captured
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PeripheralOutcomes {
    mouse: DiscoveryOutcome<Option<String>>,
    keyboard: DiscoveryOutcome<Option<String>>,
}

/// Typed result of one complete hardware inventory attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareDiscovery {
    local: HardwareInventory,
    cpu_temperature: DiscoveryOutcome<Option<std::path::PathBuf>>,
    cpu_controls: DiscoveryOutcome<(Option<std::path::PathBuf>, Option<std::path::PathBuf>, bool)>,
    thermal: DiscoveryOutcome<(
        BTreeMap<String, std::path::PathBuf>,
        BTreeMap<String, std::path::PathBuf>,
    )>,
    system_batteries: DiscoveryOutcome<Vec<String>>,
    nvidia: DiscoveryOutcome<bool>,
    amd: DiscoveryOutcome<Option<AmdGpuSource>>,
    intel: DiscoveryOutcome<(Option<std::path::PathBuf>, Option<String>)>,
    route: DiscoveryOutcome<Option<String>>,
    disk_io: DiscoveryOutcome<Option<String>>,
    backlight: DiscoveryOutcome<bool>,
    wifi: DiscoveryOutcome<bool>,
    peripherals: PeripheralOutcomes,
    smart: DiscoveryOutcome<BTreeMap<String, SmartDisk>>,
}

impl HardwareDiscovery {
    /// Merges every confirmed portion and retains prior values for failed portions.
    pub fn merge_into(self, inventory: &mut HardwareInventory) {
        inventory.cpu_count = self.local.cpu_count;
        apply(&mut inventory.cpu_temp_path, self.cpu_temperature);
        if let DiscoveryOutcome::Confirmed((frequency, turbo, turbo_supported)) = self.cpu_controls
        {
            inventory.cpu_freq_path = frequency;
            inventory.cpu_turbo_path = turbo;
            inventory.cpu_turbo_supported = turbo_supported;
        }
        if let DiscoveryOutcome::Confirmed((temperatures, fans)) = self.thermal {
            inventory.hd_temp_paths = temperatures;
            inventory.fan_paths = fans;
        }
        apply(&mut inventory.battery_sys_ids, self.system_batteries);
        apply(&mut inventory.has_nvidia, self.nvidia);
        apply(&mut inventory.amd_gpu, self.amd);
        if let DiscoveryOutcome::Confirmed((frequency, pci)) = self.intel {
            inventory.intel_gpu_freq_path = frequency;
            inventory.intel_gpu_pci = pci;
        }
        apply(&mut inventory.net_device, self.route);
        apply(&mut inventory.disk_io_device, self.disk_io);
        apply(&mut inventory.has_backlight, self.backlight);
        apply(&mut inventory.has_wifi, self.wifi);
        apply(&mut inventory.battery_mouse_id, self.peripherals.mouse);
        apply(&mut inventory.battery_kbd_id, self.peripherals.keyboard);
        apply(&mut inventory.disk_smart_drives, self.smart);
    }

    fn into_inventory(self) -> HardwareInventory {
        let mut inventory = HardwareInventory::default();
        self.merge_into(&mut inventory);
        inventory
    }
}

fn apply<T>(current: &mut T, outcome: DiscoveryOutcome<T>) -> ReconciliationOutcome {
    match outcome {
        DiscoveryOutcome::Confirmed(value) => {
            *current = value;
            ReconciliationOutcome::Captured
        }
        DiscoveryOutcome::Failed => ReconciliationOutcome::Failed,
    }
}

// ── Hardware discovery ───────────────────────────────────────────────────────

/// One-time startup discovery of static hardware paths and presence flags.
///
/// Mirrors `discover_hardware` in `src/sensors.py`: resolves the CPU/disk/fan
/// hwmon paths, enumerates UPower batteries, detects NVIDIA/Intel GPUs, the
/// default-route net device, the root-disk I/O device, backlight/wifi/turbo
/// presence, and (when SMART is enabled) the SMART-capable drives. Peripheral
/// (mouse/keyboard) UPower ids are discovered via the private
/// `find_peripherals` helper.
///
/// `cpu_count` is supplied by the caller (the daemon resolves it via
/// `available_parallelism`); tests pass a fixed value so discovery stays
/// deterministic.
#[must_use]
pub fn discover_hardware(
    sys_root: &Path,
    proc_root: &Path,
    cfg: &Config,
    dbus: &mut dyn DbusFacade,
    commands: &mut dyn CommandRunner,
    cpu_count: usize,
) -> HardwareInventory {
    discover_hardware_attempt(sys_root, proc_root, cfg, dbus, commands, cpu_count).into_inventory()
}

/// Discovers only bounded local `/proc` and `/sys` inventory for startup or reload.
///
/// Command and D-Bus discovery is intentionally excluded so this inventory can seed the scheduler catalog without delaying the first-paint deadline.
#[must_use]
pub fn discover_local_hardware(
    sys_root: &Path,
    proc_root: &Path,
    cfg: &Config,
    cpu_count: usize,
) -> HardwareInventory {
    discover_local_hardware_attempt(sys_root, proc_root, cfg, cpu_count).into_inventory()
}

/// Performs one bounded local `/proc` and `/sys` inventory attempt for startup or reload merging.
#[must_use]
pub(crate) fn discover_local_hardware_attempt(
    sys_root: &Path,
    proc_root: &Path,
    cfg: &Config,
    cpu_count: usize,
) -> HardwareDiscovery {
    local_hardware_attempt(sys_root, proc_root, cfg, cpu_count)
}

/// Performs one typed inventory attempt for startup or reload merging.
#[must_use]
pub fn discover_hardware_attempt(
    sys_root: &Path,
    proc_root: &Path,
    cfg: &Config,
    dbus: &mut dyn DbusFacade,
    commands: &mut dyn CommandRunner,
    cpu_count: usize,
) -> HardwareDiscovery {
    let mut discovery = local_hardware_attempt(sys_root, proc_root, cfg, cpu_count);
    discovery.system_batteries = DiscoveryOutcome::from_result(power::find_battery_sys(dbus));
    discovery.route = detect_net_device(commands);
    discovery.smart = if cfg.disks.smart {
        smart_discovery_outcome(power::detect_smart_disks(dbus, sys_root), sys_root)
    } else {
        DiscoveryOutcome::Confirmed(BTreeMap::new())
    };
    discovery.peripherals = find_peripherals(cfg, dbus);
    discovery
}

fn local_hardware_attempt(
    sys_root: &Path,
    proc_root: &Path,
    cfg: &Config,
    cpu_count: usize,
) -> HardwareDiscovery {
    let cpu_paths = cpu::discover_cpu_paths(sys_root, &cfg.sensors);
    let hd_temp_paths = disk::find_hd_temp_paths(sys_root, &cfg.sensors);
    let fan_paths = disk::find_fan_speed_paths(sys_root, &cfg.sensors);
    let hwmon_complete = validate_hwmon_enumeration(sys_root).is_ok();
    let has_nvidia = DiscoveryOutcome::from_result(gpu_nvidia::detect_nvidia_outcome(sys_root));
    let intel = DiscoveryOutcome::from_result(gpu_intel::detect_intel_gpu_outcome(sys_root));
    let disk_io_device = DiscoveryOutcome::from_result(disk::detect_disk_io_device_outcome(
        proc_root, sys_root, "/",
    ));
    let has_backlight = DiscoveryOutcome::from_result(detect_has_backlight_outcome(sys_root));
    let has_wifi = DiscoveryOutcome::from_result(network::detect_has_wifi_outcome(sys_root));

    let local = HardwareInventory {
        // `capabilities`/`metrics` are derived per-poll from config inside
        // `collect` (Python's HardwareInfo has no equivalent fields); left
        // empty so the snapshot only carries discovered hardware.
        capabilities: BTreeSet::new(),
        metrics: BTreeSet::new(),
        cpu_temp_path: None,
        cpu_freq_path: None,
        cpu_turbo_path: None,
        hd_temp_paths: BTreeMap::new(),
        fan_paths: BTreeMap::new(),
        battery_sys_ids: Vec::new(),
        has_nvidia: false,
        amd_gpu: None,
        intel_gpu_freq_path: None,
        intel_gpu_pci: None,
        net_device: None,
        disk_io_device: None,
        cpu_count: cpu_count.max(1),
        cpu_turbo_supported: false,
        has_backlight: false,
        has_wifi: false,
        battery_mouse_id: None,
        battery_kbd_id: None,
        disk_smart_drives: BTreeMap::new(),
    };
    HardwareDiscovery {
        local,
        cpu_temperature: completion_outcome(hwmon_complete, cpu_paths.cpu_temp_path),
        cpu_controls: completion_outcome(
            validate_cpu_control_paths(sys_root).is_ok(),
            (
                cpu_paths.cpu_freq_path,
                cpu_paths.cpu_turbo_path,
                cpu_paths.cpu_turbo_supported,
            ),
        ),
        thermal: completion_outcome(hwmon_complete, (hd_temp_paths, fan_paths)),
        system_batteries: DiscoveryOutcome::Failed,
        nvidia: has_nvidia,
        amd: DiscoveryOutcome::from_result(gpu_amd::detect_amd_gpu(sys_root)),
        intel: intel.map(|paths| (paths.freq_path, paths.pci)),
        route: DiscoveryOutcome::Failed,
        disk_io: disk_io_device,
        backlight: has_backlight,
        wifi: has_wifi,
        peripherals: PeripheralOutcomes {
            mouse: DiscoveryOutcome::Failed,
            keyboard: DiscoveryOutcome::Failed,
        },
        smart: DiscoveryOutcome::Failed,
    }
}

/// Reconciles one demanded inventory family, retaining prior values when a boundary or local enumeration fails.
#[must_use]
pub(crate) fn reconcile_inventory_family(
    family: InventoryFamily,
    hw: &mut HardwareInventory,
    sys_root: &Path,
    proc_root: &Path,
    cfg: &Config,
    dbus: &mut dyn DbusFacade,
    commands: &mut dyn CommandRunner,
) -> ReconciliationOutcome {
    match family {
        InventoryFamily::Cpu => {
            let paths = cpu::discover_cpu_paths(sys_root, &cfg.sensors);
            let temperature = apply(
                &mut hw.cpu_temp_path,
                completion_outcome(
                    validate_hwmon_enumeration(sys_root).is_ok(),
                    paths.cpu_temp_path,
                ),
            );
            let controls = if validate_cpu_control_paths(sys_root).is_ok() {
                hw.cpu_freq_path = paths.cpu_freq_path;
                hw.cpu_turbo_path = paths.cpu_turbo_path;
                hw.cpu_turbo_supported = paths.cpu_turbo_supported;
                ReconciliationOutcome::Captured
            } else {
                ReconciliationOutcome::Failed
            };
            temperature.and(controls)
        }
        InventoryFamily::Thermal => {
            let temperatures = disk::find_hd_temp_paths(sys_root, &cfg.sensors);
            let fans = disk::find_fan_speed_paths(sys_root, &cfg.sensors);
            if validate_hwmon_enumeration(sys_root).is_ok() {
                hw.hd_temp_paths = temperatures;
                hw.fan_paths = fans;
                ReconciliationOutcome::Captured
            } else {
                ReconciliationOutcome::Failed
            }
        }
        InventoryFamily::SystemBattery => apply(
            &mut hw.battery_sys_ids,
            DiscoveryOutcome::from_result(power::find_battery_sys(dbus)),
        ),
        InventoryFamily::Smart => {
            let outcome = if cfg.disks.smart {
                smart_discovery_outcome(power::detect_smart_disks(dbus, sys_root), sys_root)
            } else {
                DiscoveryOutcome::Confirmed(BTreeMap::new())
            };
            apply(&mut hw.disk_smart_drives, outcome)
        }
        InventoryFamily::Nvidia => apply(
            &mut hw.has_nvidia,
            DiscoveryOutcome::from_result(gpu_nvidia::detect_nvidia_outcome(sys_root)),
        ),
        InventoryFamily::Amd => apply(
            &mut hw.amd_gpu,
            DiscoveryOutcome::from_result(gpu_amd::detect_amd_gpu(sys_root)),
        ),
        InventoryFamily::Intel => match gpu_intel::detect_intel_gpu_outcome(sys_root) {
            Ok(paths) => {
                hw.intel_gpu_freq_path = paths.freq_path;
                hw.intel_gpu_pci = paths.pci;
                ReconciliationOutcome::Captured
            }
            Err(_) => ReconciliationOutcome::Failed,
        },
        InventoryFamily::Backlight => apply(
            &mut hw.has_backlight,
            DiscoveryOutcome::from_result(detect_has_backlight_outcome(sys_root)),
        ),
        InventoryFamily::Network => {
            let route = apply(&mut hw.net_device, detect_net_device(commands));
            let wifi = apply(
                &mut hw.has_wifi,
                DiscoveryOutcome::from_result(network::detect_has_wifi_outcome(sys_root)),
            );
            route.and(wifi)
        }
        InventoryFamily::DiskIo => apply(
            &mut hw.disk_io_device,
            DiscoveryOutcome::from_result(disk::detect_disk_io_device_outcome(
                proc_root, sys_root, "/",
            )),
        ),
        InventoryFamily::Peripheral => {
            let peripherals = find_peripherals(cfg, dbus);
            let mouse = apply(&mut hw.battery_mouse_id, peripherals.mouse);
            let keyboard = apply(&mut hw.battery_kbd_id, peripherals.keyboard);
            mouse.and(keyboard)
        }
    }
}

/// Retry discovery of hardware that can appear after startup: UPower
/// peripherals (mouse/keyboard) and the default-route net device.
///
/// Confirmed enumeration replaces peripheral ids, including confirmed absence;
/// boundary failures retain the previous ids. The net device is retried only
/// while still `None` (the daemon started before the network came up). Rescan scheduling state belongs to the pure scheduler, not the inventory.
pub fn rescan_peripherals(
    hw: &mut HardwareInventory,
    cfg: &Config,
    dbus: &mut dyn DbusFacade,
    commands: &mut dyn CommandRunner,
) {
    let peripherals = find_peripherals(cfg, dbus);
    apply(&mut hw.battery_mouse_id, peripherals.mouse);
    apply(&mut hw.battery_kbd_id, peripherals.keyboard);
    if hw.net_device.is_none() {
        apply(&mut hw.net_device, detect_net_device(commands));
    }
}

/// Returns `true` when a configured item still wants a peripheral that has not
/// appeared yet (mouse/keyboard UPower id, or the default-route net device).
///
/// Mirrors `needs_periph_rescan` in `src/sensors.py`. Bolt-configured devices
/// never need UPower discovery (they are addressed by index, not enumerated).
#[must_use]
pub fn needs_periph_rescan(hw: &HardwareInventory, cfg: &Config) -> bool {
    let wants_mouse = cfg.panel.has("battery_mouse") || cfg.tooltip.has("battery_mouse");
    let wants_kbd = cfg.panel.has("battery_kbd") || cfg.tooltip.has("battery_kbd");
    if wants_mouse && hw.battery_mouse_id.is_none() && cfg.battery.mouse_bolt.is_none() {
        return true;
    }
    if wants_kbd && hw.battery_kbd_id.is_none() && cfg.battery.kbd_bolt.is_none() {
        return true;
    }
    if hw.net_device.is_none()
        && NET_GATED_ITEMS
            .iter()
            .any(|name| cfg.panel.has(name) || cfg.tooltip.has(name))
    {
        return true;
    }
    false
}

// ── Hardware-inventory discovery readers ────────────────────────────────────

/// Default-route net device via `ip route get` / `ip route show default`.
pub(crate) fn detect_net_device(
    commands: &mut dyn CommandRunner,
) -> DiscoveryOutcome<Option<String>> {
    for args in [["route", "get", "8.8.8.8"], ["route", "show", "default"]] {
        let argv = args.map(std::ffi::OsString::from);
        match commands.run(Path::new("ip"), &argv, NETWORK_COMMAND_TIMEOUT) {
            Ok(output) if output.status == CommandStatus::Exit(0) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(device) = network::route_device(&stdout) {
                    return DiscoveryOutcome::Confirmed(Some(device.to_owned()));
                }
                if args[1] == "show" && stdout.trim().is_empty() {
                    return DiscoveryOutcome::Confirmed(None);
                }
                if args[1] == "show" {
                    return DiscoveryOutcome::Failed;
                }
            }
            Ok(_) | Err(_) => {
                if args[1] == "show" {
                    return DiscoveryOutcome::Failed;
                }
            }
        }
    }
    DiscoveryOutcome::Failed
}

fn completion_outcome<T>(completed: bool, value: T) -> DiscoveryOutcome<T> {
    if completed {
        DiscoveryOutcome::Confirmed(value)
    } else {
        DiscoveryOutcome::Failed
    }
}

fn smart_discovery_outcome<E>(
    result: Result<BTreeMap<String, SmartDisk>, E>,
    sys_root: &Path,
) -> DiscoveryOutcome<BTreeMap<String, SmartDisk>> {
    let Ok(mut disks) = result else {
        return DiscoveryOutcome::Failed;
    };
    for (label, drive) in &mut disks {
        let Ok(rotational) = disk::is_rotational_outcome(sys_root, label) else {
            return DiscoveryOutcome::Failed;
        };
        drive.rotational = rotational;
    }
    DiscoveryOutcome::Confirmed(disks)
}

fn validate_hwmon_enumeration(sys_root: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(sys_root.join("class/hwmon"))? {
        let path = entry?.path();
        std::fs::canonicalize(&path)?;
        std::fs::read_to_string(path.join("name"))?;
    }
    Ok(())
}

fn validate_cpu_control_paths(sys_root: &Path) -> io::Result<()> {
    std::fs::read_dir(sys_root.join("devices/system/cpu"))?.collect::<io::Result<Vec<_>>>()?;
    for path in [
        sys_root.join("devices/system/cpu/cpu0/cpufreq/scaling_cur_freq"),
        sys_root.join("devices/system/cpu/intel_pstate/no_turbo"),
        sys_root.join("devices/system/cpu/cpufreq/boost"),
    ] {
        let _ = path.try_exists()?;
    }
    Ok(())
}

/// A backlight device exposing both `brightness` and `max_brightness`.
///
/// Mirrors `_detect_has_backlight` in `src/sensors.py`. Desktops/VMs have none
/// → `false`, gating `screen_brightness` off.
fn detect_has_backlight_outcome(sys_root: &Path) -> io::Result<bool> {
    let entries = std::fs::read_dir(sys_root.join("class/backlight"))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let brightness = path.join("brightness").try_exists()?;
        let maximum = path.join("max_brightness").try_exists()?;
        if brightness && maximum {
            return Ok(true);
        }
    }
    Ok(false)
}

// ── Peripheral hardware-inventory discovery ─────────────────────────────────

/// UPower type enum values used by the peripheral classifier.
const UPOWER_TYPE_MOUSE: u32 = 5;
/// UPower keyboard device type.
const UPOWER_TYPE_KEYBOARD: u32 = 6;

/// Discovers Logitech hidpp battery UPower paths for the mouse/keyboard.
///
/// Mirrors `_find_peripherals` in `src/sensors.py`: manual `[battery]` Unifying
/// overrides win; otherwise each `/battery_hidpp` path is classified by UPower
/// `Type` (mouse=5, keyboard=6) with model-name heuristics as a fallback for
/// devices that report `Type=0`. Returns `(mouse_id, kbd_id)`.
fn find_peripherals(cfg: &Config, dbus: &mut dyn DbusFacade) -> PeripheralOutcomes {
    let mut mouse = cfg.battery.mouse_unifying.clone();
    let mut kbd = cfg.battery.kbd_unifying.clone();
    if mouse.is_some() && kbd.is_some() {
        return PeripheralOutcomes {
            mouse: DiscoveryOutcome::Confirmed(mouse),
            keyboard: DiscoveryOutcome::Confirmed(kbd),
        };
    }

    let Ok(paths) = power::upower_enumerate(dbus) else {
        return PeripheralOutcomes {
            mouse: mouse.map_or(DiscoveryOutcome::Failed, |id| {
                DiscoveryOutcome::Confirmed(Some(id))
            }),
            keyboard: kbd.map_or(DiscoveryOutcome::Failed, |id| {
                DiscoveryOutcome::Confirmed(Some(id))
            }),
        };
    };
    let hidpp: Vec<String> = paths
        .into_iter()
        .filter(|path| path.contains("/battery_hidpp"))
        .collect();
    let mut property_failed = false;
    for path in hidpp {
        if mouse.is_some() && kbd.is_some() {
            break;
        }
        let props = match power::upower_device_props_result(dbus, &path) {
            Ok(props) => props,
            Err(_) => {
                property_failed = true;
                continue;
            }
        };
        let model = props.model.as_deref().unwrap_or("").to_ascii_lowercase();
        let kind = props.kind;

        let is_kbd = kind == Some(UPOWER_TYPE_KEYBOARD)
            || ["keyboard", "keys", "ergo"]
                .iter()
                .any(|w| model.contains(w))
            || ["k4", "k8", "mx keys"].iter().any(|p| model.starts_with(p));
        let is_mouse = kind == Some(UPOWER_TYPE_MOUSE)
            || model.contains("mouse")
            || model.contains("master")
            || model.contains("mx m")
            || model.contains("trackball");

        if is_kbd && kbd.is_none() {
            kbd = Some(path);
        } else if is_mouse && mouse.is_none() {
            mouse = Some(path);
        }
    }
    PeripheralOutcomes {
        mouse: if mouse.is_some() || !property_failed {
            DiscoveryOutcome::Confirmed(mouse)
        } else {
            DiscoveryOutcome::Failed
        },
        keyboard: if kbd.is_some() || !property_failed {
            DiscoveryOutcome::Confirmed(kbd)
        } else {
            DiscoveryOutcome::Failed
        },
    }
}
