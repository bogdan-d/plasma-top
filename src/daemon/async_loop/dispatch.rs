use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn drain_actions(
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

pub(super) fn flush_owner_messages(
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

pub(super) fn queue_owner_message(
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
