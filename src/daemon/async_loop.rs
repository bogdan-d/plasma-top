use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;

use crate::adapters::{
    ProductionClock, ProductionCommandRunner, ProductionDbusFacade, ProductionIoEvents,
    ProductionNotificationFacade,
};
use crate::config::{cache_live_geom, load_config};
use crate::domain::boundary::{ClockSnapshot, FilesystemRoots, NotificationPayload};
use crate::error::{Error, Result};
use crate::notify::check_and_notify;
use crate::scheduler::{
    InventoryUpdate, JobKind, OwnerId, PageId, RefreshTrigger, Scheduler, SchedulerAction,
    SchedulerEvent, SchedulerTime, Transition,
};
use crate::sensors::{discover_local_hardware, discover_local_hardware_attempt};

use super::{
    DaemonPaths, PAGE_WAKE_INTERVAL, cleanup, mtime, page_index, publish_pages, write_atomic,
};

#[path = "async_loop/completion.rs"]
mod completion;
#[path = "async_loop/control.rs"]
mod control;
#[path = "async_loop/notifications.rs"]
mod notifications;
#[path = "async_loop/reload.rs"]
mod reload;
#[path = "async_loop/state.rs"]
mod state;
#[path = "async_loop/worker.rs"]
mod worker;

use completion::handle_completion;
#[cfg(test)]
use completion::{
    reconcile_inventory_publication, should_trigger_nvidia_fallback, suppress_tooltip_refresh,
    transition_starts_job,
};
use control::{LoopWake, preempt_for_shutdown, process_io_events, sleep_duration, wait_for_wake};
use state::RuntimeState;
use worker::{
    DispatchValidity, JobInput, OwnerMessage, OwnerSenders, RescanInput, invalidate_readings,
};

const NOTIFICATION_CHANNEL_CAPACITY: usize = 8;
const BLOCKING_LANE_CAPACITY: usize = 1;
const OWNER_PENDING_CAPACITY: usize = 1024;
const FIRST_PAINT_DEADLINE: std::time::Duration = std::time::Duration::from_millis(200);

pub(super) struct DaemonServices {
    pub(super) commands: ProductionCommandRunner,
    pub(super) dbus: ProductionDbusFacade,
    pub(super) notifications: ProductionNotificationFacade,
    pub(super) stopped: Arc<AtomicBool>,
    pub(super) events: ProductionIoEvents,
    pub(super) clock: ProductionClock,
    pub(super) shutdown: tokio::sync::watch::Receiver<bool>,
    #[cfg(test)]
    pub(super) blocked_owner: Option<worker::TestOwnerBlock>,
}

