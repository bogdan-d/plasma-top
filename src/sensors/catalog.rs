use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;
use std::time::Duration;

use crate::config::{Config, Mounts, Surface};
use crate::domain::form::Form;
use crate::domain::item::ItemToken;
use crate::domain::metric::{Capability, Metric};
use crate::domain::readings::{HardwareInventory, InventoryFamily};
use crate::domain::registry::needed_capabilities;
use crate::scheduler::{
    ConfigGeneration, DemandPlan, JobId, JobKind, JobSpec, OwnerId, PageId, PeripheralRole,
    PeripheralSource, SchedulerConfig, SourceIdentity, TimingClass,
};

use super::{disk, gpu_intel, gpu_nvidia, network, power, process};

pub(super) fn configured_capabilities(cfg: &Config) -> BTreeSet<Capability> {
    let items = cfg
        .panel
        .sections
        .iter()
        .chain(cfg.tooltip.sections.iter())
        .flat_map(|section| section.items.iter())
        .filter_map(|token| ItemToken::from_str(token).ok());
    needed_capabilities(
        items,
        notification_flags(cfg).into_iter(),
        cfg.pages.order.iter().map(String::as_str),
    )
}

fn notification_flags(cfg: &Config) -> Vec<&'static str> {
    let n = &cfg.notifications;
    [
        (n.cpu_temp, "cpu_temp"),
        (n.gpu_nvidia_temp, "gpu_nvidia_temp"),
        (n.disk_usage, "disk_usage"),
        (n.disk_smart, "disk_smart"),
        (n.hd_temp, "hd_temp"),
        (n.battery_sys, "battery_sys"),
        (n.battery_mouse, "battery_mouse"),
        (n.battery_kbd, "battery_kbd"),
        (n.load_avg, "load_avg"),
        (n.server_check, "server_check"),
    ]
    .into_iter()
    .filter_map(|(enabled, name)| enabled.then_some(name))
    .collect()
}

pub(crate) fn scheduler_config(
    cfg: &Config,
    hw: &HardwareInventory,
    confirmed_auto_mounts: &[String],
    generation: ConfigGeneration,
) -> SchedulerConfig {
    let mut catalog = Catalog::new(cfg, hw, confirmed_auto_mounts);
    let mut demand = DemandPlan::default();
    let panel_inventory = catalog.surface_inventory_families(&cfg.panel);
    let tooltip_inventory = catalog.surface_inventory_families(&cfg.tooltip);
    let notification_inventory = catalog.notification_inventory_families();
    let panel_jobs = catalog.surface_jobs(&cfg.panel);
    let mut panel_startup_jobs = panel_jobs.clone();
    demand.hidden.extend(panel_jobs.iter().cloned());
    demand
        .main_tooltip
        .extend(catalog.surface_jobs(&cfg.tooltip));
    demand.hidden.extend(catalog.notification_jobs());

    let graphs_enabled = cfg.pages.order.iter().any(|page| page == "graphs");
    if graphs_enabled {
        demand.hidden.extend(catalog.graph_jobs());
    }
    for family in panel_inventory {
        let inventory = catalog.inventory_job(family);
        panel_startup_jobs.insert(inventory.clone());
        demand.hidden.insert(inventory);
    }
    let mut hidden_inventory = notification_inventory;
    if graphs_enabled {
        hidden_inventory.extend([
            InventoryFamily::Nvidia,
            InventoryFamily::Intel,
            InventoryFamily::Network,
        ]);
    }
    for family in hidden_inventory {
        demand.hidden.insert(catalog.inventory_job(family));
    }
    for family in tooltip_inventory {
        demand.main_tooltip.insert(catalog.inventory_job(family));
    }
    for page in &cfg.pages.order {
        let page_id = PageId::from_id(page);
        let jobs = catalog.page_jobs(&page_id);
        if !jobs.is_empty() {
            demand.pages.insert(page_id, jobs);
        }
    }
    let wants_disk_usage = demand
        .hidden
        .iter()
        .chain(&demand.main_tooltip)
        .chain(demand.pages.values().flatten())
        .any(|job| job.kind == JobKind::DiskUsage);
    if wants_disk_usage && matches!(cfg.disks.mounts, Mounts::Auto) {
        let inventory = catalog.periodic_singleton(
            OwnerId::Discovery,
            JobKind::MountInventory,
            cfg.display.poll_interval.duration(),
        );
        if demand
            .hidden
            .iter()
            .any(|job| job.kind == JobKind::DiskUsage)
        {
            demand.hidden.insert(inventory.clone());
        }
        if panel_jobs.iter().any(|job| job.kind == JobKind::DiskUsage) {
            panel_startup_jobs.insert(inventory.clone());
        }
        if demand
            .main_tooltip
            .iter()
            .any(|job| job.kind == JobKind::DiskUsage)
        {
            demand.main_tooltip.insert(inventory.clone());
        }
        for jobs in demand.pages.values_mut() {
            if jobs.iter().any(|job| job.kind == JobKind::DiskUsage) {
                jobs.insert(inventory.clone());
            }
        }
    }
    for job in panel_startup_jobs {
        if let Some(spec) = catalog.specs.get_mut(&job) {
            spec.startup_panel = spec.timing != TimingClass::History;
        }
    }

    SchedulerConfig {
        generation,
        display_interval: cfg.display.poll_interval.duration(),
        jobs: catalog.specs.into_values().collect(),
        demand,
    }
}

