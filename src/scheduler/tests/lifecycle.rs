use super::*;

fn started_panel_scheduler() -> (Scheduler, JobId, JobTicket) {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let mut spec = JobSpec::fast(cpu.clone(), Duration::from_secs(1));
    spec.startup_panel = true;
    spec.counter = true;
    spec.volatile = true;
    let mut scheduler = Scheduler::new();
    let transition = startup(
        &mut scheduler,
        config(Duration::from_secs(1), vec![spec], [cpu.clone()]),
    );
    let ticket = first_start(&transition);
    (scheduler, cpu, ticket)
}

fn resume_reconciliation(transition: &Transition) -> ResumeReconciliationId {
    transition
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::RescanHardware {
                kind: RescanKind::VolatileInventoryAndRoute,
                resume_reconciliation,
            } => *resume_reconciliation,
            _ => None,
        })
        .expect("resume reconciliation")
}

#[test]
fn notifications_arm_only_after_first_panel_publication() {
    let (mut scheduler, cpu, ticket) = started_panel_scheduler();
    let first = finish(
        &mut scheduler,
        at_millis(10),
        ticket,
        CompletionKind::Captured,
    );
    assert!(!scheduler.notifications_armed());
    let publication = publications(&first)[0].0;
    let _ = scheduler.handle(SchedulerEvent::PanelPublished {
        at: at_millis(11),
        publication,
    });
    assert!(scheduler.notifications_armed());
    let due = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(12),
        job: cpu,
        trigger: RefreshTrigger::Signal,
    });
    let ticket = first_start(&due);
    let completed = finish(
        &mut scheduler,
        at_millis(13),
        ticket.clone(),
        CompletionKind::Captured,
    );
    assert!(has_notification(&completed, &ticket));
}

#[test]
fn unknown_publication_acknowledgement_does_not_arm_notifications() {
    let (mut scheduler, _cpu, ticket) = started_panel_scheduler();
    let _ = finish(
        &mut scheduler,
        at_millis(10),
        ticket,
        CompletionKind::Captured,
    );
    let rejected = scheduler.handle(SchedulerEvent::PanelPublished {
        at: at_millis(11),
        publication: PublicationId(999),
    });

    assert_eq!(rejected.disposition, EventDisposition::RejectedStale);
    assert!(!scheduler.notifications_armed());
}

#[test]
fn suspend_cancels_running_work_and_pauses_dispatch() {
    let (mut scheduler, _cpu, ticket) = started_panel_scheduler();
    let suspended = scheduler.handle(SchedulerEvent::Suspend { at: at_millis(1) });
    assert!(suspended.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket: cancelled, reason: CancelReason::Suspend } if cancelled == &ticket)
    }));
    assert_eq!(scheduler.next_wake(), None);
    let paused = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_seconds(10) });
    assert!(starts(&paused).is_empty());
}

#[test]
fn resume_resets_counter_baselines_rescans_and_refreshes_demand() {
    let (mut scheduler, cpu, _ticket) = started_panel_scheduler();
    let suspended = scheduler.handle(SchedulerEvent::Suspend { at: at_millis(1) });
    let cancelled = suspended.actions.iter().find_map(|action| match action {
        SchedulerAction::CancelJob { ticket, .. } => Some(ticket.clone()),
        _ => None,
    });
    if let Some(ticket) = cancelled {
        let _ = scheduler.handle(SchedulerEvent::JobCancelled {
            at: at_millis(1),
            ticket,
        });
    }
    let resumed = scheduler.handle(SchedulerEvent::Resume { at: at_seconds(5) });

    let resume_reconciliation = resume_reconciliation(&resumed);
    assert!(resumed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::ResetCounterBaseline { job } if job == &cpu)
    }));
    assert!(starts(&resumed).is_empty());
    let refreshed = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: at_seconds(5),
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: vec![{
                let mut spec = JobSpec::fast(cpu.clone(), Duration::from_secs(1));
                spec.counter = true;
                spec
            }],
            demand: plan([cpu.clone()], [], []),
            resume_acknowledgement: Some(resume_reconciliation),
        },
    });
    assert!(starts(&refreshed).iter().any(|ticket| ticket.job == cpu));
}

#[test]
fn unrelated_inventory_during_resume_does_not_release_reconciliation_gate() {
    let stale = job(OwnerId::Cpu, JobKind::Cpu);
    let reconciled = job(OwnerId::Power, JobKind::SystemBattery);
    let mut scheduler = Scheduler::new();
    let _ = startup(
        &mut scheduler,
        config(Duration::from_secs(1), Vec::new(), []),
    );
    let _ = scheduler.handle(SchedulerEvent::Suspend { at: at_millis(1) });
    let resumed = scheduler.handle(SchedulerEvent::Resume { at: at_millis(2) });
    let resume_reconciliation = resume_reconciliation(&resumed);

    let unrelated = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: at_millis(2),
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: vec![JobSpec::fast(stale.clone(), Duration::from_secs(1))],
            demand: plan([stale], [], []),
            resume_acknowledgement: None,
        },
    });
    assert!(starts(&unrelated).is_empty());

    let acknowledged = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: at_millis(2),
        update: InventoryUpdate {
            generation: InventoryGeneration(3),
            jobs: vec![JobSpec::fast(reconciled.clone(), Duration::from_secs(1))],
            demand: plan([reconciled.clone()], [], []),
            resume_acknowledgement: Some(resume_reconciliation),
        },
    });
    assert_eq!(first_start(&acknowledged).job, reconciled);
}