pub(super) async fn run(
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    services: DaemonServices,
) -> Result<()> {
    let DaemonServices {
        commands,
        dbus,
        notifications,
        stopped,
        events: mut io_events,
        clock,
        mut shutdown,
        #[cfg(test)]
        blocked_owner,
    } = services;
    fs::create_dir_all(&paths.runtime)?;
    fs::create_dir_all(&paths.state)?;
    cleanup(paths);
    write_atomic(&paths.page, "0")?;

    let boot = clock.snapshot();
    let cfg = load_config(config_path, None)?;
    let cpu_count = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let hw = discover_local_hardware(&roots.sys_root, &roots.proc_root, &cfg, cpu_count);
    let active = publish_pages(paths, &cfg)?;
    let mut state = RuntimeState::new(config_path, paths, cfg, hw, active);
    let (watch_path, machine_paths) = state::config_watch_paths(config_path);
    let blocking_lane = Arc::new(Semaphore::new(BLOCKING_LANE_CAPACITY));
    let (completion_sender, mut completions) = mpsc::channel(worker::COMPLETION_CHANNEL_CAPACITY);
    let mut owner_tasks = JoinSet::new();
    let validity = DispatchValidity::default();
    let owner_senders = worker::spawn_owners(
        &mut owner_tasks,
        worker::OwnerServices {
            commands,
            dbus,
            #[cfg(test)]
            blocked_owner,
        },
        roots,
        clock.clone(),
        completion_sender,
        Arc::clone(&blocking_lane),
        validity.clone(),
    );
    let (notification_sender, notification_receiver) = mpsc::channel(NOTIFICATION_CHANNEL_CAPACITY);
    let mut notification_task = tokio::spawn(notifications::run(
        notification_receiver,
        notifications,
        Arc::clone(&blocking_lane),
    ));
    let mut scheduler = Scheduler::new();
    let mut actions = VecDeque::new();
    let mut owner_messages = VecDeque::new();
    let mut pending_completion = None;
    enqueue(
        &mut actions,
        scheduler.handle(SchedulerEvent::Startup {
            at: SchedulerTime::from_duration(boot.monotonic),
            config: state.scheduler_config(),
            inventory_generation: state.inventory_generation,
        }),
    );
    enqueue(
        &mut actions,
        scheduler.handle(SchedulerEvent::TooltipPresented {
            at: now(&clock),
            presented: true,
        }),
    );
    let (_, selected) = state.selected_page(paths);
    enqueue(
        &mut actions,
        scheduler.handle(SchedulerEvent::SelectedPageChanged {
            at: now(&clock),
            page: selected,
        }),
    );
    cache_live_geom();

    loop {
        preempt_for_shutdown(
            stopped.load(Ordering::Acquire),
            now(&clock),
            &mut scheduler,
            &mut pending_completion,
            &mut actions,
            &mut owner_messages,
        );
        drain_actions(
            &mut actions,
            &mut owner_messages,
            &mut scheduler,
            &mut state,
            roots,
            paths,
            &clock,
            boot,
            &notification_sender,
            &validity,
        )?;
        if let Some(component) = flush_owner_messages(&mut owner_messages, &owner_senders) {
            critical_failure(component, &mut scheduler, &clock, &mut actions);
            continue;
        }
        if state.terminate {
            break;
        }
        if let Some((completion, completion_at)) = pending_completion.take() {
            let completion_at = completion_time_after_drain(completion_at, now(&clock));
            handle_completion(
                completion,
                &mut scheduler,
                &mut state,
                roots,
                paths,
                &clock,
                boot,
                completion_at,
                &mut actions,
                &mut owner_messages,
                &validity,
            );
            continue;
        }
        if clock.snapshot().monotonic >= boot.monotonic.saturating_add(FIRST_PAINT_DEADLINE)
            && let Some(publication) = state.deferred_first_paint.take()
        {
            state.deferred_first_paint_jobs.clear();
            actions.push_back(deadline_aware_first_paint(publication, &clock, boot));
        }
        process_io_events(
            io_events.drain(),
            &mut scheduler,
            &state,
            &clock,
            &mut actions,
            &validity,
        );
        drain_actions(
            &mut actions,
            &mut owner_messages,
            &mut scheduler,
            &mut state,
            roots,
            paths,
            &clock,
            boot,
            &notification_sender,
            &validity,
        )?;
        if let Some(component) = flush_owner_messages(&mut owner_messages, &owner_senders) {
            critical_failure(component, &mut scheduler, &clock, &mut actions);
            continue;
        }
        if state.terminate {
            break;
        }
        enqueue(
            &mut actions,
            scheduler.handle(SchedulerEvent::TimeAdvanced { at: now(&clock) }),
        );
        drain_actions(
            &mut actions,
            &mut owner_messages,
            &mut scheduler,
            &mut state,
            roots,
            paths,
            &clock,
            boot,
            &notification_sender,
            &validity,
        )?;
        if let Some(component) = flush_owner_messages(&mut owner_messages, &owner_senders) {
            critical_failure(component, &mut scheduler, &clock, &mut actions);
            continue;
        }
        reload::check(
            config_path,
            roots,
            paths,
            cpu_count,
            &watch_path,
            &machine_paths,
            &mut scheduler,
            &mut state,
            &clock,
            &mut actions,
            &mut owner_messages,
        )?;
        let (_, selected) = state.selected_page(paths);
        enqueue(
            &mut actions,
            scheduler.handle(SchedulerEvent::SelectedPageChanged {
                at: now(&clock),
                page: selected,
            }),
        );
        reload::trigger_external_changes(&mut scheduler, &mut state, &clock, &mut actions);
        if state.update_style(paths) {
            enqueue(
                &mut actions,
                scheduler.handle(SchedulerEvent::DisplayRefreshRequested { at: now(&clock) }),
            );
            request_selected_graph(&mut scheduler, &state, paths, &clock, &mut actions);
        }
        if state.first_paint_published && state.theme_reconciliation_pending {
            state.theme_reconciliation_pending = false;
            if !queue_owner_message(
                &mut owner_messages,
                (
                    OwnerId::Page,
                    OwnerMessage::ReconcileTheme {
                        kdeglobals: paths.kdeglobals.clone(),
                        stamp: state.kde_stamp,
                    },
                ),
            ) {
                critical_failure("owner pending queue", &mut scheduler, &clock, &mut actions);
            }
        }
        drain_actions(
            &mut actions,
            &mut owner_messages,
            &mut scheduler,
            &mut state,
            roots,
            paths,
            &clock,
            boot,
            &notification_sender,
            &validity,
        )?;
        if let Some(component) = flush_owner_messages(&mut owner_messages, &owner_senders) {
            critical_failure(component, &mut scheduler, &clock, &mut actions);
        }
        if state.terminate || !actions.is_empty() {
            continue;
        }

        let sleep_for = sleep_duration(&scheduler, &state, boot, clock.snapshot());
        match wait_for_wake(
            &mut owner_tasks,
            &mut notification_task,
            &mut shutdown,
            &mut completions,
            sleep_for,
        )
        .await
        {
            LoopWake::OwnerExit(exit) => {
                let component = owner_exit_component(exit);
                critical_failure(component, &mut scheduler, &clock, &mut actions);
            }
            LoopWake::NotificationExit => {
                critical_failure("notification task", &mut scheduler, &clock, &mut actions);
            }
            LoopWake::Shutdown | LoopWake::Sleep => {}
            LoopWake::Completion(completion) => match completion {
                Some(completion) => {
                    let completion_at = now(&clock);
                    pending_completion = Some((completion, completion_at));
                    enqueue(
                        &mut actions,
                        scheduler.handle(SchedulerEvent::TimeAdvanced { at: completion_at }),
                    );
                }
                None => critical_failure(
                    "owner completion service",
                    &mut scheduler,
                    &clock,
                    &mut actions,
                ),
            },
        }
    }

    owner_tasks.abort_all();
    notification_task.abort();
    cleanup(paths);
    if let Some(component) = state.critical_component {
        return Err(Error::Runtime(format!(
            "critical async daemon {component} exited unexpectedly"
        )));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn drain_actions(
    actions: &mut VecDeque<SchedulerAction>,
    owner_messages: &mut VecDeque<(OwnerId, OwnerMessage)>,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    clock: &ProductionClock,
    boot: ClockSnapshot,
    notification_sender: &mpsc::Sender<NotificationPayload>,
    validity: &DispatchValidity,
) -> Result<()> {
    while let Some(action) = actions.pop_front() {
        match action {
            SchedulerAction::StartJob { ticket } => {
                if scheduler.is_current_ticket(&ticket) {
                    validity.insert(ticket.run_id);
                    let selected_index = page_index(paths, state.active.len());
                    let input = JobInput {
                        ticket: ticket.clone(),
                        cfg: state.cfg.clone(),
                        hw: state.hw.clone(),
                        readings: state.readings.clone(),
                        active: state.active.clone(),
                        selected_index,
                        css: state.css.clone(),
                        style_generation: state.style_generation,
                        render_generation: state.render_generation,
                        resolved_mounts: state.resolved_mounts.clone(),
                        gpu_decoder_outcome: state.decoder_outcome_for(&ticket.job),
                        gpu_history_point: state.gpu_history_point_for(&ticket),
                    };
                    if !queue_owner_message(
                        owner_messages,
                        (ticket.job.owner, OwnerMessage::Run(Box::new(input))),
                    ) {
                        return Err(Error::Runtime(
                            "bounded owner pending queue is full".to_owned(),
                        ));
                    }
                }
            }
            SchedulerAction::CancelJob { ticket, .. } => {
                state.notification_samples.remove(&ticket.run_id);
                let was_dispatched = validity.remove(ticket.run_id);
                let before = owner_messages.len();
                owner_messages.retain(|(_, message)| {
                    message
                        .ticket()
                        .is_none_or(|queued| queued.run_id != ticket.run_id)
                });
                if !was_dispatched || owner_messages.len() != before {
                    prepend(
                        actions,
                        scheduler.handle(SchedulerEvent::JobCancelled {
                            at: now(clock),
                            ticket,
                        }),
                    );
                }
            }
            SchedulerAction::InvalidateJob { job, .. } => {
                invalidate_readings(&job, &mut state.readings);
                state.invalidate_decoder(&job);
                state.completed_jobs.remove(&job);
                state.deferred_first_paint_jobs.remove(&job);
                if !queue_owner_message(owner_messages, (job.owner, OwnerMessage::Invalidate(job)))
                {
                    return Err(Error::Runtime(
                        "bounded owner pending queue is full".to_owned(),
                    ));
                }
            }
            SchedulerAction::ResetCounterBaseline { job } => {
                if !queue_owner_message(owner_messages, (job.owner, OwnerMessage::Reset(job))) {
                    return Err(Error::Runtime(
                        "bounded owner pending queue is full".to_owned(),
                    ));
                }
            }
            SchedulerAction::PublishDisplay {
                publication,
                reason,
                panel,
                tooltip,
            } => {
                if panel && state.deferred_first_paint.is_some() {
                    continue;
                }
                let snapshot = clock.snapshot();
                let render_generation = state.render_generation;
                if let Some(acknowledgement) = state.publish(
                    publication,
                    reason,
                    panel,
                    tooltip,
                    scheduler,
                    roots,
                    paths,
                    snapshot,
                    boot,
                )? {
                    prepend(actions, acknowledgement);
                }
                if state.render_generation != render_generation {
                    request_selected_graph(scheduler, state, paths, clock, actions);
                }
            }
            SchedulerAction::EvaluateNotifications { ticket } => {
                if let Some(sample) = state.notification_samples.remove(&ticket.run_id) {
                    let mut facade = notifications::QueueFacade(notification_sender);
                    let report = check_and_notify(
                        &sample,
                        &state.cfg,
                        &mut state.notifications,
                        &state.hw,
                        clock.snapshot().monotonic,
                        &mut facade,
                    );
                    for failure in report.failures {
                        eprintln!("[notify] {}", failure.error);
                    }
                }
            }
            SchedulerAction::RescanHardware {
                kind,
                resume_reconciliation,
            } => {
                if !queue_owner_message(
                    owner_messages,
                    (
                        OwnerId::Discovery,
                        OwnerMessage::Rescan(Box::new(RescanInput {
                            kind,
                            cfg: state.cfg.clone(),
                            hw: state.hw.clone(),
                            config_generation: state.config_generation,
                            inventory_generation: state.inventory_generation,
                            lifecycle_generation: validity.lifecycle_generation(),
                            resume_reconciliation,
                        })),
                    ),
                ) {
                    return Err(Error::Runtime(
                        "bounded owner pending queue is full".to_owned(),
                    ));
                }
            }
            SchedulerAction::Terminate { component } => {
                state.terminate = true;
                state.critical_component = component;
            }
            SchedulerAction::ApplyBackoff { .. } | SchedulerAction::ScheduleDeadline { .. } => {}
        }
    }
    Ok(())
}

fn flush_owner_messages(
    pending: &mut VecDeque<(OwnerId, OwnerMessage)>,
    senders: &OwnerSenders,
) -> Option<&'static str> {
    let count = pending.len();
    for _ in 0..count {
        let Some((owner, message)) = pending.pop_front() else {
            break;
        };
        let Some(sender) = senders.sender(owner) else {
            return Some(owner_name(owner));
        };
        match sender.try_send(message) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(message)) => {
                pending.push_back((owner, message));
            }
            Err(mpsc::error::TrySendError::Closed(_)) => return Some(owner_name(owner)),
        }
    }
    None
}

