use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use tokio::sync::{Semaphore, mpsc, oneshot};
use tokio::task::JoinSet;

use crate::adapters::{ProductionClock, ProductionCommandRunner, ProductionDbusFacade};
use crate::config::Config;
use crate::domain::boundary::{ClockSnapshot, FilesystemRoots};
use crate::domain::readings::{DisplaySnapshot, HardwareInventory, MetricSample};
use crate::page_commands::{
    Page, PageCommandAttempt, PageCommandCache, PageSource, attempt_command_with_state_and_clock,
};
use crate::scheduler::{
    CompletionKind, ConfigGeneration, InventoryGeneration, JobId, JobKind, JobTicket, OwnerId,
    RescanKind, RunId, SourceIdentity,
};
use crate::sensors::{
    CollectCtx, ReconciliationOutcome, discover_hardware_attempt, disk, gpu_history, gpu_nvidia,
    invalidate_scheduled_job, power, reconcile_inventory_family, rescan_peripherals,
    reset_counter_baseline,
};

use super::super::{DynRunner, render_page_cached};

#[path = "worker/owners.rs"]
mod owners;

use owners::Owners;

pub(super) const OWNER_CHANNEL_CAPACITY: usize = 2;
pub(super) const COMPLETION_CHANNEL_CAPACITY: usize = 16;
pub(super) type GpuHistoryValue = (Option<i32>, Option<i32>, gpu_history::DecoderOutcome);
pub(super) type GpuHistoryPoint = MetricSample<GpuHistoryValue>;
pub(super) type SourcedGpuHistoryPoint = (SourceIdentity, GpuHistoryPoint);

pub(super) struct OwnerServices {
    pub(super) commands: ProductionCommandRunner,
    pub(super) dbus: ProductionDbusFacade,
    #[cfg(test)]
    pub(super) blocked_owner: Option<TestOwnerBlock>,
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct TestOwnerBlock {
    pub(super) owner: OwnerId,
    pub(super) entered: Arc<std::sync::atomic::AtomicBool>,
    pub(super) release: Arc<Semaphore>,
}

#[derive(Clone)]
pub(super) struct JobInput {
    pub(super) ticket: JobTicket,
    pub(super) cfg: Config,
    pub(super) hw: HardwareInventory,
    pub(super) readings: DisplaySnapshot,
    pub(super) active: Vec<Page>,
    pub(super) selected_index: usize,
    pub(super) css: String,
    pub(super) style_generation: u64,
    pub(super) render_generation: u64,
    pub(super) resolved_mounts: Vec<String>,
    pub(super) gpu_decoder_outcome: gpu_history::DecoderOutcome,
    pub(super) gpu_history_point: Option<GpuHistoryPoint>,
}

#[derive(Clone)]
pub(super) struct RescanInput {
    pub(super) kind: RescanKind,
    pub(super) cfg: Config,
    pub(super) hw: HardwareInventory,
    pub(super) config_generation: ConfigGeneration,
    pub(super) inventory_generation: InventoryGeneration,
    pub(super) lifecycle_generation: u64,
    pub(super) resume_reconciliation: Option<crate::scheduler::ResumeReconciliationId>,
}

pub(super) enum OwnerMessage {
    Run(Box<JobInput>),
    Reset(JobId),
    Invalidate(JobId),
    PagesChanged(Vec<Page>),
    Rescan(Box<RescanInput>),
    ReconcileTheme {
        kdeglobals: std::path::PathBuf,
        stamp: Option<SystemTime>,
    },
}

impl OwnerMessage {
    pub(super) fn ticket(&self) -> Option<&JobTicket> {
        match self {
            Self::Run(input) => Some(&input.ticket),
            Self::Reset(_)
            | Self::Invalidate(_)
            | Self::PagesChanged(_)
            | Self::Rescan(_)
            | Self::ReconcileTheme { .. } => None,
        }
    }
}

pub(super) struct JobCompletion {
    pub(super) ticket: JobTicket,
    pub(super) completion: CompletionKind,
    pub(super) readings: DisplaySnapshot,
    pub(super) notifications: DisplaySnapshot,
    pub(super) notification_ready: bool,
    pub(super) hw: HardwareInventory,
    pub(super) resolved_mounts: Vec<String>,
    pub(super) command_cache: Option<PageCommandCache>,
    pub(super) rendered_page: Option<String>,
    pub(super) style_generation: u64,
    pub(super) render_generation: u64,
    pub(super) decoder_outcome: Option<gpu_history::DecoderOutcome>,
    pub(super) gpu_history_point: Option<SourcedGpuHistoryPoint>,
    pub(super) decision: Option<oneshot::Sender<bool>>,
}

pub(super) enum OwnerCompletion {
    Job(Box<JobCompletion>),
    Cancelled(JobTicket),
    Rescan {
        config_generation: ConfigGeneration,
        inventory_generation: InventoryGeneration,
        lifecycle_generation: u64,
        resume_reconciliation: Option<crate::scheduler::ResumeReconciliationId>,
        hw: Box<HardwareInventory>,
    },
    Theme {
        stamp: Option<SystemTime>,
        light: bool,
    },
}

#[derive(Clone, Default)]
pub(super) struct DispatchValidity {
    runs: Arc<Mutex<BTreeSet<RunId>>>,
    lifecycle_generation: Arc<AtomicU64>,
}

impl DispatchValidity {
    pub(super) fn insert(&self, run: RunId) {
        match self.runs.lock() {
            Ok(mut runs) => {
                runs.insert(run);
            }
            Err(poisoned) => {
                poisoned.into_inner().insert(run);
            }
        }
    }

