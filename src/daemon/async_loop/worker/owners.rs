use crate::domain::readings::DisplaySnapshot;
use crate::scheduler::JobId;
use crate::sensors::process::ProcessState;
use crate::sensors::{
    OwnerRefs, cpu, external, gpu_history, gpu_intel, gpu_nvidia, invalidate_scheduled_job, memory,
    network, power,
};

#[derive(Clone)]
pub(super) struct Owners {
    pub(super) cpu: cpu::CpuState,
    pub(super) memory: memory::MemoryState,
    pub(super) network: network::NetworkState,
    pub(super) disk: crate::sensors::disk::DiskState,
    pub(super) process: ProcessState,
    pub(super) intel_gpu: gpu_intel::IntelGpuState,
    pub(super) power: power::PowerState,
    pub(super) nvidia: gpu_nvidia::NvidiaState,
    pub(super) gpu_history: gpu_history::GpuHistoryState,
    pub(super) external: external::ExternalState,
}

impl Owners {
    pub(super) fn new() -> Self {
        Self {
            cpu: cpu::CpuState::default(),
            memory: memory::MemoryState::default(),
            network: network::NetworkState::default(),
            disk: crate::sensors::disk::DiskState::default(),
            process: ProcessState::default(),
            intel_gpu: gpu_intel::IntelGpuState::default(),
            power: power::PowerState::default(),
            nvidia: gpu_nvidia::NvidiaState::default(),
            gpu_history: gpu_history::GpuHistoryState::default(),
            external: external::ExternalState::default(),
        }
    }

    pub(super) fn refs(&mut self) -> OwnerRefs<'_> {
        OwnerRefs {
            cpu: &mut self.cpu,
            memory: &mut self.memory,
            network: &mut self.network,
            disk: &mut self.disk,
            process: &mut self.process,
            intel_gpu: &mut self.intel_gpu,
            power: &mut self.power,
            nvidia: &mut self.nvidia,
            gpu_history: &mut self.gpu_history,
            external: &mut self.external,
        }
    }
}

pub(super) fn invalidate_readings(job: &JobId, readings: &mut DisplaySnapshot) {
    let mut owners = Owners::new();
    invalidate_scheduled_job(job, owners.refs(), readings);
}