fn queue_owner_message(
    pending: &mut VecDeque<(OwnerId, OwnerMessage)>,
    queued: (OwnerId, OwnerMessage),
) -> bool {
    let (owner, message) = queued;
    pending.retain(|(candidate_owner, candidate)| {
        if *candidate_owner != owner {
            return true;
        }
        match (&message, candidate) {
            (OwnerMessage::PagesChanged(_), OwnerMessage::PagesChanged(_))
            | (OwnerMessage::ReconcileTheme { .. }, OwnerMessage::ReconcileTheme { .. }) => false,
            (OwnerMessage::Rescan(input), OwnerMessage::Rescan(candidate)) => {
                input.kind != candidate.kind
            }
            (OwnerMessage::Reset(job), OwnerMessage::Reset(candidate))
            | (OwnerMessage::Invalidate(job), OwnerMessage::Invalidate(candidate)) => {
                job != candidate
            }
            (OwnerMessage::Run(input), OwnerMessage::Run(candidate)) => {
                input.ticket.run_id != candidate.ticket.run_id
            }
            _ => true,
        }
    });
    if pending.len() >= OWNER_PENDING_CAPACITY {
        return false;
    }
    pending.push_back((owner, message));
    true
}

fn request_selected_graph(
    scheduler: &mut Scheduler,
    state: &RuntimeState,
    paths: &DaemonPaths,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
) {
    let (_, selected) = state.selected_page(paths);
    if selected != PageId::Graphs {
        return;
    }
    if let Some(job) = state
        .scheduler_config()
        .jobs
        .into_iter()
        .find(|spec| spec.id.kind == JobKind::PageRender)
        .map(|spec| spec.id)
    {
        enqueue(
            actions,
            scheduler.handle(SchedulerEvent::RefreshTriggered {
                at: now(clock),
                job,
                trigger: RefreshTrigger::Signal,
            }),
        );
    }
}