struct Catalog<'a> {
    cfg: &'a Config,
    hw: &'a HardwareInventory,
    confirmed_auto_mounts: &'a [String],
    capabilities: BTreeSet<Capability>,
    specs: BTreeMap<JobId, JobSpec>,
}

impl<'a> Catalog<'a> {
    fn new(
        cfg: &'a Config,
        hw: &'a HardwareInventory,
        confirmed_auto_mounts: &'a [String],
    ) -> Self {
        Self {
            cfg,
            hw,
            confirmed_auto_mounts,
            capabilities: configured_capabilities(cfg),
            specs: BTreeMap::new(),
        }
    }

    fn surface_jobs(&mut self, surface: &Surface) -> BTreeSet<JobId> {
        surface
            .sections
            .iter()
            .flat_map(|section| &section.items)
            .filter_map(|token| ItemToken::from_str(token).ok())
            .flat_map(|item| self.item_jobs(item))
            .collect()
    }

    fn item_jobs(&mut self, item: ItemToken) -> Vec<JobId> {
        let metric = item.metric();
        match metric {
            Metric::CpuUsage => {
                let source = self.cpu_job();
                let mut jobs = vec![source.clone()];
                if item.form().is_some_and(Form::renders_history) {
                    jobs.push(self.cpu_history(source));
                }
                jobs
            }
            Metric::MemUsage => {
                let source = self.memory_job();
                let mut jobs = vec![source.clone()];
                if item.form().is_some_and(Form::renders_history) {
                    jobs.push(self.memory_history(source));
                }
                jobs
            }
            _ => self.metric_jobs(metric),
        }
    }

    fn surface_inventory_families(&self, surface: &Surface) -> BTreeSet<InventoryFamily> {
        surface
            .sections
            .iter()
            .flat_map(|section| &section.items)
            .filter_map(|token| ItemToken::from_str(token).ok())
            .filter_map(|item| self.metric_inventory_family(item.metric()))
            .collect()
    }

    fn notification_inventory_families(&self) -> BTreeSet<InventoryFamily> {
        notification_flags(self.cfg)
            .into_iter()
            .filter_map(|name| name.parse::<Metric>().ok())
            .filter_map(|metric| self.metric_inventory_family(metric))
            .collect()
    }

