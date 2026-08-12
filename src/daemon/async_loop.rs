use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::{Semaphore, mpsc};
use tokio::task::JoinSet;

use crate::adapters::{
    ProductionClock, ProductionCommandRunner, ProductionDbusFacade, ProductionIoEvents,
    ProductionNotificationFacade,
};
use crate::config::{cache_live_geom, load_config};
use crate::domain::boundary::{
    ClockSnapshot, CommandRunner, DbusFacade, FilesystemRoots, NotificationFacade,
    NotificationPayload,
};
use crate::error::{Error, Result};
use crate::file_watch::FileWatcher;
use crate::notify::check_and_notify;
use crate::page_commands::build_pages;
use crate::profiling::{
    ProfileSession, ProfileStimuli, ProfileStimulusConfig, ProfileStimulusState,
};
use crate::scheduler::{
    InventoryUpdate, JobKind, OwnerId, PageId, RefreshTrigger, Scheduler, SchedulerAction,
    SchedulerEvent, SchedulerTime, Transition,
};
use crate::sensors::{discover_local_hardware, discover_local_hardware_attempt};

use super::{DaemonPaths, cleanup, publish_pages, write_atomic};

#[path = "async_loop/completion.rs"]
mod completion;
#[path = "async_loop/control.rs"]
mod control;
#[path = "async_loop/files.rs"]
mod files;
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
#[cfg(test)]
use control::wait_for_wake;
use control::{
    LoopWake, preempt_for_shutdown, process_io_events, sleep_duration, wait_for_wake_with_files,
};
use state::RuntimeState;
use worker::{DispatchValidity, JobInput, OwnerMessage, OwnerSenders, RescanInput};

const NOTIFICATION_CHANNEL_CAPACITY: usize = 8;
const BLOCKING_LANE_CAPACITY: usize = 1;
const OWNER_PENDING_CAPACITY: usize = 1024;
const FIRST_PAINT_DEADLINE: Duration = Duration::from_millis(200);

pub(super) struct DaemonServices<
    C = ProductionCommandRunner,
    D = ProductionDbusFacade,
    N = ProductionNotificationFacade,
> {
    pub(super) commands: C,
    pub(super) dbus: D,
    pub(super) notifications: N,
    pub(super) stopped: Arc<AtomicBool>,
    pub(super) events: ProductionIoEvents,
    pub(super) clock: ProductionClock,
    pub(super) shutdown: tokio::sync::watch::Receiver<bool>,
    #[cfg(test)]
    pub(super) blocked_owner: Option<worker::TestOwnerBlock>,
}

pub(super) struct ProfileRun {
    pub(super) duration: Duration,
    pub(super) presented: bool,
    pub(super) selected_page: PageId,
    pub(super) stimuli: bool,
    pub(super) session: Arc<ProfileSession>,
    pub(super) config_stimulus: ProfileConfigStimulus,
    pub(super) shutdown: tokio::sync::watch::Sender<bool>,
}

pub(super) struct ProfileConfigStimulus {
    pub(super) path: std::path::PathBuf,
    pub(super) original: Vec<u8>,
    pub(super) alternate: Vec<u8>,
}

