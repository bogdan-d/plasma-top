use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Duration;

use crate::domain::readings::InventoryFamily;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct SchedulerTime(Duration);

impl SchedulerTime {
    pub(crate) const ZERO: Self = Self(Duration::ZERO);

    pub(crate) const fn from_duration(value: Duration) -> Self {
        Self(value)
    }

    pub(crate) const fn duration(self) -> Duration {
        self.0
    }

    pub(crate) fn saturating_add(self, duration: Duration) -> Self {
        Self(self.0.saturating_add(duration))
    }

    pub(crate) fn saturating_sub(self, duration: Duration) -> Self {
        Self(self.0.saturating_sub(duration))
    }
}

impl From<Duration> for SchedulerTime {
    fn from(value: Duration) -> Self {
        Self(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct ConfigGeneration(pub(crate) u64);

impl ConfigGeneration {
    pub(crate) fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(crate) struct InventoryGeneration(pub(crate) u64);

impl InventoryGeneration {
    pub(crate) fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RunId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PublicationId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ResumeReconciliationId(pub(crate) u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct HistoryDeadline(SchedulerTime);

impl HistoryDeadline {
    pub(crate) const fn new(at: SchedulerTime) -> Self {
        Self(at)
    }

    pub(crate) const fn at(self) -> SchedulerTime {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum OwnerId {
    Cpu,
    Process,
    Memory,
    Network,
    Disk,
    Power,
    Nvidia,
    IntelGpu,
    GpuHistory,
    External,
    Page,
    Discovery,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum PageId {
    Main,
    Processes,
    CpuCores,
    Connections,
    Fastfetch,
    Graphs,
    Other(String),
}

impl PageId {
    pub(crate) fn from_id(value: &str) -> Self {
        match value {
            "full" => Self::Main,
            "processes" => Self::Processes,
            "cpu_cores" => Self::CpuCores,
            "connections" => Self::Connections,
            "fastfetch" => Self::Fastfetch,
            "graphs" => Self::Graphs,
            other => Self::Other(other.to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum SourceIdentity {
    Singleton,
    CpuSources {
        temperature: Option<PathBuf>,
        frequency: Option<PathBuf>,
        turbo: Option<PathBuf>,
    },
    Path(PathBuf),
    Device(String),
    Mount(PathBuf),
    NamedPath {
        name: String,
        path: PathBuf,
    },
    SmartDrive {
        label: String,
        object_path: String,
    },
    Battery(String),
    Peripheral {
        role: PeripheralRole,
        source: PeripheralSource,
    },
    Inventory(InventoryFamily),
    Page(PageId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum PeripheralRole {
    Mouse,
    Keyboard,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum PeripheralSource {
    Upower(String),
    Bolt(i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum JobKind {
    Cpu,
    CpuCores,
    CpuHistory,
    CpuCoreHistory,
    Memory,
    MemoryHistory,
    NetworkRate,
    NetworkIdentity,
    NetworkHistory,
    DiskIo,
    DiskUsage,
    Smart,
    DiskTemperature,
    FanSpeed,
    SystemBattery,
    PeripheralBattery,
    NvidiaNvml,
    NvidiaFallback,
    IntelFrequency,
    IntelUsage,
    GpuHistory,
    PanelProcesses,
    PageProcesses,
    Brightness,
    UpdatesFile,
    ServerFile,
    PageCommand,
    PageRender,
    HardwareDiscovery,
    MountInventory,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct JobId {
    pub(crate) owner: OwnerId,
    pub(crate) kind: JobKind,
    pub(crate) source: SourceIdentity,
}

impl JobId {
    pub(crate) const fn singleton(owner: OwnerId, kind: JobKind) -> Self {
        Self {
            owner,
            kind,
            source: SourceIdentity::Singleton,
        }
    }

    pub(crate) fn with_source(owner: OwnerId, kind: JobKind, source: SourceIdentity) -> Self {
        Self {
            owner,
            kind,
            source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TimingClass {
    FastDisplay,
    Periodic,
    History,
    Triggered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobSpec {
    pub(crate) id: JobId,
    pub(crate) timing: TimingClass,
    pub(crate) freshness: Duration,
    pub(crate) history_source: Option<JobId>,
    pub(crate) startup_panel: bool,
    pub(crate) counter: bool,
    pub(crate) volatile: bool,
}

impl JobSpec {
    pub(crate) fn periodic(id: JobId, freshness: Duration) -> Self {
        Self {
            id,
            timing: TimingClass::Periodic,
            freshness,
            history_source: None,
            startup_panel: false,
            counter: false,
            volatile: false,
        }
    }

    pub(crate) fn fast(id: JobId, freshness: Duration) -> Self {
        Self {
            id,
            timing: TimingClass::FastDisplay,
            freshness,
            history_source: None,
            startup_panel: false,
            counter: false,
            volatile: false,
        }
    }

    pub(crate) fn history(id: JobId, source: JobId, cadence: Duration) -> Self {
        Self {
            id,
            timing: TimingClass::History,
            freshness: cadence,
            history_source: Some(source),
            startup_panel: false,
            counter: false,
            volatile: false,
        }
    }

    pub(crate) fn source_history(id: JobId, cadence: Duration) -> Self {
        Self {
            id,
            timing: TimingClass::History,
            freshness: cadence,
            history_source: None,
            startup_panel: false,
            counter: false,
            volatile: false,
        }
    }

    pub(crate) fn triggered(id: JobId, retry_budget: Duration) -> Self {
        Self {
            id,
            timing: TimingClass::Triggered,
            freshness: retry_budget,
            history_source: None,
            startup_panel: false,
            counter: false,
            volatile: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct DemandPlan {
    pub(crate) hidden: BTreeSet<JobId>,
    pub(crate) main_tooltip: BTreeSet<JobId>,
    pub(crate) pages: BTreeMap<PageId, BTreeSet<JobId>>,
}

impl DemandPlan {
    pub(crate) fn demanded(&self, presented: bool, selected_page: &PageId) -> BTreeSet<JobId> {
        let mut demanded = self.hidden.clone();
        if presented {
            demanded.extend(self.main_tooltip.iter().cloned());
            if let Some(page) = self.pages.get(selected_page) {
                demanded.extend(page.iter().cloned());
            }
        }
        demanded
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SchedulerConfig {
    pub(crate) generation: ConfigGeneration,
    pub(crate) display_interval: Duration,
    pub(crate) jobs: Vec<JobSpec>,
    pub(crate) demand: DemandPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InventoryUpdate {
    pub(crate) generation: InventoryGeneration,
    pub(crate) jobs: Vec<JobSpec>,
    pub(crate) demand: DemandPlan,
    pub(crate) resume_acknowledgement: Option<ResumeReconciliationId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JobTicket {
    pub(crate) run_id: RunId,
    pub(crate) job: JobId,
    pub(crate) config_generation: ConfigGeneration,
    pub(crate) inventory_generation: InventoryGeneration,
    pub(crate) history_deadline: Option<HistoryDeadline>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionKind {
    Captured,
    Baseline,
    ConfirmedAbsent,
    Failed,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefreshTrigger {
    FileChanged,
    Signal,
    RouteChanged,
    PeripheralChanged,
    Reconnect,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SchedulerEvent {
    Startup {
        at: SchedulerTime,
        config: SchedulerConfig,
        inventory_generation: InventoryGeneration,
    },
    TimeAdvanced {
        at: SchedulerTime,
    },
    ConfigChanged {
        at: SchedulerTime,
        config: SchedulerConfig,
    },
    InventoryChanged {
        at: SchedulerTime,
        update: InventoryUpdate,
    },
    DemandChanged {
        at: SchedulerTime,
        demand: DemandPlan,
    },
    TooltipPresented {
        at: SchedulerTime,
        presented: bool,
    },
    SelectedPageChanged {
        at: SchedulerTime,
        page: PageId,
    },
    DisplayRefreshRequested {
        at: SchedulerTime,
    },
    TooltipRefreshRequested {
        at: SchedulerTime,
    },
    JobFinished {
        at: SchedulerTime,
        ticket: JobTicket,
        completion: CompletionKind,
        notification_ready: bool,
    },
    JobCancelled {
        at: SchedulerTime,
        ticket: JobTicket,
    },
    RefreshTriggered {
        at: SchedulerTime,
        job: JobId,
        trigger: RefreshTrigger,
    },
    PanelPublished {
        at: SchedulerTime,
        publication: PublicationId,
    },
    Suspend {
        at: SchedulerTime,
    },
    Resume {
        at: SchedulerTime,
    },
    Shutdown {
        at: SchedulerTime,
    },
    CriticalFailure {
        at: SchedulerTime,
        component: &'static str,
    },
}

impl SchedulerEvent {
    pub(crate) const fn at(&self) -> SchedulerTime {
        match self {
            Self::Startup { at, .. }
            | Self::TimeAdvanced { at }
            | Self::ConfigChanged { at, .. }
            | Self::InventoryChanged { at, .. }
            | Self::DemandChanged { at, .. }
            | Self::TooltipPresented { at, .. }
            | Self::SelectedPageChanged { at, .. }
            | Self::DisplayRefreshRequested { at }
            | Self::TooltipRefreshRequested { at }
            | Self::JobFinished { at, .. }
            | Self::JobCancelled { at, .. }
            | Self::RefreshTriggered { at, .. }
            | Self::PanelPublished { at, .. }
            | Self::Suspend { at }
            | Self::Resume { at }
            | Self::Shutdown { at }
            | Self::CriticalFailure { at, .. } => *at,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublishReason {
    FirstPaintReady,
    FirstPaintTimeout,
    DisplayDeadline,
    TooltipActivated,
    TooltipRefresh,
    PageChanged,
    ConfigChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelReason {
    ConfigChanged,
    SourceReplaced,
    DemandEnded,
    Suspend,
    Shutdown,
    CriticalFailure,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RescanKind {
    Hardware,
    VolatileInventoryAndRoute,
    Peripherals,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SchedulerAction {
    CancelJob {
        ticket: JobTicket,
        reason: CancelReason,
    },
    InvalidateJob {
        job: JobId,
        reason: CancelReason,
    },
    ResetCounterBaseline {
        job: JobId,
    },
    PublishDisplay {
        publication: PublicationId,
        reason: PublishReason,
        panel: bool,
        tooltip: bool,
    },
    EvaluateNotifications {
        ticket: JobTicket,
    },
    RescanHardware {
        kind: RescanKind,
        resume_reconciliation: Option<ResumeReconciliationId>,
    },
    ApplyBackoff {
        job: JobId,
        failures: u32,
        retry_at: SchedulerTime,
    },
    StartJob {
        ticket: JobTicket,
    },
    ScheduleDeadline {
        at: SchedulerTime,
    },
    Terminate {
        component: Option<&'static str>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventDisposition {
    Accepted,
    RejectedStale,
    RejectedTimeRegression,
    RejectedLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Transition {
    pub(crate) disposition: EventDisposition,
    pub(crate) actions: Vec<SchedulerAction>,
}

impl Transition {
    pub(crate) fn rejected(disposition: EventDisposition) -> Self {
        Self {
            disposition,
            actions: Vec::new(),
        }
    }
}