    pub(super) fn remove(&self, run: RunId) -> bool {
        match self.runs.lock() {
            Ok(mut runs) => runs.remove(&run),
            Err(poisoned) => poisoned.into_inner().remove(&run),
        }
    }

    fn contains(&self, run: RunId) -> bool {
        match self.runs.lock() {
            Ok(runs) => runs.contains(&run),
            Err(poisoned) => poisoned.into_inner().contains(&run),
        }
    }

    pub(super) fn lifecycle_generation(&self) -> u64 {
        self.lifecycle_generation.load(Ordering::Acquire)
    }

    pub(super) fn advance_lifecycle(&self) -> u64 {
        self.lifecycle_generation
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1)
    }
}

#[derive(Clone)]
pub(super) struct OwnerSenders {
    senders: BTreeMap<OwnerId, mpsc::Sender<OwnerMessage>>,
}

impl OwnerSenders {
    pub(super) fn sender(&self, owner: OwnerId) -> Option<&mpsc::Sender<OwnerMessage>> {
        self.senders.get(&owner)
    }

    #[cfg(test)]
    pub(super) fn single(owner: OwnerId, sender: mpsc::Sender<OwnerMessage>) -> Self {
        Self {
            senders: BTreeMap::from([(owner, sender)]),
        }
    }
}

pub(super) fn spawn_owners(
    tasks: &mut JoinSet<OwnerId>,
    services: OwnerServices,
    roots: &FilesystemRoots,
    clock: ProductionClock,
    completion: mpsc::Sender<OwnerCompletion>,
    blocking_lane: Arc<Semaphore>,
    validity: DispatchValidity,
) -> OwnerSenders {
    let mut senders = BTreeMap::new();
    for owner in ALL_OWNERS {
        let (sender, receiver) = mpsc::channel(OWNER_CHANNEL_CAPACITY);
        senders.insert(owner, sender);
        let worker = WorkerState::new(owner, services.commands.clone(), services.dbus.clone());
        let roots = roots.clone();
        let clock = clock.clone();
        let completion = completion.clone();
        let blocking_lane = Arc::clone(&blocking_lane);
        let validity = validity.clone();
        #[cfg(test)]
        let blocked_owner = services.blocked_owner.clone();
        tasks.spawn(owner_loop(
            worker,
            receiver,
            completion,
            roots,
            clock,
            blocking_lane,
            validity,
            #[cfg(test)]
            blocked_owner,
        ));
    }
    OwnerSenders { senders }
}

const ALL_OWNERS: [OwnerId; 12] = [
    OwnerId::Cpu,
    OwnerId::Process,
    OwnerId::Memory,
    OwnerId::Network,
    OwnerId::Disk,
    OwnerId::Power,
    OwnerId::Nvidia,
    OwnerId::IntelGpu,
    OwnerId::GpuHistory,
    OwnerId::External,
    OwnerId::Page,
    OwnerId::Discovery,
];

