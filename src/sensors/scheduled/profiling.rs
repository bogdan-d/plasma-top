use std::time::Duration;

use crate::domain::readings::RetainedMetricSample;
use crate::scheduler::{JobId, JobKind, PeripheralRole, SourceIdentity};

use super::super::{OwnerRefs, network};

pub(crate) fn capture_time(job: &JobId, owners: OwnerRefs<'_>) -> Option<Duration> {
    match (&job.kind, &job.source) {
        (JobKind::Cpu, _) => oldest([
            retained(&owners.cpu.usage),
            retained(&owners.cpu.temperature),
            retained(&owners.cpu.frequency_mhz),
            retained(&owners.cpu.turbo),
            retained(&owners.cpu.uptime_seconds),
            retained(&owners.cpu.load_average),
        ]),
        (JobKind::CpuCores, _) => retained(&owners.cpu.core_usage),
        (JobKind::CpuHistory, _) => owners.cpu.cpu_history_sample_at,
        (JobKind::CpuCoreHistory, _) => owners.cpu.cpu_core_history_sample_at,
        (JobKind::Memory, _) => oldest([
            retained(&owners.memory.usage),
            retained(&owners.memory.swap_usage),
        ]),
        (JobKind::MemoryHistory, _) => owners.memory.mem_history_sample_at,
        (JobKind::NetworkRate, _) => retained(&owners.network.rate),
        (JobKind::NetworkIdentity, _) => oldest([
            retained(&owners.network.info),
            retained(&owners.network.wifi),
        ]),
        (JobKind::NetworkHistory, _) => network::net_history_sample_at(owners.network),
        (JobKind::DiskIo, _) => retained(&owners.disk.io),
        (JobKind::DiskUsage, SourceIdentity::Mount(path)) => {
            owners.disk.usage.get(path.to_str()?).and_then(retained)
        }
        (JobKind::Smart, SourceIdentity::SmartDrive { label, .. }) => {
            owners.disk.smart_cache.get(label).and_then(retained)
        }
        (JobKind::DiskTemperature, SourceIdentity::NamedPath { name, .. }) => {
            owners.disk.hd_temp_cache.get(name).and_then(retained)
        }
        (JobKind::FanSpeed, SourceIdentity::NamedPath { name, .. }) => {
            owners.disk.fan_speed_cache.get(name).and_then(retained)
        }
        (JobKind::SystemBattery, SourceIdentity::Battery(id)) => owners
            .power
            .battery_sys_cache
            .get(id)
            .and_then(|sample| sample.sampled_at),
        (JobKind::PeripheralBattery, SourceIdentity::Peripheral { role, .. }) => match role {
            PeripheralRole::Mouse => owners.power.battery_mouse_cache.sampled_at,
            PeripheralRole::Keyboard => owners.power.battery_kbd_cache.sampled_at,
        },
        (JobKind::NvidiaNvml | JobKind::NvidiaFallback, _) => oldest([
            retained(&owners.nvidia.cache.history_samples),
            retained(&owners.nvidia.cache.decoder),
            retained(&owners.nvidia.cache.fan),
        ]),
        (JobKind::IntelFrequency, _) => retained(&owners.intel_gpu.frequency),
        (JobKind::IntelUsage, _) => retained(&owners.intel_gpu.usage),
        (JobKind::GpuHistory, _) => oldest([
            owners
                .gpu_history
                .usage_sample
                .as_ref()
                .map(|sample| sample.captured_at),
            owners
                .gpu_history
                .decoder_sample
                .as_ref()
                .map(|sample| sample.captured_at),
        ]),
        (JobKind::PanelProcesses, _) => retained(&owners.process.panel),
        (JobKind::PageProcesses, _) => retained(&owners.process.page),
        (JobKind::Brightness, _) => retained(&owners.external.brightness),
        (JobKind::UpdatesFile, _) => retained(&owners.external.updates),
        (JobKind::ServerFile, _) => retained(&owners.external.server),
        _ => None,
    }
}

fn oldest<const N: usize>(captures: [Option<Duration>; N]) -> Option<Duration> {
    captures.into_iter().flatten().min()
}

fn retained<T>(sample: &RetainedMetricSample<T>) -> Option<Duration> {
    sample.latest.as_ref().map(|sample| sample.captured_at)
}