fn critical_failure(
    component: &'static str,
    scheduler: &mut Scheduler,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
) {
    enqueue(
        actions,
        scheduler.handle(SchedulerEvent::CriticalFailure {
            at: now(clock),
            component,
        }),
    );
}

fn owner_name(owner: OwnerId) -> &'static str {
    match owner {
        OwnerId::Cpu => "CPU owner",
        OwnerId::Process => "process owner",
        OwnerId::Memory => "memory owner",
        OwnerId::Network => "network owner",
        OwnerId::Disk => "disk owner",
        OwnerId::Power => "power owner",
        OwnerId::Nvidia => "NVIDIA owner",
        OwnerId::IntelGpu => "Intel GPU owner",
        OwnerId::GpuHistory => "GPU history owner",
        OwnerId::External => "external owner",
        OwnerId::Page => "page owner",
        OwnerId::Discovery => "discovery owner",
    }
}

fn deadline_aware_first_paint(
    action: SchedulerAction,
    clock: &ProductionClock,
    boot: ClockSnapshot,
) -> SchedulerAction {
    if clock.snapshot().monotonic < boot.monotonic.saturating_add(FIRST_PAINT_DEADLINE) {
        return action;
    }
    match action {
        SchedulerAction::PublishDisplay {
            publication,
            panel,
            tooltip,
            ..
        } => SchedulerAction::PublishDisplay {
            publication,
            reason: crate::scheduler::PublishReason::FirstPaintTimeout,
            panel,
            tooltip,
        },
        action => action,
    }
}

fn owner_exit_component(
    exit: Option<std::result::Result<OwnerId, tokio::task::JoinError>>,
) -> &'static str {
    match exit {
        Some(Ok(owner)) => owner_name(owner),
        Some(Err(_)) | None => "owner task",
    }
}

fn now(clock: &ProductionClock) -> SchedulerTime {
    SchedulerTime::from_duration(clock.snapshot().monotonic)
}

fn completion_time_after_drain(
    received_at: SchedulerTime,
    observed_after_drain: SchedulerTime,
) -> SchedulerTime {
    received_at.max(observed_after_drain)
}

fn enqueue(actions: &mut VecDeque<SchedulerAction>, transition: Transition) {
    actions.extend(transition.actions);
}

fn prepend(actions: &mut VecDeque<SchedulerAction>, transition: Transition) {
    for action in transition.actions.into_iter().rev() {
        actions.push_front(action);
    }
}

#[cfg(test)]
#[path = "async_loop/tests.rs"]
mod tests;