    fn metric_inventory_family(&self, metric: Metric) -> Option<InventoryFamily> {
        match metric {
            Metric::CpuFreq | Metric::CpuTurbo | Metric::CpuTemp => Some(InventoryFamily::Cpu),
            Metric::HdTemp | Metric::FanSpeed => Some(InventoryFamily::Thermal),
            Metric::DiskSmart if self.cfg.disks.smart => Some(InventoryFamily::Smart),
            Metric::GpuNvidiaTemp
            | Metric::GpuNvidiaUsage
            | Metric::GpuNvidiaMemUsage
            | Metric::GpuNvidiaDecoderUsage
            | Metric::GpuNvidiaFanSpeed => Some(InventoryFamily::Nvidia),
            Metric::GpuIntelFreq | Metric::GpuIntelUsage | Metric::GpuIntelDecoderUsage => {
                Some(InventoryFamily::Intel)
            }
            Metric::GpuAmdUsage
            | Metric::GpuAmdCodecUsage
            | Metric::GpuAmdMemUsage
            | Metric::GpuAmdFreq
            | Metric::GpuAmdTemp
            | Metric::GpuAmdPower
            | Metric::GpuAmdFanSpeed => None,
            Metric::ScreenBrightness => Some(InventoryFamily::Backlight),
            Metric::BatterySystem => Some(InventoryFamily::SystemBattery),
            Metric::BatteryMouse
                if self.cfg.battery.mouse_unifying.is_none()
                    && self.cfg.battery.mouse_bolt.is_none() =>
            {
                Some(InventoryFamily::Peripheral)
            }
            Metric::BatteryKeyboard
                if self.cfg.battery.kbd_unifying.is_none()
                    && self.cfg.battery.kbd_bolt.is_none() =>
            {
                Some(InventoryFamily::Peripheral)
            }
            Metric::NetSpeed
            | Metric::NetDevice
            | Metric::NetIp
            | Metric::NetDeviceIp
            | Metric::WifiSsid
            | Metric::WifiSignal
            | Metric::WifiSsidSignal => Some(InventoryFamily::Network),
            Metric::DiskIo => Some(InventoryFamily::DiskIo),
            Metric::CpuUsage
            | Metric::MemUsage
            | Metric::SwapUsage
            | Metric::DiskUsage
            | Metric::DiskSmart
            | Metric::TopProcess
            | Metric::Uptime
            | Metric::LoadAverage
            | Metric::SystemUpdates
            | Metric::ServerCheck
            | Metric::BatteryMouse
            | Metric::BatteryKeyboard => None,
        }
    }

    fn inventory_job(&mut self, family: InventoryFamily) -> JobId {
        let freshness = match family {
            InventoryFamily::SystemBattery => power::BAT_CACHE_TTL,
            InventoryFamily::Peripheral => power::PERIPH_CACHE_TTL,
            InventoryFamily::Cpu
            | InventoryFamily::Thermal
            | InventoryFamily::Smart
            | InventoryFamily::Nvidia
            | InventoryFamily::Intel
            | InventoryFamily::Backlight
            | InventoryFamily::Network
            | InventoryFamily::DiskIo => Duration::from_secs(60),
        };
        let id = JobId::with_source(
            OwnerId::Discovery,
            JobKind::HardwareDiscovery,
            SourceIdentity::Inventory(family),
        );
        self.insert(JobSpec::periodic(id, freshness))
    }

    fn metric_jobs(&mut self, metric: Metric) -> Vec<JobId> {
        match metric {
            Metric::CpuUsage => vec![self.cpu_job()],
            Metric::MemUsage => vec![self.memory_job()],
            Metric::SwapUsage => vec![self.memory_job()],
            Metric::CpuFreq
            | Metric::CpuTurbo
            | Metric::CpuTemp
            | Metric::Uptime
            | Metric::LoadAverage => vec![self.cpu_job()],
            Metric::HdTemp => self.disk_temperature_jobs(),
            Metric::DiskUsage => self.disk_usage_jobs(),
            Metric::DiskSmart => self.smart_jobs(),
            Metric::GpuNvidiaTemp
            | Metric::GpuNvidiaUsage
            | Metric::GpuNvidiaMemUsage
            | Metric::GpuNvidiaDecoderUsage
            | Metric::GpuNvidiaFanSpeed => self.nvidia_jobs(),
            // AMDGPU collection is connected by the scheduler integration ticket.
            Metric::GpuAmdUsage
            | Metric::GpuAmdCodecUsage
            | Metric::GpuAmdMemUsage
            | Metric::GpuAmdFreq
            | Metric::GpuAmdTemp
            | Metric::GpuAmdPower
            | Metric::GpuAmdFanSpeed => Vec::new(),
            Metric::GpuIntelFreq => self.intel_frequency_jobs(),
            Metric::GpuIntelUsage | Metric::GpuIntelDecoderUsage => self.intel_usage_jobs(),
            Metric::ScreenBrightness => self.brightness_jobs(),
            Metric::FanSpeed => self.fan_jobs(),
            Metric::BatterySystem => self.system_battery_jobs(),
            Metric::BatteryMouse => self.peripheral_jobs(PeripheralRole::Mouse),
            Metric::BatteryKeyboard => self.peripheral_jobs(PeripheralRole::Keyboard),
            Metric::NetSpeed => {
                let Some(source) = self.network_rate_job() else {
                    return Vec::new();
                };
                vec![source]
            }
            Metric::DiskIo => self.disk_io_jobs(),
            Metric::NetDevice
            | Metric::NetIp
            | Metric::NetDeviceIp
            | Metric::WifiSsid
            | Metric::WifiSignal
            | Metric::WifiSsidSignal => vec![self.periodic_singleton(
                OwnerId::Network,
                JobKind::NetworkIdentity,
                network::NET_INFO_TTL,
            )],
            Metric::TopProcess => {
                let id = self.periodic_singleton(
                    OwnerId::Process,
                    JobKind::PanelProcesses,
                    process::TOP_PROCESS_TTL,
                );
                vec![self.mark_counter(id)]
            }
            Metric::SystemUpdates => {
                self.external_file_job(JobKind::UpdatesFile, &self.cfg.system_updates.file)
            }
            Metric::ServerCheck => {
                self.external_file_job(JobKind::ServerFile, &self.cfg.server_check.file)
            }
        }
    }

