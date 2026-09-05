use std::path::Path;

use crate::domain::boundary::{ClockSnapshot, CommandRunner, DbusFacade, FilesystemRoots};

use super::external::ExternalState;
use super::gpu_history::GpuHistoryState;
use super::gpu_intel::IntelGpuState;
use super::gpu_nvidia::{NvidiaState, NvmlFacade};
use super::power::{BoltBatteryFacade, PowerState};
use super::{cpu, disk, memory, network, process};

/// Short-lived borrowed wiring for the domain owners used by one synchronous sampling pass.
///
/// The wiring owns no cache state. Daemon and diagnostic composition keep every domain owner as a separate long-lived value and construct this view only while executing one scheduled job.
pub struct OwnerRefs<'a> {
    /// CPU aggregate/per-core diff and history owner.
    pub cpu: &'a mut cpu::CpuState,
    /// Memory sample and history owner.
    pub memory: &'a mut memory::MemoryState,
    /// Network identity, rate, and history owner.
    pub network: &'a mut network::NetworkState,
    /// Disk I/O, usage, temperature, fan, and SMART owner.
    pub disk: &'a mut disk::DiskState,
    /// Panel and selected-page process owner.
    pub process: &'a mut process::ProcessState,
    /// AMDGPU independently retained samples.
    pub amd_gpu: &'a mut crate::sensors::gpu_amd::AmdGpuState,
    /// Intel GPU sample and counter owner.
    pub intel_gpu: &'a mut IntelGpuState,
    /// System and peripheral power owner.
    pub power: &'a mut PowerState,
    /// NVIDIA sample and source-selection owner.
    pub nvidia: &'a mut NvidiaState,
    /// Selected GPU history owner.
    pub gpu_history: &'a mut GpuHistoryState,
    /// Brightness and external status-file owner.
    pub external: &'a mut ExternalState,
}

impl OwnerRefs<'_> {
    pub(crate) fn reborrow(&mut self) -> OwnerRefs<'_> {
        OwnerRefs {
            cpu: self.cpu,
            memory: self.memory,
            network: self.network,
            disk: self.disk,
            process: self.process,
            amd_gpu: self.amd_gpu,
            intel_gpu: self.intel_gpu,
            power: self.power,
            nvidia: self.nvidia,
            gpu_history: self.gpu_history,
            external: self.external,
        }
    }
}

/// Borrowed roots, boundaries, and clock for one synchronous sampling pass.
///
/// Grouping the `&mut` boundaries keeps scheduled job execution wiring explicit and reviewable.
///
/// The serial executor never stores the context; it borrows for one job attempt.
pub struct CollectCtx<'io, 'optional> {
    /// `/proc` fixture root (production: `/proc`).
    pub proc_root: &'io Path,
    /// `/sys` fixture root (production: `/sys`).
    pub sys_root: &'io Path,
    /// Command runner for `ip`/`iw`/`nvidia-smi`.
    pub commands: &'io mut dyn CommandRunner,
    /// D-Bus facade for UPower/UDisks2.
    pub dbus: &'io mut dyn DbusFacade,
    /// Optional NVML facade. `None` selects the `nvidia-smi` fallback path (matches Python with `python-nvidia-ml-py` absent).
    pub nvml: Option<&'optional mut (dyn NvmlFacade + 'optional)>,
    /// Optional Bolt HID facade. `None` suppresses Bolt battery reads.
    pub bolt: Option<&'optional mut (dyn BoltBatteryFacade + 'optional)>,
    /// Clock sampled around each owner attempt and at display assembly.
    pub clock: &'io mut dyn FnMut() -> ClockSnapshot,
    /// First-paint flag: skip slow sources without an immediately required sample.
    pub skip_slow: bool,
}

impl<'io, 'optional> CollectCtx<'io, 'optional> {
    /// Builds a context from a [`FilesystemRoots`] plus the boundary trait objects the daemon holds, defaulting `skip_slow = false`.
    #[must_use]
    pub fn new(
        roots: &'io FilesystemRoots,
        commands: &'io mut dyn CommandRunner,
        dbus: &'io mut dyn DbusFacade,
        clock: &'io mut dyn FnMut() -> ClockSnapshot,
    ) -> Self {
        Self {
            proc_root: &roots.proc_root,
            sys_root: &roots.sys_root,
            commands,
            dbus,
            nvml: None,
            bolt: None,
            clock,
            skip_slow: false,
        }
    }
}
