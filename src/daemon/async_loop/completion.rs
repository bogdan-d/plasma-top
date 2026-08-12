use std::collections::{BTreeSet, VecDeque};

use crate::adapters::ProductionClock;
use crate::domain::boundary::{ClockSnapshot, FilesystemRoots};
use crate::profiling::AttemptDisposition;
use crate::scheduler::{
    CompletionKind, EventDisposition, InventoryUpdate, JobKind, OwnerId, RefreshTrigger, Scheduler,
    SchedulerAction, SchedulerEvent, SchedulerTime, TimingClass, Transition,
};

use super::state::RuntimeState;
use super::worker::{DispatchValidity, OwnerCompletion, OwnerMessage, RescanInput};
use super::{
    critical_failure, deadline_aware_first_paint, enqueue, now, queue_owner_message,
    request_selected_graph,
};
use crate::daemon::DaemonPaths;

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_completion(
    completion: OwnerCompletion,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    _paths: &DaemonPaths,
    clock: &ProductionClock,
    boot: ClockSnapshot,
    completion_at: SchedulerTime,
    actions: &mut VecDeque<SchedulerAction>,
    owner_messages: &mut VecDeque<(OwnerId, OwnerMessage)>,
    validity: &DispatchValidity,
) {
    match completion {
        OwnerCompletion::Job(mut completion) => {
            let decision = completion.decision.take();
            validity.remove(completion.ticket.run_id);
            let was_current = scheduler.is_current_ticket(&completion.ticket);
            let rerender = was_current
                && completion.ticket.job.kind == JobKind::PageRender
                && !state.render_input_current(&completion);
            let commit_eligible = state.can_commit(scheduler, &completion);
            let mut finished = scheduler.handle(SchedulerEvent::JobFinished {
                at: completion_at,
                ticket: completion.ticket.clone(),
                completion: completion.completion,
                notification_ready: commit_eligible && completion.notification_ready,
            });
            let current = commit_eligible && finished.disposition == EventDisposition::Accepted;
            if let Some(profile) = &state.profile {
                let disposition = if current {
                    AttemptDisposition::Accepted(completion.completion)
                } else if !was_current && finished.disposition == EventDisposition::Accepted {
                    AttemptDisposition::Cancelled
                } else {
                    AttemptDisposition::Rejected
                };
                profile.resolve_attempt(completion.ticket.run_id, disposition);
            }
            let jobs_before_inventory = if current {
                state
                    .scheduler_config()
                    .jobs
                    .into_iter()
                    .map(|spec| spec.id)
                    .collect::<BTreeSet<_>>()
            } else {
                BTreeSet::new()
            };
            let render_generation = state.render_generation;
            let inventory_changed = current && state.commit(&completion);
            if current {
                state.completed_jobs.insert(completion.ticket.job.clone());
                state
                    .deferred_first_paint_jobs
                    .remove(&completion.ticket.job);
            }
            let render_invalidated = current && state.render_generation != render_generation;
            let rendered_page = current && completion.rendered_page.is_some();
            if current && completion.notification_ready {
                state
                    .notification_samples
                    .insert(completion.ticket.run_id, completion.notifications.clone());
            }
            let will_notify = finished.actions.iter().any(|action| {
                matches!(
                    action,
                    SchedulerAction::EvaluateNotifications { ticket }
                        if ticket.run_id == completion.ticket.run_id
                )
            });
            let replacement = finished.actions.iter().find_map(|action| match action {
                SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::PageRender => {
                    Some(ticket.clone())
                }
                _ => None,
            });
            if !will_notify {
                state.notification_samples.remove(&completion.ticket.run_id);
            }
            let mut inventory_transition = None;
            let mut inventory_has_no_followups = false;
            if should_trigger_nvidia_fallback(
                current,
                completion.ticket.job.kind,
                completion.completion,
                &finished,
            ) {
                enqueue(
                    actions,
                    scheduler.handle(SchedulerEvent::RefreshTriggered {
                        at: completion_at,
                        job: crate::scheduler::JobId::singleton(
                            OwnerId::Nvidia,
                            JobKind::NvidiaFallback,
                        ),
                        trigger: RefreshTrigger::Reconnect,
                    }),
                );
            }
            if inventory_changed {
                if let Some(index) = finished.actions.iter().position(|action| {
                    matches!(
                        action,
                        SchedulerAction::PublishDisplay {
                            reason: crate::scheduler::PublishReason::FirstPaintReady,
                            ..
                        }
                    )
                }) {
                    let action = finished.actions.remove(index);
                    state.deferred_first_paint = Some(action);
                    state.refresh_deferred_first_paint_jobs();
                }
                let current_config = state.scheduler_config();
                let (_, selected_page) = state.selected_page();
                let tooltip_demand = current_config.demand.demanded(true, &selected_page);
                let introduced = current_config
                    .jobs
                    .into_iter()
                    .filter(|spec| spec.timing != TimingClass::History)
                    .map(|spec| spec.id)
                    .filter(|job| {
                        tooltip_demand.contains(job) && !jobs_before_inventory.contains(job)
                    })
                    .collect::<BTreeSet<_>>();
                inventory_has_no_followups = introduced.is_empty();
                if !introduced.is_empty() {
                    suppress_tooltip_refresh(&mut finished);
                }
                state.inventory_generation = state.inventory_generation.next();
                let config = state.scheduler_config();
                inventory_transition = Some(scheduler.handle(SchedulerEvent::InventoryChanged {
                    at: completion_at,
                    update: InventoryUpdate {
                        generation: state.inventory_generation,
                        jobs: config.jobs,
                        demand: config.demand,
                        resume_acknowledgement: None,
                    },
                }));
            }
            if let Some(inventory) = inventory_transition {
                enqueue(actions, inventory);
            }
            let replacement_current = replacement
                .as_ref()
                .is_some_and(|ticket| scheduler.is_current_ticket(ticket));
            enqueue(actions, finished);
            reconcile_inventory_publication(
                inventory_has_no_followups,
                state,
                scheduler,
                clock,
                boot,
                completion_at,
                actions,
            );
            if rerender && !replacement_current {
                enqueue(
                    actions,
                    scheduler.handle(SchedulerEvent::RefreshTriggered {
                        at: completion_at,
                        job: completion.ticket.job.clone(),
                        trigger: RefreshTrigger::Signal,
                    }),
                );
            }
            if render_invalidated && !replacement_current {
                request_selected_graph(scheduler, state, clock, actions);
            }
            if rendered_page
                && !actions.iter().any(|action| {
                    matches!(
                        action,
                        SchedulerAction::PublishDisplay { tooltip: true, .. }
                    )
                })
            {
                enqueue(
                    actions,
                    scheduler.handle(SchedulerEvent::TooltipRefreshRequested { at: completion_at }),
                );
            }
            if let Some(decision) = decision {
                let _ = decision.send(current);
            }
        }
        OwnerCompletion::Cancelled(ticket) => {
            validity.remove(ticket.run_id);
            if let Some(profile) = &state.profile {
                profile.resolve_attempt(ticket.run_id, AttemptDisposition::Cancelled);
            }
            enqueue(
                actions,
                scheduler.handle(SchedulerEvent::JobCancelled {
                    at: now(clock),
                    ticket,
                }),
            );
        }
        OwnerCompletion::Rescan {
            config_generation,
            inventory_generation,
            lifecycle_generation,
            resume_reconciliation,
            hw,
        } => {
            if config_generation == state.config_generation
                && inventory_generation == state.inventory_generation
                && lifecycle_generation == validity.lifecycle_generation()
            {
                let hardware_changed = state.hw != *hw;
                state.hw = *hw;
                if hardware_changed {
                    state.rendered_graph = None;
                    state.render_generation = state.render_generation.saturating_add(1);
                }
                state.inventory_generation = state.inventory_generation.next();
                let config = state.scheduler_config();
                enqueue(
                    actions,
                    scheduler.handle(SchedulerEvent::InventoryChanged {
                        at: now(clock),
                        update: InventoryUpdate {
                            generation: state.inventory_generation,
                            jobs: config.jobs,
                            demand: config.demand,
                            resume_acknowledgement: resume_reconciliation,
                        },
                    }),
                );
                state.refresh_deferred_first_paint_jobs();
                if hardware_changed {
                    request_selected_graph(scheduler, state, clock, actions);
                    enqueue(
                        actions,
                        scheduler
                            .handle(SchedulerEvent::DisplayRefreshRequested { at: now(clock) }),
                    );
                }
            } else if lifecycle_generation == validity.lifecycle_generation()
                && !queue_owner_message(
                    owner_messages,
                    (
                        OwnerId::Discovery,
                        OwnerMessage::Rescan(Box::new(RescanInput {
                            kind: crate::scheduler::RescanKind::VolatileInventoryAndRoute,
                            cfg: state.cfg.clone(),
                            hw: state.hw.clone(),
                            config_generation: state.config_generation,
                            inventory_generation: state.inventory_generation,
                            lifecycle_generation: validity.lifecycle_generation(),
                            resume_reconciliation,
                        })),
                    ),
                )
            {
                critical_failure("owner pending queue", scheduler, clock, actions);
            }
        }
        OwnerCompletion::Theme { stamp, light } => {
            if stamp == state.kde_stamp && state.apply_theme(light) {
                enqueue(
                    actions,
                    scheduler.handle(SchedulerEvent::DisplayRefreshRequested { at: now(clock) }),
                );
                request_selected_graph(scheduler, state, clock, actions);
            }
        }
    }
    let _ = roots;
}

