use std::path::PathBuf;
use std::time::Duration;

use crate::domain::readings::{BatterySystemReading, DiskUsageReading, DisplaySnapshot};
use crate::scheduler::{JobId, JobKind, OwnerId, SourceIdentity};
use crate::sensors::process::ProcessState;
use crate::sensors::{
    OwnerRefs, cpu, disk, external, gpu_history, gpu_intel, gpu_nvidia, memory, network, power,
};

use super::invalidate_scheduled_job;

struct Owners {
    cpu: cpu::CpuState,
    memory: memory::MemoryState,
    network: network::NetworkState,
    disk: disk::DiskState,
    process: ProcessState,
    amd: crate::sensors::gpu_amd::AmdGpuState,
    intel: gpu_intel::IntelGpuState,
    power: power::PowerState,
    nvidia: gpu_nvidia::NvidiaState,
    gpu_history: gpu_history::GpuHistoryState,
    external: external::ExternalState,
}

impl Owners {
    fn new() -> Self {
        Self {
            cpu: Default::default(),
            memory: Default::default(),
            network: Default::default(),
            disk: Default::default(),
            process: Default::default(),
            amd: Default::default(),
            intel: Default::default(),
            power: Default::default(),
            nvidia: Default::default(),
            gpu_history: Default::default(),
            external: Default::default(),
        }
    }

    fn refs(&mut self) -> OwnerRefs<'_> {
        OwnerRefs {
            cpu: &mut self.cpu,
            memory: &mut self.memory,
            network: &mut self.network,
            disk: &mut self.disk,
            process: &mut self.process,
            amd_gpu: &mut self.amd,
            intel_gpu: &mut self.intel,
            power: &mut self.power,
            nvidia: &mut self.nvidia,
            gpu_history: &mut self.gpu_history,
            external: &mut self.external,
        }
    }
}

#[test]
fn keyed_source_invalidation_clears_only_the_confirmed_source() {
    let mut owners = Owners::new();
    let mut readings = DisplaySnapshot {
        battery_sys: vec![
            BatterySystemReading {
                id: String::from("BAT0"),
                ..BatterySystemReading::default()
            },
            BatterySystemReading {
                id: String::from("BAT1"),
                ..BatterySystemReading::default()
            },
        ],
        ..DisplaySnapshot::default()
    };
    readings
        .disk_usage
        .insert(String::from("/data"), Some(DiskUsageReading::default()));
    readings.hd_temps.insert(String::from("nvme"), Some(42));
    readings.system_updates = Some(7);
    owners.external.updates_source = Some(PathBuf::from("/tmp/updates"));

    for job in [
        JobId::with_source(
            OwnerId::Power,
            JobKind::SystemBattery,
            SourceIdentity::Battery(String::from("BAT0")),
        ),
        JobId::with_source(
            OwnerId::Disk,
            JobKind::DiskUsage,
            SourceIdentity::Mount(PathBuf::from("/data")),
        ),
        JobId::with_source(
            OwnerId::Disk,
            JobKind::DiskTemperature,
            SourceIdentity::NamedPath {
                name: String::from("nvme"),
                path: PathBuf::from("/sys/temp"),
            },
        ),
        JobId::with_source(
            OwnerId::External,
            JobKind::UpdatesFile,
            SourceIdentity::Path(PathBuf::from("/tmp/updates")),
        ),
    ] {
        invalidate_scheduled_job(&job, owners.refs(), &mut readings, None);
    }

    assert_eq!(readings.battery_sys.len(), 1);
    assert_eq!(readings.battery_sys[0].id, "BAT1");
    assert!(!readings.disk_usage.contains_key("/data"));
    assert!(!readings.hd_temps.contains_key("nvme"));
    assert_eq!(readings.system_updates, None);
    assert_eq!(owners.external.updates_source, None);
}

#[test]
fn aggregate_cpu_source_replacement_invalidates_the_composite_sample() {
    let mut owners = Owners::new();
    let mut readings = DisplaySnapshot {
        cpu_usage: Some(30),
        cpu_temp: Some(50),
        cpu_freq_mhz: Some(3_000.0),
        cpu_turbo: Some(true),
        uptime_seconds: Some(60),
        load_average: Some(crate::domain::readings::LoadAverage::default()),
        ..DisplaySnapshot::default()
    };
    let job = JobId::with_source(
        OwnerId::Cpu,
        JobKind::Cpu,
        SourceIdentity::CpuSources {
            temperature: Some(PathBuf::from("/sys/temp-a")),
            frequency: Some(PathBuf::from("/sys/freq-a")),
            turbo: Some(PathBuf::from("/sys/turbo-a")),
        },
    );

    invalidate_scheduled_job(&job, owners.refs(), &mut readings, None);

    assert_eq!(readings.cpu_usage, None);
    assert_eq!(readings.cpu_temp, None);
    assert_eq!(readings.cpu_freq_mhz, None);
    assert_eq!(readings.cpu_turbo, None);
    assert_eq!(readings.uptime_seconds, None);
    assert_eq!(readings.load_average, None);
}

#[test]
fn brightness_removal_invalidates_only_brightness_external_data() {
    let mut owners = Owners::new();
    owners
        .external
        .brightness
        .record_value(40, Duration::from_secs(1));
    owners
        .external
        .updates
        .record_value(7, Duration::from_secs(1));
    owners.external.updates_source = Some(PathBuf::from("/run/updates"));
    owners
        .external
        .server
        .record_value(true, Duration::from_secs(1));
    owners.external.server_source = Some(PathBuf::from("/run/server"));
    let mut readings = DisplaySnapshot {
        screen_brightness: Some(40),
        system_updates: Some(7),
        server_ok: Some(true),
        ..DisplaySnapshot::default()
    };

    invalidate_scheduled_job(
        &JobId::singleton(OwnerId::External, JobKind::Brightness),
        owners.refs(),
        &mut readings,
        None,
    );

    assert!(owners.external.brightness.latest.is_none());
    assert_eq!(readings.screen_brightness, None);
    assert_eq!(readings.system_updates, Some(7));
    assert_eq!(readings.server_ok, Some(true));
    assert_eq!(
        owners.external.updates_source,
        Some(PathBuf::from("/run/updates"))
    );
    assert_eq!(
        owners.external.server_source,
        Some(PathBuf::from("/run/server"))
    );
}