pub(super) async fn run<C, D, N>(
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    services: DaemonServices<C, D, N>,
    profile_run: Option<ProfileRun>,
) -> Result<()>
where
    C: CommandRunner + Clone + Send + 'static,
    D: DbusFacade + Clone + Send + 'static,
    N: NotificationFacade + Clone + Send + 'static,
{
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
    if profile_run.is_none() {
        cleanup(paths);
        write_atomic(&paths.page, "0")?;
    }

    let boot = clock.snapshot();
    let cfg = load_config(config_path, None)?;
    let cpu_count = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let hw = discover_local_hardware(&roots.sys_root, &roots.proc_root, &cfg, cpu_count);
    let active = if profile_run.is_some() {
        build_pages(&cfg.pages.order)
    } else {
        publish_pages(paths, &cfg)?
    };
    if let Some(profile) = &profile_run
        && profile.selected_page != PageId::Main
        && !active
            .iter()
            .any(|page| PageId::from_id(page.id) == profile.selected_page)
    {
        return Err(Error::Runtime(format!(
            "profiling scenario page '{}' is not configured in pages.order",
            profile.selected_page.as_id()
        )));
    }
    let selected_index = profile_run.as_ref().map_or(0, |profile| {
        active
            .iter()
            .position(|page| PageId::from_id(page.id) == profile.selected_page)
            .unwrap_or(0)
    });
    if let Some(profile) = &profile_run {
        write_atomic(&paths.page, &selected_index.to_string())?;
        write_atomic(&paths.npages, &active.len().to_string())?;
        let presented = paths.state.join("presented");
        fs::create_dir_all(&presented)?;
        if profile.presented {
            write_atomic(&presented.join("1"), "")?;
        }
    }
    let mut state = RuntimeState::new(paths, cfg, hw, active);
    if let Some(profile) = &profile_run {
        state.configure_profiling(Arc::clone(&profile.session));
    }
    let (watch_path, machine_paths) = state::config_watch_paths(config_path);
    let mut file_watcher = FileWatcher::new(files::watch_targets(
        &watch_path,
        &machine_paths,
        paths,
        &state,
    ))
    .map_err(|error| Error::Runtime(format!("file watch installation failed: {error}")))?;
    let mut presentation = files::presentation_status(paths, &clock)?;
    let mut stimuli = profile_run
        .as_ref()
        .filter(|profile| profile.stimuli)
        .map(|profile| {
            ProfileStimuli::new(ProfileStimulusConfig {
                ready_at: clock.snapshot().monotonic,
                initially_presented: profile.presented,
                selected_page: selected_index,
                page_count: state.active.len(),
                pages: state
                    .active
                    .iter()
                    .map(|page| PageId::from_id(page.id))
                    .collect(),
                config_path: profile.config_stimulus.path.clone(),
                config_original: profile.config_stimulus.original.clone(),
                config_alternate: profile.config_stimulus.alternate.clone(),
                config_generation: state.config_generation,
                overlay: state.cfg.display.overlay,
                session: Arc::clone(&profile.session),
            })
        });
    let blocking_lane = Arc::new(Semaphore::new(BLOCKING_LANE_CAPACITY));
    let (completion_sender, mut completions) = mpsc::channel(worker::COMPLETION_CHANNEL_CAPACITY);
    let mut owner_tasks = JoinSet::new();
    let validity = DispatchValidity::default();
    let owner_senders = worker::spawn_owners(
        &mut owner_tasks,
        worker::OwnerServices {
            commands,
            dbus,
            profile: profile_run
                .as_ref()
                .map(|profile| Arc::clone(&profile.session)),
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
            presented: presentation.leases.presented,
        }),
    );
    let (_, selected) = state.selected_page();
    enqueue(
        &mut actions,
        scheduler.handle(SchedulerEvent::SelectedPageChanged {
            at: now(&clock),
            page: selected,
        }),
    );
    if profile_run.is_none() {
        cache_live_geom();
    }
    if profile_run.is_none() {
        let armed_sources = file_watcher.rescan_sources();
        files::process_and_stabilize(
            armed_sources,
            &mut file_watcher,
            &watch_path,
            &machine_paths,
            config_path,
            roots,
            paths,
            cpu_count,
            &mut scheduler,
            &mut state,
            &clock,
            &mut actions,
            &mut owner_messages,
            &mut presentation,
        )?;
    }
    let profile_deadline = profile_run
        .as_ref()
        .map(|profile| boot.monotonic.saturating_add(profile.duration));
    let mut profile_shutdown_requested = false;

    loop {
        let loop_now = clock.snapshot().monotonic;
        if !profile_shutdown_requested
            && profile_deadline.is_some_and(|deadline| loop_now >= deadline)
        {
            profile_shutdown_requested = true;
            if let Some(stimuli) = &stimuli {
                let (page_index, page) = state.selected_page();
                stimuli.time_out(ProfileStimulusState {
                    presented: presentation.leases.presented,
                    page_index,
                    page,
                    config_generation: state.config_generation,
                    overlay: state.cfg.display.overlay,
                });
            }
            if let Some(profile) = &profile_run {
                profile.session.record_shutdown_started();
                let _ = profile.shutdown.send(true);
            }
            enqueue(
                &mut actions,
                scheduler.handle(SchedulerEvent::Shutdown { at: now(&clock) }),
            );
        }
        if !profile_shutdown_requested && let Some(stimuli) = &mut stimuli {
            let _ = stimuli.next_due(loop_now);
            stimuli.inject_due(
                loop_now,
                paths,
                &clock,
                state.config_generation,
                state.cfg.display.overlay,
            )?;
        }
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
        let advanced_at = now(&clock);
        enqueue(
            &mut actions,
            scheduler.handle(SchedulerEvent::TimeAdvanced { at: advanced_at }),
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
        if presentation
            .deadline
            .is_some_and(|expiry| expiry <= clock.snapshot().monotonic)
        {
            files::update_presentation(
                &mut presentation,
                paths,
                &clock,
                &mut scheduler,
                &mut actions,
                state.profile.as_deref(),
            )?;
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

        let snapshot = clock.snapshot();
        let mut sleep_for = presentation.deadline.map_or_else(
            || sleep_duration(&scheduler, &state, boot, snapshot),
            |expiry| {
                sleep_duration(&scheduler, &state, boot, snapshot)
                    .min(expiry.saturating_sub(snapshot.monotonic))
            },
        );
        if let Some(deadline) = profile_deadline {
            sleep_for = sleep_for.min(deadline.saturating_sub(snapshot.monotonic));
        }
        if let Some(due) = stimuli
            .as_mut()
            .and_then(|stimuli| stimuli.next_due(snapshot.monotonic))
        {
            sleep_for = sleep_for.min(due.saturating_sub(snapshot.monotonic));
        }
        let scheduled_wake = scheduler.next_wake();
        match wait_for_wake_with_files(
            &mut owner_tasks,
            &mut notification_task,
            &mut shutdown,
            &mut completions,
            &mut file_watcher,
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
            LoopWake::Shutdown => {}
            LoopWake::Sleep => {
                if let (Some(profile), Some(due)) = (&profile_run, scheduled_wake) {
                    let at = now(&clock);
                    if due <= at {
                        profile.session.record_wake(due.duration(), at.duration());
                    }
                }
            }
            LoopWake::Files(changed) => {
                let changed = changed
                    .map_err(|error| Error::Runtime(format!("file watch failed: {error}")))?;
                files::process_and_stabilize(
                    changed,
                    &mut file_watcher,
                    &watch_path,
                    &machine_paths,
                    config_path,
                    roots,
                    paths,
                    cpu_count,
                    &mut scheduler,
                    &mut state,
                    &clock,
                    &mut actions,
                    &mut owner_messages,
                    &mut presentation,
                )?;
            }
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
    if profile_run.is_none() {
        cleanup(paths);
    }
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
                    state.mark_graph_render_requested(&ticket);
                    let (selected_index, _) = state.selected_page();
                    if let Some(profile) = &state.profile {
                        profile.record_job_queued(
                            ticket.run_id,
                            &ticket.job,
                            clock.snapshot().monotonic,
                        );
                    }
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
                state.clear_graph_render_requested(&ticket);
                state.notification_samples.remove(&ticket.run_id);
                let was_dispatched = validity.remove(ticket.run_id);
                let before = owner_messages.len();
                owner_messages.retain(|(_, message)| {
                    message
                        .ticket()
                        .is_none_or(|queued| queued.run_id != ticket.run_id)
                });
                if owner_messages.len() != before
                    && let Some(profile) = &state.profile
                {
                    profile.resolve_attempt(
                        ticket.run_id,
                        crate::profiling::AttemptDisposition::Cancelled,
                    );
                }
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
                if let Some(profile) = &state.profile {
                    profile.invalidate_capture(&job);
                }
                state.invalidate_job_readings(&job);
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
                display_deadline,
                skipped_display_deadlines,
                panel,
                tooltip,
            } => {
                if panel
                    && state.suppress_panel_publication_before_first_paint(
                        reason,
                        skipped_display_deadlines,
                    )
                {
                    continue;
                }
                let snapshot = clock.snapshot();
                if let Some(acknowledgement) = state.publish(
                    publication,
                    reason,
                    display_deadline,
                    skipped_display_deadlines,
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
                let graph_start_queued = actions.iter().any(|action| {
                    matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::PageRender)
                });
                if tooltip && !graph_start_queued && state.selected_graph_needs_render() {
                    request_selected_graph(scheduler, state, clock, actions);
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
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
) {
    let (_, selected) = state.selected_page();
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
            display_deadline,
            skipped_display_deadlines,
            panel,
            tooltip,
            ..
        } => SchedulerAction::PublishDisplay {
            publication,
            reason: crate::scheduler::PublishReason::FirstPaintTimeout,
            display_deadline,
            skipped_display_deadlines,
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