    fn notification_jobs(&mut self) -> BTreeSet<JobId> {
        let notifications = &self.cfg.notifications;
        let capabilities = [
            (notifications.cpu_temp, Capability::CpuTemperature),
            (notifications.gpu_nvidia_temp, Capability::GpuNvidia),
            (notifications.disk_usage, Capability::DiskUsage),
            (notifications.disk_smart, Capability::DiskSmart),
            (notifications.hd_temp, Capability::DiskTemperature),
            (notifications.battery_sys, Capability::BatterySystem),
            (notifications.battery_mouse, Capability::BatteryMouse),
            (notifications.battery_kbd, Capability::BatteryKeyboard),
            (notifications.load_avg, Capability::LoadAverage),
            (notifications.server_check, Capability::ServerCheck),
        ];
        capabilities
            .into_iter()
            .filter(|(enabled, _)| *enabled)
            .flat_map(|(_, capability)| self.capability_jobs(capability))
            .collect()
    }

    fn capability_jobs(&mut self, capability: Capability) -> Vec<JobId> {
        match capability {
            Capability::SwapUsage => vec![self.memory_job()],
            Capability::CpuFrequency
            | Capability::CpuTurbo
            | Capability::CpuTemperature
            | Capability::Uptime
            | Capability::LoadAverage => vec![self.cpu_job()],
            Capability::DiskTemperature => self.disk_temperature_jobs(),
            Capability::DiskUsage => self.disk_usage_jobs(),
            Capability::DiskSmart => self.smart_jobs(),
            Capability::GpuNvidia => self.nvidia_jobs(),
            // AMDGPU collection is connected by the scheduler integration ticket.
            Capability::GpuAmdUsage
            | Capability::GpuAmdCodec
            | Capability::GpuAmdMemory
            | Capability::GpuAmdFrequency
            | Capability::GpuAmdTemperature
            | Capability::GpuAmdPower
            | Capability::GpuAmdFanSpeed => Vec::new(),
            Capability::GpuIntelFrequency => self.intel_frequency_jobs(),
            Capability::GpuIntelUsage | Capability::GpuIntelDecoder => self.intel_usage_jobs(),
            Capability::ScreenBrightness => self.brightness_jobs(),
            Capability::FanSpeed => self.fan_jobs(),
            Capability::BatterySystem => self.system_battery_jobs(),
            Capability::BatteryMouse => self.peripheral_jobs(PeripheralRole::Mouse),
            Capability::BatteryKeyboard => self.peripheral_jobs(PeripheralRole::Keyboard),
            Capability::NetworkSpeed => self.network_rate_job().into_iter().collect(),
            Capability::DiskIo => self.disk_io_jobs(),
            Capability::NetworkInfo => vec![self.periodic_singleton(
                OwnerId::Network,
                JobKind::NetworkIdentity,
                network::NET_INFO_TTL,
            )],
            Capability::TopProcess => {
                let id = self.periodic_singleton(
                    OwnerId::Process,
                    JobKind::PanelProcesses,
                    process::TOP_PROCESS_TTL,
                );
                vec![self.mark_counter(id)]
            }
            Capability::SystemUpdates => {
                self.external_file_job(JobKind::UpdatesFile, &self.cfg.system_updates.file)
            }
            Capability::ServerCheck => {
                self.external_file_job(JobKind::ServerFile, &self.cfg.server_check.file)
            }
        }
    }