#[test]
fn resume_waits_for_reconciled_source_and_reanchors_fast_deadline() {
    let old = sourced(OwnerId::Network, JobKind::NetworkRate, "eth0");
    let new = sourced(OwnerId::Network, JobKind::NetworkRate, "wlan0");
    let mut old_spec = JobSpec::fast(old.clone(), Duration::from_secs(1));
    old_spec.counter = true;
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(Duration::from_secs(1), vec![old_spec], [old]),
    );
    let initial_ticket = first_start(&initial);
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        initial_ticket,
        CompletionKind::Captured,
    );
    let _ = scheduler.handle(SchedulerEvent::Suspend { at: at_millis(123) });
    let resumed = scheduler.handle(SchedulerEvent::Resume { at: at_millis(500) });
    assert!(starts(&resumed).is_empty());
    let resume_reconciliation = resume_reconciliation(&resumed);

    let mut new_spec = JobSpec::fast(new.clone(), Duration::from_secs(1));
    new_spec.counter = true;
    let inventory = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: at_millis(500),
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: vec![new_spec],
            demand: plan([new.clone()], [], []),
            resume_acknowledgement: Some(resume_reconciliation),
        },
    });
    let ticket = first_start(&inventory);
    assert_eq!(ticket.job, new);
    let _ = finish(
        &mut scheduler,
        at_millis(501),
        ticket,
        CompletionKind::Captured,
    );
    assert!(
        starts(&scheduler.handle(SchedulerEvent::TimeAdvanced {
            at: at_millis(1_449)
        }))
        .is_empty()
    );
    assert_eq!(
        starts(&scheduler.handle(SchedulerEvent::TimeAdvanced {
            at: at_millis(1_450)
        }))[0]
            .job,
        new
    );
}

#[test]
fn notification_ready_subsample_is_evaluated_after_baseline_completion() {
    let (mut scheduler, _cpu, ticket) = started_panel_scheduler();
    let first = finish(
        &mut scheduler,
        at_millis(1),
        ticket,
        CompletionKind::Captured,
    );
    let publication = publications(&first)[0].0;
    let _ = scheduler.handle(SchedulerEvent::PanelPublished {
        at: at_millis(1),
        publication,
    });
    let refresh = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(2),
        job: job(OwnerId::Cpu, JobKind::Cpu),
        trigger: RefreshTrigger::Signal,
    });
    let ticket = first_start(&refresh);
    let completed = scheduler.handle(SchedulerEvent::JobFinished {
        at: at_millis(3),
        ticket: ticket.clone(),
        completion: CompletionKind::Baseline,
        notification_ready: true,
    });
    assert!(has_notification(&completed, &ticket));
}

#[test]
fn shutdown_cancels_work_terminates_and_rejects_late_completion() {
    let (mut scheduler, _cpu, ticket) = started_panel_scheduler();
    let shutdown = scheduler.handle(SchedulerEvent::Shutdown { at: at_millis(1) });
    assert!(shutdown.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket: cancelled, reason: CancelReason::Shutdown } if cancelled == &ticket)
    }));
    assert!(
        shutdown
            .actions
            .iter()
            .any(|action| { matches!(action, SchedulerAction::Terminate { component: None }) })
    );
    let late = finish(
        &mut scheduler,
        at_millis(2),
        ticket,
        CompletionKind::Captured,
    );
    assert_eq!(late.disposition, EventDisposition::RejectedLifecycle);
}

#[test]
fn critical_failure_names_component_and_stops_scheduler() {
    let (mut scheduler, _cpu, _ticket) = started_panel_scheduler();
    let transition = scheduler.handle(SchedulerEvent::CriticalFailure {
        at: at_millis(1),
        component: "command-service",
    });
    assert!(transition.actions.iter().any(|action| matches!(
        action,
        SchedulerAction::Terminate {
            component: Some("command-service")
        }
    )));
}

#[test]
fn regressing_monotonic_event_is_rejected() {
    let (mut scheduler, _cpu, _ticket) = started_panel_scheduler();
    let _ = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_seconds(1) });
    let rejected = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(999) });
    assert_eq!(
        rejected.disposition,
        EventDisposition::RejectedTimeRegression
    );
    assert!(rejected.actions.is_empty());
}
