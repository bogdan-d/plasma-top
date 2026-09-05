use std::collections::{BTreeSet, VecDeque};

use crate::scheduler::{JobTicket, RefreshTrigger, SchedulerAction, SchedulerEvent};
use crate::sensors::{
    CollectCtx, ReconciliationOutcome, execute_scheduled_job, reconcile_inventory_family,
};

use super::*;

const MAX_STARTS_PER_DRAIN: usize = 16;

#[derive(Default)]
pub(super) struct ActionQueue {
    pending: VecDeque<QueuedAction>,
}

pub(super) enum QueuedAction {
    Scheduler(SchedulerAction),
    RunStart(JobTicket),
    Commit(Box<StagedAttempt>),
    PostCompletion {
        ticket: JobTicket,
        completion: CompletionKind,
        will_start_fallback: bool,
        inventory_changed: bool,
    },
}

struct AttemptState {
    owners: Owners,
    hw: HardwareInventory,
    readings: DisplaySnapshot,
    command_cache: PageCommandCache,
    resolved_mounts: Vec<String>,
}

pub(super) struct StagedAttempt {
    ticket: JobTicket,
    state: AttemptState,
    completion: CompletionKind,
    notification_ready: bool,
    notifications: DisplaySnapshot,
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn execute_transition(
    transition: Transition,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    command_lookup: &crate::page_commands::CommandLookup,
    boot: ClockSnapshot,
) -> Result<()> {
    let mut queue = std::mem::take(&mut state.action_queue);
    queue.extend(transition);
    let mut starts = 0;
    let mut deferred_prepaint = BTreeSet::new();

    while let Some(action) = queue.pending.pop_front() {
        match action {
            QueuedAction::Scheduler(SchedulerAction::StartJob { ticket }) => {
                if request_shutdown_if_stopped(&mut queue, scheduler, state, control) {
                    continue;
                }
                if starts >= MAX_STARTS_PER_DRAIN {
                    queue
                        .pending
                        .push_front(QueuedAction::Scheduler(SchedulerAction::StartJob {
                            ticket,
                        }));
                    break;
                }
                let overdue = scheduler.handle(SchedulerEvent::TimeAdvanced {
                    at: scheduler_time(control.snapshot()),
                });
                queue.prepend_with_tail(overdue, QueuedAction::RunStart(ticket));
            }
            QueuedAction::RunStart(ticket) => {
                if request_shutdown_if_stopped(&mut queue, scheduler, state, control) {
                    continue;
                }
                if !scheduler.is_current_ticket(&ticket) {
                    continue;
                }
                if !state.first_paint_published && is_slow_startup_job(&ticket.job) {
                    if !deferred_prepaint.insert(ticket.run_id) {
                        queue.pending.push_front(QueuedAction::RunStart(ticket));
                        break;
                    }
                    queue.pending.push_back(QueuedAction::RunStart(ticket));
                    continue;
                }
                starts = starts.saturating_add(1);
                let staged =
                    execute_start(ticket, state, roots, boundaries, control, command_lookup);
                let overdue = scheduler.handle(SchedulerEvent::TimeAdvanced {
                    at: scheduler_time(control.snapshot()),
                });
                queue.prepend_before_commit(overdue, staged);
            }
            QueuedAction::Commit(staged) => {
                commit_attempt(*staged, &mut queue, scheduler, state, control)
            }
            QueuedAction::PostCompletion {
                ticket,
                completion,
                will_start_fallback,
                inventory_changed,
            } => {
                if ticket.job.kind == JobKind::NvidiaNvml
                    && completion == CompletionKind::Failed
                    && !will_start_fallback
                    && nvidia_fallback_due(&state.owners.nvidia, control.snapshot().monotonic)
                {
                    let fallback = JobId::singleton(
                        crate::scheduler::OwnerId::Nvidia,
                        JobKind::NvidiaFallback,
                    );
                    queue.prepend(scheduler.handle(SchedulerEvent::RefreshTriggered {
                        at: scheduler_time(control.snapshot()),
                        job: fallback,
                        trigger: RefreshTrigger::Reconnect,
                    }));
                }
                if inventory_changed {
                    state.inventory_generation = state.inventory_generation.next();
                    let config = state.scheduler_config(roots);
                    queue.prepend(scheduler.handle(SchedulerEvent::InventoryChanged {
                        at: scheduler_time(control.snapshot()),
                        update: crate::scheduler::InventoryUpdate {
                            generation: state.inventory_generation,
                            jobs: config.jobs,
                            demand: config.demand,
                            resume_acknowledgement: None,
                        },
                    }));
                }
            }
            QueuedAction::Scheduler(SchedulerAction::PublishDisplay {
                publication,
                reason,
                panel,
                tooltip,
                ..
            }) => {
                if let Some(acknowledgement) = publication::publish(
                    publication,
                    reason,
                    panel,
                    tooltip,
                    scheduler,
                    state,
                    roots,
                    paths,
                    control,
                    boot,
                )? {
                    queue.prepend(acknowledgement);
                }
            }
            QueuedAction::Scheduler(SchedulerAction::EvaluateNotifications { ticket }) => {
                if let Some(sample) = state.notification_samples.remove(&ticket.run_id) {
                    let now = control.snapshot().monotonic;
                    let report = check_and_notify(
                        &sample,
                        &state.cfg,
                        &mut state.notifications,
                        &state.hw,
                        now,
                        &mut super::super::DynNotification(boundaries.notifications),
                    );
                    for failure in report.failures {
                        eprintln!("[notify] {}", failure.error);
                    }
                }
            }
            QueuedAction::Scheduler(SchedulerAction::ResetCounterBaseline { job }) => {
                reset_counter_baseline(&job, state.owners.refs());
            }
            QueuedAction::Scheduler(SchedulerAction::RescanHardware {
                kind,
                resume_reconciliation,
            }) => {
                rescan(kind, state, roots, boundaries);
                state.inventory_generation = state.inventory_generation.next();
                let config = state.scheduler_config(roots);
                queue.prepend(scheduler.handle(SchedulerEvent::InventoryChanged {
                    at: scheduler_time(control.snapshot()),
                    update: crate::scheduler::InventoryUpdate {
                        generation: state.inventory_generation,
                        jobs: config.jobs,
                        demand: config.demand,
                        resume_acknowledgement: resume_reconciliation,
                    },
                }));
            }
            QueuedAction::Scheduler(SchedulerAction::InvalidateJob { job, .. }) => {
                invalidate_scheduled_job(&job, state.owners.refs(), &mut state.readings);
            }
            QueuedAction::Scheduler(SchedulerAction::CancelJob { ticket, .. }) => {
                state.notification_samples.remove(&ticket.run_id);
                queue.prepend(scheduler.handle(SchedulerEvent::JobCancelled {
                    at: scheduler_time(control.snapshot()),
                    ticket,
                }));
            }
            QueuedAction::Scheduler(SchedulerAction::Terminate { .. }) => state.terminate = true,
            QueuedAction::Scheduler(
                SchedulerAction::ApplyBackoff { .. } | SchedulerAction::ScheduleDeadline { .. },
            ) => {}
        }
    }

    state.action_queue = queue;
    Ok(())
}

impl ActionQueue {
    fn extend(&mut self, transition: Transition) {
        let (correctness, deferred): (Vec<_>, Vec<_>) = transition
            .actions
            .into_iter()
            .partition(must_process_before_start);
        let insertion = self
            .pending
            .iter()
            .position(QueuedAction::is_start)
            .unwrap_or(self.pending.len());
        for (offset, action) in correctness.into_iter().enumerate() {
            self.pending
                .insert(insertion + offset, QueuedAction::Scheduler(action));
        }
        self.pending
            .extend(deferred.into_iter().map(QueuedAction::Scheduler));
    }