    fn graph_jobs(&mut self) -> BTreeSet<JobId> {
        let cpu = self.cpu_job();
        let memory = self.memory_job();
        let mut jobs = BTreeSet::from([
            cpu.clone(),
            self.cpu_history(cpu),
            memory.clone(),
            self.memory_history(memory),
        ]);
        if let Some(network) = self.network_rate_job() {
            jobs.insert(network.clone());
            jobs.insert(self.network_history(network));
        }
        let gpu_sources = if self.hw.has_nvidia {
            self.nvidia_jobs()
        } else {
            self.intel_usage_jobs()
        };
        jobs.extend(gpu_sources);
        let gpu_source = self
            .hw
            .has_nvidia
            .then(|| String::from("nvidia"))
            .or_else(|| {
                self.hw
                    .intel_gpu_pci
                    .as_ref()
                    .map(|pci| format!("intel:{pci}"))
            });
        if let Some(source) = gpu_source {
            jobs.insert(self.insert(JobSpec::source_history(
                JobId::with_source(
                    OwnerId::GpuHistory,
                    JobKind::GpuHistory,
                    SourceIdentity::Device(source),
                ),
                self.cfg.display.history_interval.duration(),
            )));
        }
        jobs
    }

    fn brightness_jobs(&mut self) -> Vec<JobId> {
        if !self.hw.has_backlight {
            return Vec::new();
        }
        vec![self.fast_singleton(OwnerId::External, JobKind::Brightness)]
    }

    fn page_jobs(&mut self, page: &PageId) -> BTreeSet<JobId> {
        match page {
            PageId::Processes => {
                let id = self.fast_singleton(OwnerId::Process, JobKind::PageProcesses);
                BTreeSet::from([self.mark_counter(id)])
            }
            PageId::CpuCores => {
                let cores = self.fast_singleton(OwnerId::Cpu, JobKind::CpuCores);
                let cores = self.mark_counter(cores);
                let history = self.insert(JobSpec::history(
                    JobId::singleton(OwnerId::Cpu, JobKind::CpuCoreHistory),
                    cores.clone(),
                    self.cfg.display.history_interval.duration(),
                ));
                BTreeSet::from([cores, history])
            }
            PageId::Connections => BTreeSet::from([self.fast_page_command(PageId::Connections)]),
            PageId::Fastfetch => {
                BTreeSet::from([self.page_command(PageId::Fastfetch, Duration::from_secs(30))])
            }
            PageId::Graphs => {
                BTreeSet::from([self.fast_singleton(OwnerId::Page, JobKind::PageRender)])
            }
            PageId::Main | PageId::Other(_) => BTreeSet::new(),
        }
    }

    fn cpu_job(&mut self) -> JobId {
        let id = JobId::with_source(
            OwnerId::Cpu,
            JobKind::Cpu,
            SourceIdentity::CpuSources {
                temperature: self
                    .capabilities
                    .contains(&Capability::CpuTemperature)
                    .then(|| self.hw.cpu_temp_path.clone())
                    .flatten(),
                frequency: self
                    .capabilities
                    .contains(&Capability::CpuFrequency)
                    .then(|| self.hw.cpu_freq_path.clone())
                    .flatten(),
                turbo: (self.capabilities.contains(&Capability::CpuTurbo)
                    && self.hw.cpu_turbo_supported)
                    .then(|| self.hw.cpu_turbo_path.clone())
                    .flatten(),
            },
        );
        let id = self.insert(JobSpec::fast(id, self.cfg.display.poll_interval.duration()));
        if let Some(spec) = self.specs.get_mut(&id) {
            spec.counter = true;
        }
        id
    }