#[allow(clippy::too_many_arguments)]
async fn owner_loop(
    mut worker: WorkerState,
    mut receiver: mpsc::Receiver<OwnerMessage>,
    completion_sender: mpsc::Sender<OwnerCompletion>,
    roots: FilesystemRoots,
    clock: ProductionClock,
    blocking_lane: Arc<Semaphore>,
    validity: DispatchValidity,
    #[cfg(test)] blocked_owner: Option<TestOwnerBlock>,
) -> OwnerId {
    let owner = worker.owner;
    while let Some(message) = receiver.recv().await {
        match message {
            OwnerMessage::Run(input) => {
                #[cfg(test)]
                if let Some(block) = blocked_owner.as_ref().filter(|block| block.owner == owner) {
                    block.entered.store(true, Ordering::Release);
                    if block.release.acquire().await.is_err() {
                        return owner;
                    }
                }
                if !validity.contains(input.ticket.run_id) {
                    if completion_sender
                        .send(OwnerCompletion::Cancelled(input.ticket))
                        .await
                        .is_err()
                    {
                        return owner;
                    }
                    continue;
                }
                let checkpoint = worker.checkpoint();
                let mut output = if uses_blocking_lane(&input.ticket.job) {
                    let Ok(permit) = Arc::clone(&blocking_lane).acquire_owned().await else {
                        return owner;
                    };
                    if !validity.contains(input.ticket.run_id) {
                        if completion_sender
                            .send(OwnerCompletion::Cancelled(input.ticket))
                            .await
                            .is_err()
                        {
                            return owner;
                        }
                        continue;
                    }
                    let attempt_roots = roots.clone();
                    let attempt_clock = clock.clone();
                    let task = tokio::task::spawn_blocking(move || {
                        let _permit = permit;
                        let output = worker.execute(*input, &attempt_roots, &attempt_clock);
                        (worker, output)
                    });
                    let Ok((returned, output)) = task.await else {
                        return owner;
                    };
                    worker = returned;
                    output
                } else {
                    worker.execute(*input, &roots, &clock)
                };
                let (decision, accepted) = oneshot::channel();
                output.decision = Some(decision);
                if completion_sender
                    .send(OwnerCompletion::Job(Box::new(output)))
                    .await
                    .is_err()
                {
                    return owner;
                }
                if !matches!(accepted.await, Ok(true)) {
                    worker.restore(checkpoint);
                }
            }
            OwnerMessage::Reset(job) => reset_counter_baseline(&job, worker.owners.refs()),
            OwnerMessage::Invalidate(job) => {
                invalidate_scheduled_job(
                    &job,
                    worker.owners.refs(),
                    &mut DisplaySnapshot::default(),
                );
            }
            OwnerMessage::PagesChanged(pages) => worker.pages_changed(&pages),
            OwnerMessage::Rescan(input) => {
                if input.lifecycle_generation != validity.lifecycle_generation() {
                    continue;
                }
                let Ok(permit) = Arc::clone(&blocking_lane).acquire_owned().await else {
                    return owner;
                };
                if input.lifecycle_generation != validity.lifecycle_generation() {
                    continue;
                }
                let attempt_roots = roots.clone();
                let task = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    let output = worker.rescan(*input, &attempt_roots);
                    (worker, output)
                });
                let Ok((returned, output)) = task.await else {
                    return owner;
                };
                worker = returned;
                if completion_sender.send(output).await.is_err() {
                    return owner;
                }
            }
            OwnerMessage::ReconcileTheme { kdeglobals, stamp } => {
                let Ok(permit) = Arc::clone(&blocking_lane).acquire_owned().await else {
                    return owner;
                };
                let mut commands = worker.commands.clone();
                let task = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    super::super::plasma_is_light(&mut commands, &kdeglobals)
                });
                let Ok(light) = task.await else {
                    return owner;
                };
                if completion_sender
                    .send(OwnerCompletion::Theme { stamp, light })
                    .await
                    .is_err()
                {
                    return owner;
                }
            }
        }
    }
    owner
}

