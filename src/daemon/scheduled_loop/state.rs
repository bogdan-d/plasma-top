use super::*;

#[derive(Clone)]
pub(super) struct Owners {
    pub(super) cpu: cpu::CpuState,
    pub(super) memory: memory::MemoryState,
    pub(super) network: network::NetworkState,
    pub(super) disk: disk::DiskState,
    pub(super) process: ProcessState,
    pub(super) amd_gpu: crate::sensors::gpu_amd::AmdGpuState,
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
            disk: disk::DiskState::default(),
            process: ProcessState::default(),
            amd_gpu: crate::sensors::gpu_amd::AmdGpuState::default(),
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
            amd_gpu: &mut self.amd_gpu,
            intel_gpu: &mut self.intel_gpu,
            power: &mut self.power,
            nvidia: &mut self.nvidia,
            gpu_history: &mut self.gpu_history,
            external: &mut self.external,
        }
    }
}

pub(super) struct RuntimeState {
    pub(super) cfg: Config,
    pub(super) hw: HardwareInventory,
    pub(super) active: Vec<Page>,
    pub(super) owners: Owners,
    pub(super) readings: DisplaySnapshot,
    pub(super) notifications: NotificationState,
    pub(super) notification_samples: BTreeMap<RunId, DisplaySnapshot>,
    pub(super) command_cache: PageCommandCache,
    pub(super) config_generation: ConfigGeneration,
    pub(super) inventory_generation: InventoryGeneration,
    pub(super) css: String,
    pub(super) light: bool,
    pub(super) css_path: PathBuf,
    pub(super) overlay_path: Option<PathBuf>,
    pub(super) css_stamp: Option<SystemTime>,
    pub(super) overlay_stamp: Option<SystemTime>,
    pub(super) config_stamp: Option<SystemTime>,
    pub(super) machine_stamps: Vec<Option<SystemTime>>,
    pub(super) plasma_stamp: Option<SystemTime>,
    pub(super) geom_stamp: Option<SystemTime>,
    pub(super) kde_stamp: Option<SystemTime>,
    pub(super) updates_stamp: Option<SystemTime>,
    pub(super) server_stamp: Option<SystemTime>,
    pub(super) panel_html: Option<String>,
    pub(super) tooltip_html: Option<String>,
    pub(super) display_publication_count: usize,
    pub(super) first_paint_published: bool,
    pub(super) theme_reconciliation_pending: bool,
    pub(super) shutdown_requested: bool,
    pub(super) action_queue: ActionQueue,
    pub(super) resolved_mounts: Vec<String>,
    pub(super) boot_pending: BTreeSet<&'static str>,
    pub(super) terminate: bool,
}

impl RuntimeState {
    pub(super) fn selected_page(&self, paths: &DaemonPaths) -> (usize, PageId) {
        let index = page_index(paths, self.active.len());
        (index, PageId::from_id(self.active[index].id))
    }

    pub(super) fn scheduler_config(
        &self,
        _roots: &FilesystemRoots,
    ) -> crate::scheduler::SchedulerConfig {
        scheduler_config(
            &self.cfg,
            &self.hw,
            &self.resolved_mounts,
            self.config_generation,
        )
    }
}