pub(super) fn transition_starts_job(transition: &Transition, kind: JobKind) -> bool {
    transition.actions.iter().any(
        |action| matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == kind),
    )
}

pub(super) fn should_trigger_nvidia_fallback(
    current: bool,
    kind: JobKind,
    completion: CompletionKind,
    finished: &Transition,
) -> bool {
    current
        && kind == JobKind::NvidiaNvml
        && completion == CompletionKind::Failed
        && !transition_starts_job(finished, JobKind::NvidiaFallback)
}

fn release_deferred_first_paint(
    state: &mut RuntimeState,
    actions: &mut VecDeque<SchedulerAction>,
    clock: &ProductionClock,
    boot: ClockSnapshot,
) -> bool {
    if !state.deferred_first_paint_jobs.is_empty() {
        return false;
    }
    let Some(publication) = state.deferred_first_paint.take() else {
        return false;
    };
    actions.push_back(deadline_aware_first_paint(publication, clock, boot));
    true
}

#[allow(clippy::too_many_arguments)]
pub(super) fn reconcile_inventory_publication(
    inventory_has_no_followups: bool,
    state: &mut RuntimeState,
    scheduler: &mut Scheduler,
    clock: &ProductionClock,
    boot: ClockSnapshot,
    at: SchedulerTime,
    actions: &mut VecDeque<SchedulerAction>,
) {
    let released_first_paint = release_deferred_first_paint(state, actions, clock, boot);
    if !released_first_paint && !inventory_has_no_followups {
        return;
    }
    if actions
        .iter()
        .any(|action| matches!(action, SchedulerAction::PublishDisplay { panel: true, .. }))
    {
        if actions.iter().any(|action| {
            matches!(
                action,
                SchedulerAction::PublishDisplay {
                    panel: true,
                    tooltip: true,
                    ..
                }
            )
        }) {
            remove_tooltip_only_publications(actions);
        }
        return;
    }
    if !inventory_has_no_followups {
        return;
    }
    remove_tooltip_only_publications(actions);
    enqueue(
        actions,
        scheduler.handle(SchedulerEvent::DisplayRefreshRequested { at }),
    );
}

fn remove_tooltip_only_publications(actions: &mut VecDeque<SchedulerAction>) {
    actions.retain(|action| {
        !matches!(
            action,
            SchedulerAction::PublishDisplay {
                panel: false,
                tooltip: true,
                ..
            }
        )
    });
}

pub(super) fn suppress_tooltip_refresh(transition: &mut Transition) {
    transition.actions.retain(|action| {
        !matches!(
            action,
            SchedulerAction::PublishDisplay {
                reason: crate::scheduler::PublishReason::TooltipRefresh,
                ..
            }
        )
    });
}