struct WorkerState {
    owner: OwnerId,
    owners: Owners,
    commands: ProductionCommandRunner,
    dbus: ProductionDbusFacade,
    command_cache: PageCommandCache,
    had_process_page: bool,
    bolt: Option<crate::sensors::hid::BoltHidFacade>,
    #[cfg(feature = "nvml")]
    nvml: Option<gpu_nvidia::ProductionNvml>,
}

struct WorkerCheckpoint {
    owners: Owners,
    command_cache: PageCommandCache,
    had_process_page: bool,
}

impl WorkerState {
    fn new(owner: OwnerId, commands: ProductionCommandRunner, dbus: ProductionDbusFacade) -> Self {
        Self {
            owner,
            owners: Owners::new(),
            commands,
            dbus,
            command_cache: PageCommandCache::new(),
            had_process_page: false,
            bolt: (owner == OwnerId::Power).then(crate::sensors::hid::BoltHidFacade::default),
            #[cfg(feature = "nvml")]
            nvml: (owner == OwnerId::Nvidia).then(gpu_nvidia::ProductionNvml::new),
        }
    }

    fn pages_changed(&mut self, pages: &[Page]) {
        self.command_cache.retain_pages(pages);
        let has_process_page = pages
            .iter()
            .any(|page| page.render() == Some(crate::page_commands::PageRenderKind::TopProcess));
        if self.had_process_page && !has_process_page {
            self.owners.process.reset_page();
        }
        self.had_process_page = has_process_page;
    }

    fn checkpoint(&self) -> WorkerCheckpoint {
        WorkerCheckpoint {
            owners: self.owners.clone(),
            command_cache: self.command_cache.clone(),
            had_process_page: self.had_process_page,
        }
    }

    fn restore(&mut self, checkpoint: WorkerCheckpoint) {
        self.owners = checkpoint.owners;
        self.command_cache = checkpoint.command_cache;
        self.had_process_page = checkpoint.had_process_page;
    }