    fn prepend(&mut self, transition: Transition) {
        for action in transition.actions.into_iter().rev() {
            self.pending.push_front(QueuedAction::Scheduler(action));
        }
    }

    fn prepend_with_tail(&mut self, transition: Transition, tail: QueuedAction) {
        self.pending.push_front(tail);
        self.prepend(transition);
    }

    fn prepend_before_commit(&mut self, transition: Transition, staged: StagedAttempt) {
        let (before, after): (Vec<_>, Vec<_>) = transition
            .actions
            .into_iter()
            .partition(|action| !matches!(action, SchedulerAction::StartJob { .. }));
        for action in after.into_iter().rev() {
            self.pending.push_front(QueuedAction::Scheduler(action));
        }
        self.pending
            .push_front(QueuedAction::Commit(Box::new(staged)));
        for action in before.into_iter().rev() {
            self.pending.push_front(QueuedAction::Scheduler(action));
        }
    }
}

impl QueuedAction {
    fn is_start(&self) -> bool {
        matches!(
            self,
            Self::Scheduler(SchedulerAction::StartJob { .. }) | Self::RunStart(_)
        )
    }
}

fn must_process_before_start(action: &SchedulerAction) -> bool {
    !matches!(
        action,
        SchedulerAction::StartJob { .. } | SchedulerAction::ScheduleDeadline { .. }
    )
}

fn request_shutdown_if_stopped(
    queue: &mut ActionQueue,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    control: &mut dyn LoopControl,
) -> bool {
    if !control.should_stop() {
        return false;
    }
    if !state.shutdown_requested {
        state.shutdown_requested = true;
        queue.prepend(scheduler.handle(SchedulerEvent::Shutdown {
            at: scheduler_time(control.snapshot()),
        }));
    }
    true
}

fn clone_attempt_state(state: &RuntimeState) -> AttemptState {
    AttemptState {
        owners: state.owners.clone(),
        hw: state.hw.clone(),
        readings: state.readings.clone(),
        command_cache: state.command_cache.clone(),
        resolved_mounts: state.resolved_mounts.clone(),
    }
}

#[allow(clippy::too_many_arguments)]
fn execute_start(
    ticket: JobTicket,
    state: &RuntimeState,
    roots: &FilesystemRoots,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    command_lookup: &crate::page_commands::CommandLookup,
) -> StagedAttempt {
    let mut staged = clone_attempt_state(state);
    let (completion, notification_ready, notifications) = if ticket.job.kind == JobKind::PageCommand
    {
        let Some(page) = page_for_job(&ticket.job, &state.active) else {
            return StagedAttempt {
                ticket,
                state: staged,
                completion: CompletionKind::ConfirmedAbsent,
                notification_ready: false,
                notifications: DisplaySnapshot::default(),
            };
        };
        let mut capture_clock = || control.snapshot().monotonic;
        let mut runner = super::super::DynRunner(boundaries.commands);
        let attempt = attempt_command_with_state_and_clock(
            &page,
            &mut runner,
            command_lookup,
            &mut staged.command_cache,
            &mut capture_clock,
        );
        let completion = match attempt {
            PageCommandAttempt::Completed(_) => CompletionKind::Captured,
            PageCommandAttempt::Unavailable(_) => CompletionKind::Failed,
        };
        (completion, false, DisplaySnapshot::default())
    } else if ticket.job.kind == JobKind::PageRender {
        (CompletionKind::Captured, false, DisplaySnapshot::default())
    } else if ticket.job.kind == JobKind::HardwareDiscovery {
        let SourceIdentity::Inventory(family) = &ticket.job.source else {
            return StagedAttempt {
                ticket,
                state: staged,
                completion: CompletionKind::ConfirmedAbsent,
                notification_ready: false,
                notifications: DisplaySnapshot::default(),
            };
        };
        let outcome = reconcile_inventory_family(
            *family,
            &mut staged.hw,
            &roots.sys_root,
            &roots.proc_root,
            &state.cfg,
            boundaries.dbus,
            boundaries.commands,
        );
        let completion = match outcome {
            ReconciliationOutcome::Captured => CompletionKind::Captured,
            ReconciliationOutcome::Failed => CompletionKind::Failed,
        };
        (completion, false, DisplaySnapshot::default())
    } else if ticket.job.kind == JobKind::MountInventory {
        match disk::try_resolve_mounts(&roots.proc_root, &state.cfg) {
            Ok(mounts) => {
                staged.resolved_mounts = mounts;
                (CompletionKind::Captured, false, DisplaySnapshot::default())
            }
            Err(_) => (CompletionKind::Failed, false, DisplaySnapshot::default()),
        }
    } else {
        let mut capture_clock = || control.snapshot();
        let mut ctx = CollectCtx::new(
            roots,
            boundaries.commands,
            boundaries.dbus,
            &mut capture_clock,
        );
        ctx.nvml = boundaries.nvml.take();
        ctx.bolt = boundaries.bolt.take();
        let result = execute_scheduled_job(
            &ticket,
            staged.owners.refs(),
            &mut staged.hw,
            &state.cfg,
            &mut ctx,
            &mut staged.readings,
            None,
        );
        boundaries.nvml = ctx.nvml.take();
        boundaries.bolt = ctx.bolt.take();
        (
            result.completion,
            result.notification_ready,
            result.notifications,
        )
    };
    StagedAttempt {
        ticket,
        state: staged,
        completion,
        notification_ready,
        notifications,
    }
}

fn commit_attempt(
    staged: StagedAttempt,
    queue: &mut ActionQueue,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    control: &mut dyn LoopControl,
) {
    if !scheduler.is_current_ticket(&staged.ticket) {
        queue.prepend(scheduler.handle(SchedulerEvent::JobFinished {
            at: scheduler_time(control.snapshot()),
            ticket: staged.ticket,
            completion: staged.completion,
            notification_ready: false,
        }));
        return;
    }

    let inventory_changed =
        state.hw != staged.state.hw || state.resolved_mounts != staged.state.resolved_mounts;
    let mut readings = staged.state.readings;
    readings.assembled_at = state.readings.assembled_at;
    state.owners = staged.state.owners;
    state.hw = staged.state.hw;
    state.readings = readings;
    state.command_cache = staged.state.command_cache;
    state.resolved_mounts = staged.state.resolved_mounts;
    state
        .notification_samples
        .insert(staged.ticket.run_id, staged.notifications);

    let completed = scheduler.handle(SchedulerEvent::JobFinished {
        at: scheduler_time(control.snapshot()),
        ticket: staged.ticket.clone(),
        completion: staged.completion,
        notification_ready: staged.notification_ready,
    });
    let will_notify = completed.actions.iter().any(|action| {
        matches!(
            action,
            SchedulerAction::EvaluateNotifications { ticket }
                if ticket.run_id == staged.ticket.run_id
        )
    });
    let will_start_fallback = completed.actions.iter().any(|action| {
        matches!(
            action,
            SchedulerAction::StartJob { ticket }
                if ticket.job.kind == JobKind::NvidiaFallback
        )
    });
    if !will_notify {
        state.notification_samples.remove(&staged.ticket.run_id);
    }
    let post_completion = QueuedAction::PostCompletion {
        ticket: staged.ticket,
        completion: staged.completion,
        will_start_fallback,
        inventory_changed,
    };
    if inventory_changed {
        queue.prepend(completed);
        queue.pending.push_front(post_completion);
    } else {
        queue.prepend_with_tail(completed, post_completion);
    }
}

fn nvidia_fallback_due(state: &gpu_nvidia::NvidiaState, now: std::time::Duration) -> bool {
    state
        .cache
        .fallback_attempted_at
        .is_none_or(|attempted| now.saturating_sub(attempted) >= gpu_nvidia::GPU_CACHE_TTL)
}

fn is_slow_startup_job(job: &JobId) -> bool {
    matches!(
        job.kind,
        JobKind::NetworkIdentity
            | JobKind::DiskUsage
            | JobKind::Smart
            | JobKind::SystemBattery
            | JobKind::PeripheralBattery
            | JobKind::NvidiaNvml
            | JobKind::NvidiaFallback
            | JobKind::AmdSlow
            | JobKind::IntelUsage
            | JobKind::PanelProcesses
            | JobKind::PageProcesses
            | JobKind::PageCommand
            | JobKind::PageRender
            | JobKind::HardwareDiscovery
    )
}

#[cfg(test)]
#[path = "executor/tests.rs"]
mod tests;