    fn cpu_history(&mut self, source: JobId) -> JobId {
        self.insert(JobSpec::history(
            JobId::singleton(OwnerId::Cpu, JobKind::CpuHistory),
            source,
            self.cfg.display.history_interval.duration(),
        ))
    }

    fn memory_job(&mut self) -> JobId {
        self.fast_singleton(OwnerId::Memory, JobKind::Memory)
    }

    fn memory_history(&mut self, source: JobId) -> JobId {
        self.insert(JobSpec::history(
            JobId::singleton(OwnerId::Memory, JobKind::MemoryHistory),
            source,
            self.cfg.display.history_interval.duration(),
        ))
    }

    fn network_rate_job(&mut self) -> Option<JobId> {
        let device = self.hw.net_device.clone()?;
        let id = JobId::with_source(
            OwnerId::Network,
            JobKind::NetworkRate,
            SourceIdentity::Device(device),
        );
        let mut spec = JobSpec::fast(id.clone(), self.cfg.display.poll_interval.duration());
        spec.counter = true;
        Some(self.insert(spec))
    }

    fn network_history(&mut self, source: JobId) -> JobId {
        self.insert(JobSpec::history(
            JobId::singleton(OwnerId::Network, JobKind::NetworkHistory),
            source,
            self.cfg.display.history_interval.duration(),
        ))
    }

    fn disk_io_jobs(&mut self) -> Vec<JobId> {
        let Some(device) = self.hw.disk_io_device.clone() else {
            return Vec::new();
        };
        let id = JobId::with_source(
            OwnerId::Disk,
            JobKind::DiskIo,
            SourceIdentity::Device(device),
        );
        let mut spec = JobSpec::fast(id, self.cfg.display.poll_interval.duration());
        spec.counter = true;
        vec![self.insert(spec)]
    }

    fn disk_usage_jobs(&mut self) -> Vec<JobId> {
        let mounts = match &self.cfg.disks.mounts {
            Mounts::Explicit(mounts) => mounts.as_slice(),
            Mounts::Auto => self.confirmed_auto_mounts,
        };
        mounts
            .iter()
            .map(|mount| {
                let id = JobId::with_source(
                    OwnerId::Disk,
                    JobKind::DiskUsage,
                    SourceIdentity::Mount(mount.into()),
                );
                self.insert(JobSpec::fast(id, self.cfg.display.poll_interval.duration()))
            })
            .collect()
    }

    fn smart_jobs(&mut self) -> Vec<JobId> {
        if !self.cfg.disks.smart {
            return Vec::new();
        }
        self.hw
            .disk_smart_drives
            .iter()
            .map(|(label, drive)| {
                let cadence = if drive.rotational {
                    self.cfg.disks.smart_interval_hdd.duration()
                } else {
                    self.cfg.disks.smart_interval.duration()
                };
                let id = JobId::with_source(
                    OwnerId::Disk,
                    JobKind::Smart,
                    SourceIdentity::SmartDrive {
                        label: label.clone(),
                        object_path: drive.object_path.clone(),
                    },
                );
                self.insert(JobSpec::periodic(id, cadence))
            })
            .collect()
    }

    fn disk_temperature_jobs(&mut self) -> Vec<JobId> {
        self.hw
            .hd_temp_paths
            .iter()
            .map(|(name, path)| {
                let id = JobId::with_source(
                    OwnerId::Disk,
                    JobKind::DiskTemperature,
                    SourceIdentity::NamedPath {
                        name: name.clone(),
                        path: path.clone(),
                    },
                );
                self.insert(JobSpec::periodic(id, disk::HD_TEMP_CACHE_TTL))
            })
            .collect()
    }

    fn fan_jobs(&mut self) -> Vec<JobId> {
        self.hw
            .fan_paths
            .iter()
            .map(|(name, path)| {
                let id = JobId::with_source(
                    OwnerId::Disk,
                    JobKind::FanSpeed,
                    SourceIdentity::NamedPath {
                        name: name.clone(),
                        path: path.clone(),
                    },
                );
                self.insert(JobSpec::periodic(id, disk::FAN_SPEED_CACHE_TTL))
            })
            .collect()
    }