    fn execute(
        &mut self,
        input: JobInput,
        roots: &FilesystemRoots,
        clock: &ProductionClock,
    ) -> JobCompletion {
        self.pages_changed(&input.active);
        let ticket = input.ticket;
        let style_generation = input.style_generation;
        let render_generation = input.render_generation;
        let mut hw = input.hw;
        let mut readings = input.readings;
        let mut resolved_mounts = input.resolved_mounts;
        let mut rendered_page = None;
        let (completion, notification_ready, notifications) = match ticket.job.kind {
            JobKind::PageCommand => {
                self.execute_page_command(&ticket, &input.active, clock, &mut rendered_page)
            }
            JobKind::PageRender => {
                rendered_page = Some(render_page_cached(
                    &input.cfg,
                    &hw,
                    &readings,
                    &input.css,
                    &input.active,
                    input.selected_index,
                    &self.command_cache,
                    &roots.proc_root,
                ));
                (CompletionKind::Captured, false, DisplaySnapshot::default())
            }
            JobKind::GpuHistory => {
                match (&ticket.job.source, input.gpu_history_point.as_ref()) {
                    (SourceIdentity::Device(source), point) if source == "nvidia" => {
                        readings.gpu_usage = point.and_then(|sample| sample.value.0);
                        readings.gpu_dec = point.and_then(|sample| sample.value.1);
                    }
                    (SourceIdentity::Device(source), point) if source.starts_with("intel:") => {
                        readings.gpu_intel_usage = point.and_then(|sample| sample.value.0);
                        readings.gpu_intel_dec_usage = point.and_then(|sample| sample.value.1);
                    }
                    _ => {}
                }
                let decoder_outcome = input
                    .gpu_history_point
                    .as_ref()
                    .map_or(input.gpu_decoder_outcome, |sample| sample.value.2);
                let captured_at = ticket.history_deadline.map_or_else(
                    || clock.snapshot(),
                    |deadline| ClockSnapshot {
                        monotonic: deadline.at().duration(),
                        ..ClockSnapshot::default()
                    },
                );
                let result = self.owners.gpu_history.sample(
                    &input.cfg,
                    &hw,
                    &readings,
                    decoder_outcome,
                    captured_at,
                    true,
                );
                let completion = if let Some(result) = result {
                    if let Some(history) = result.usage {
                        readings.gpu_usage_history = history.value;
                    }
                    if let Some(history) = result.decoder {
                        readings.gpu_dec_history = history.value;
                    }
                    CompletionKind::Captured
                } else {
                    CompletionKind::ConfirmedAbsent
                };
                (completion, false, DisplaySnapshot::default())
            }
            JobKind::HardwareDiscovery => {
                let completion = if let SourceIdentity::Inventory(family) = ticket.job.source {
                    match reconcile_inventory_family(
                        family,
                        &mut hw,
                        &roots.sys_root,
                        &roots.proc_root,
                        &input.cfg,
                        &mut self.dbus,
                        &mut self.commands,
                    ) {
                        ReconciliationOutcome::Captured => CompletionKind::Captured,
                        ReconciliationOutcome::Failed => CompletionKind::Failed,
                    }
                } else {
                    CompletionKind::ConfirmedAbsent
                };
                (completion, false, DisplaySnapshot::default())
            }
            JobKind::MountInventory => {
                let completion = match disk::try_resolve_mounts(&roots.proc_root, &input.cfg) {
                    Ok(mounts) => {
                        resolved_mounts = mounts;
                        CompletionKind::Captured
                    }
                    Err(_) => CompletionKind::Failed,
                };
                (completion, false, DisplaySnapshot::default())
            }
            _ => {
                let mut capture_clock = || clock.snapshot();
                let mut context = CollectCtx::new(
                    roots,
                    &mut self.commands,
                    &mut self.dbus,
                    &mut capture_clock,
                );
                context.bolt = self
                    .bolt
                    .as_mut()
                    .map(|bolt| bolt as &mut dyn power::BoltBatteryFacade);
                #[cfg(feature = "nvml")]
                {
                    context.nvml = self
                        .nvml
                        .as_mut()
                        .map(|nvml| nvml as &mut dyn gpu_nvidia::NvmlFacade);
                }
                let result = crate::sensors::execute_scheduled_job(
                    &ticket,
                    self.owners.refs(),
                    &mut hw,
                    &input.cfg,
                    &mut context,
                    &mut readings,
                    None,
                );
                (
                    result.completion,
                    result.notification_ready,
                    result.notifications,
                )
            }
        };
        let decoder_outcome = job_decoder_outcome(
            ticket.job.kind,
            completion,
            readings.gpu_dec,
            readings.gpu_intel_dec_usage,
        );
        let gpu_history_point = match ticket.job.kind {
            JobKind::NvidiaNvml | JobKind::NvidiaFallback => self
                .owners
                .nvidia
                .latest_history_point()
                .map(|sample| (SourceIdentity::Device(String::from("nvidia")), sample)),
            JobKind::IntelUsage => {
                let source = match &ticket.job.source {
                    SourceIdentity::Device(pci) => SourceIdentity::Device(format!("intel:{pci}")),
                    source => source.clone(),
                };
                self.owners
                    .intel_gpu
                    .latest_history_point()
                    .map(|sample| (source, sample))
            }
            _ => None,
        }
        .map(|(source, sample)| {
            let outcome = decoder_outcome.unwrap_or_else(|| {
                sample.value.1.map_or(
                    gpu_history::DecoderOutcome::Unmeasured,
                    gpu_history::DecoderOutcome::Value,
                )
            });
            (
                source,
                MetricSample::new(
                    (sample.value.0, sample.value.1, outcome),
                    sample.captured_at,
                ),
            )
        });
        JobCompletion {
            ticket,
            completion,
            readings,
            notifications,
            notification_ready,
            hw,
            resolved_mounts,
            command_cache: (self.owner == OwnerId::Page).then(|| self.command_cache.clone()),
            rendered_page,
            style_generation,
            render_generation,
            decoder_outcome,
            gpu_history_point,
            decision: None,
        }
    }