    fn system_battery_jobs(&mut self) -> Vec<JobId> {
        self.hw
            .battery_sys_ids
            .iter()
            .map(|id| {
                let job = JobId::with_source(
                    OwnerId::Power,
                    JobKind::SystemBattery,
                    SourceIdentity::Battery(id.clone()),
                );
                self.insert(JobSpec::periodic(job, power::BAT_CACHE_TTL))
            })
            .collect()
    }

    fn peripheral_jobs(&mut self, role: PeripheralRole) -> Vec<JobId> {
        let Some(source) = power::resolve_peripheral_source(self.cfg, self.hw, role) else {
            return Vec::new();
        };
        let freshness = match source {
            PeripheralSource::Upower(_) => power::PERIPH_CACHE_TTL,
            PeripheralSource::Bolt(_) => power::BOLT_CACHE_TTL,
        };
        let id = JobId::with_source(
            OwnerId::Power,
            JobKind::PeripheralBattery,
            SourceIdentity::Peripheral { role, source },
        );
        vec![self.insert(JobSpec::periodic(id, freshness))]
    }

    fn nvidia_jobs(&mut self) -> Vec<JobId> {
        if !self.hw.has_nvidia {
            return Vec::new();
        }
        vec![
            self.fast_singleton(OwnerId::Nvidia, JobKind::NvidiaNvml),
            self.periodic_singleton(
                OwnerId::Nvidia,
                JobKind::NvidiaFallback,
                gpu_nvidia::GPU_CACHE_TTL,
            ),
        ]
    }

    fn intel_frequency_jobs(&mut self) -> Vec<JobId> {
        let Some(path) = self.hw.intel_gpu_freq_path.clone() else {
            return Vec::new();
        };
        let id = JobId::with_source(
            OwnerId::IntelGpu,
            JobKind::IntelFrequency,
            SourceIdentity::Path(path),
        );
        vec![self.insert(JobSpec::fast(id, self.cfg.display.poll_interval.duration()))]
    }

    fn intel_usage_jobs(&mut self) -> Vec<JobId> {
        let Some(pci) = self.hw.intel_gpu_pci.clone() else {
            return Vec::new();
        };
        let id = JobId::with_source(
            OwnerId::IntelGpu,
            JobKind::IntelUsage,
            SourceIdentity::Device(pci),
        );
        let mut spec = JobSpec::periodic(id, gpu_intel::INTEL_GPU_USAGE_TTL);
        spec.counter = true;
        vec![self.insert(spec)]
    }

    fn external_file_job(&mut self, kind: JobKind, path: &str) -> Vec<JobId> {
        if path.is_empty() {
            return Vec::new();
        }
        let id = JobId::with_source(OwnerId::External, kind, SourceIdentity::Path(path.into()));
        vec![self.insert(JobSpec::triggered(
            id,
            self.cfg.display.poll_interval.duration(),
        ))]
    }

    fn fast_singleton(&mut self, owner: OwnerId, kind: JobKind) -> JobId {
        let id = JobId::singleton(owner, kind);
        self.insert(JobSpec::fast(id, self.cfg.display.poll_interval.duration()))
    }

    fn periodic_singleton(&mut self, owner: OwnerId, kind: JobKind, cadence: Duration) -> JobId {
        let id = JobId::singleton(owner, kind);
        self.insert(JobSpec::periodic(id, cadence))
    }

    fn page_command(&mut self, page: PageId, cadence: Duration) -> JobId {
        let id = JobId::with_source(
            OwnerId::Page,
            JobKind::PageCommand,
            SourceIdentity::Page(page),
        );
        self.insert(JobSpec::periodic(id, cadence))
    }

    fn fast_page_command(&mut self, page: PageId) -> JobId {
        let id = JobId::with_source(
            OwnerId::Page,
            JobKind::PageCommand,
            SourceIdentity::Page(page),
        );
        self.insert(JobSpec::fast(id, self.cfg.display.poll_interval.duration()))
    }

    fn mark_counter(&mut self, id: JobId) -> JobId {
        if let Some(spec) = self.specs.get_mut(&id) {
            spec.counter = true;
        }
        id
    }

    fn insert(&mut self, spec: JobSpec) -> JobId {
        let id = spec.id.clone();
        self.specs.entry(id.clone()).or_insert(spec);
        id
    }
}

#[cfg(test)]
mod tests;