    fn execute_page_command(
        &mut self,
        ticket: &JobTicket,
        active: &[Page],
        clock: &ProductionClock,
        rendered_page: &mut Option<String>,
    ) -> (CompletionKind, bool, DisplaySnapshot) {
        let Some(page) = page_for_job(&ticket.job, active) else {
            return (
                CompletionKind::ConfirmedAbsent,
                false,
                DisplaySnapshot::default(),
            );
        };
        let lookup = super::super::executable_lookup();
        let mut capture_clock = || clock.snapshot().monotonic;
        let mut runner = DynRunner(&mut self.commands);
        let attempt = attempt_command_with_state_and_clock(
            &page,
            &mut runner,
            &lookup,
            &mut self.command_cache,
            &mut capture_clock,
        );
        *rendered_page = None;
        (
            match attempt {
                PageCommandAttempt::Completed(_) => CompletionKind::Captured,
                PageCommandAttempt::Unavailable(_) => CompletionKind::Failed,
            },
            false,
            DisplaySnapshot::default(),
        )
    }

    fn rescan(&mut self, input: RescanInput, roots: &FilesystemRoots) -> OwnerCompletion {
        let mut hw = input.hw;
        match input.kind {
            RescanKind::Hardware | RescanKind::VolatileInventoryAndRoute => {
                let cpu_count = hw.cpu_count.max(1);
                discover_hardware_attempt(
                    &roots.sys_root,
                    &roots.proc_root,
                    &input.cfg,
                    &mut self.dbus,
                    &mut self.commands,
                    cpu_count,
                )
                .merge_into(&mut hw);
            }
            RescanKind::Peripherals => {
                rescan_peripherals(&mut hw, &input.cfg, &mut self.dbus, &mut self.commands)
            }
        }
        OwnerCompletion::Rescan {
            config_generation: input.config_generation,
            inventory_generation: input.inventory_generation,
            lifecycle_generation: input.lifecycle_generation,
            resume_reconciliation: input.resume_reconciliation,
            hw: Box::new(hw),
        }
    }
}

pub(super) fn job_decoder_outcome(
    kind: JobKind,
    completion: CompletionKind,
    nvidia: Option<i32>,
    intel: Option<i32>,
) -> Option<gpu_history::DecoderOutcome> {
    match kind {
        JobKind::NvidiaNvml | JobKind::NvidiaFallback
            if completion == CompletionKind::ConfirmedAbsent =>
        {
            None
        }
        JobKind::NvidiaNvml | JobKind::NvidiaFallback => Some(decoder_outcome(completion, nvidia)),
        JobKind::IntelUsage => Some(decoder_outcome(completion, intel)),
        _ => None,
    }
}

fn page_for_job(job: &JobId, active: &[Page]) -> Option<Page> {
    let SourceIdentity::Page(page_id) = &job.source else {
        return None;
    };
    active.iter().copied().find(|page| {
        crate::scheduler::PageId::from_id(page.id) == *page_id
            && matches!(page.source, PageSource::Command(_))
    })
}

fn decoder_outcome(completion: CompletionKind, value: Option<i32>) -> gpu_history::DecoderOutcome {
    match (completion, value) {
        (CompletionKind::Captured, Some(value)) => gpu_history::DecoderOutcome::Value(value),
        (CompletionKind::Captured | CompletionKind::ConfirmedAbsent, None)
        | (CompletionKind::ConfirmedAbsent, Some(_)) => {
            gpu_history::DecoderOutcome::ConfirmedAbsent
        }
        (CompletionKind::Failed, _) => gpu_history::DecoderOutcome::TransientFailure,
        (CompletionKind::Baseline, _) => gpu_history::DecoderOutcome::Unmeasured,
    }
}

pub(super) fn uses_blocking_lane(job: &JobId) -> bool {
    matches!(
        job.kind,
        JobKind::NetworkIdentity
            | JobKind::DiskUsage
            | JobKind::Smart
            | JobKind::SystemBattery
            | JobKind::PeripheralBattery
            | JobKind::NvidiaNvml
            | JobKind::NvidiaFallback
            | JobKind::IntelUsage
            | JobKind::PanelProcesses
            | JobKind::PageProcesses
            | JobKind::UpdatesFile
            | JobKind::ServerFile
            | JobKind::PageCommand
            | JobKind::PageRender
            | JobKind::HardwareDiscovery
            | JobKind::MountInventory
    )
}

pub(super) fn invalidate_readings(job: &JobId, readings: &mut DisplaySnapshot) {
    owners::invalidate_readings(job, readings);
}
