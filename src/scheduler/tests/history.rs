use super::*;

fn history_setup() -> (Scheduler, JobId, JobId, JobTicket) {
    let source = job(OwnerId::Cpu, JobKind::Cpu);
    let history = job(OwnerId::Cpu, JobKind::CpuHistory);
    let specs = vec![
        JobSpec::triggered(source.clone(), Duration::from_secs(1)),
        JobSpec::history(history.clone(), source.clone(), Duration::from_secs(1)),
    ];
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            specs,
            [source.clone(), history.clone()],
        ),
    );
    let source_ticket = first_start(&initial);
    (scheduler, source, history, source_ticket)
}

#[test]
fn history_appends_nothing_before_first_valid_sample() {
    let (mut scheduler, _source, history, source_ticket) = history_setup();
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        source_ticket,
        CompletionKind::Baseline,
    );

    let due = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_seconds(1) });
    assert!(starts(&due).iter().all(|ticket| ticket.job != history));
}

#[test]
fn history_uses_carried_sample_after_transient_failure() {
    let (mut scheduler, source, history, source_ticket) = history_setup();
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        source_ticket,
        CompletionKind::Captured,
    );
    let first_history = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_seconds(1) });
    let history_ticket = starts(&first_history)
        .into_iter()
        .find(|ticket| ticket.job == history)
        .expect("history start");
    let _ = finish(
        &mut scheduler,
        at_seconds(1),
        history_ticket,
        CompletionKind::Captured,
    );
    let trigger = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(1_100),
        job: source.clone(),
        trigger: RefreshTrigger::Signal,
    });
    let failed = first_start(&trigger);
    let _ = finish(
        &mut scheduler,
        at_millis(1_100),
        failed,
        CompletionKind::Failed,
    );

    let due = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_seconds(2) });
    let started = starts(&due);
    let history_started = if started.iter().any(|ticket| ticket.job == history) {
        started
    } else {
        let source_retry = started
            .into_iter()
            .find(|ticket| ticket.job == source)
            .expect("source retry before same-owner history");
        starts(&finish(
            &mut scheduler,
            at_seconds(2),
            source_retry,
            CompletionKind::Failed,
        ))
    };
    assert!(history_started.iter().any(|ticket| ticket.job == history));
}

#[test]
fn history_skips_missed_deadlines_without_synthetic_points() {
    let (mut scheduler, _source, history, source_ticket) = history_setup();
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        source_ticket,
        CompletionKind::Captured,
    );

    let late = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: at_millis(5_500),
    });
    let history_starts = starts(&late)
        .into_iter()
        .filter(|ticket| ticket.job == history)
        .collect::<Vec<_>>();
    assert_eq!(history_starts.len(), 1);
    assert_eq!(
        history_starts[0].history_deadline,
        Some(HistoryDeadline::new(at_seconds(5)))
    );
    let completed = finish(
        &mut scheduler,
        at_millis(5_501),
        history_starts[0].clone(),
        CompletionKind::Captured,
    );
    assert!(
        starts(&completed)
            .iter()
            .all(|ticket| ticket.job != history)
    );
    assert_eq!(scheduler.next_wake(), Some(at_seconds(6)));
}

#[test]
fn history_ticket_keeps_nominal_cutoff_while_new_source_capture_finishes() {
    let (mut scheduler, source, history, source_ticket) = history_setup();
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        source_ticket,
        CompletionKind::Captured,
    );
    let refresh = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(900),
        job: source,
        trigger: RefreshTrigger::Signal,
    });
    let source_refresh = first_start(&refresh);
    let due = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_seconds(1) });
    assert!(starts(&due).is_empty());

    let completed = finish(
        &mut scheduler,
        at_millis(1_100),
        source_refresh,
        CompletionKind::Captured,
    );
    let history_ticket = starts(&completed)
        .into_iter()
        .find(|ticket| ticket.job == history)
        .expect("history starts after source owner releases");
    assert_eq!(
        history_ticket.history_deadline,
        Some(HistoryDeadline::new(at_seconds(1)))
    );
}

#[test]
fn activation_trigger_and_demand_refresh_never_start_history_early() {
    let source = job(OwnerId::Cpu, JobKind::Cpu);
    let history = job(OwnerId::Cpu, JobKind::CpuHistory);
    let specs = vec![
        JobSpec::triggered(source.clone(), Duration::from_secs(1)),
        JobSpec::history(history.clone(), source.clone(), Duration::from_secs(1)),
    ];
    let mut scheduler = Scheduler::new();
    let mut cfg = config(Duration::from_secs(10), specs.clone(), [source.clone()]);
    cfg.demand.main_tooltip.insert(history.clone());
    let initial = startup(&mut scheduler, cfg);
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        first_start(&initial),
        CompletionKind::Captured,
    );

    let activated = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(100),
        presented: true,
    });
    assert!(
        starts(&activated)
            .iter()
            .all(|ticket| ticket.job != history)
    );
    let triggered = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(200),
        job: history.clone(),
        trigger: RefreshTrigger::Signal,
    });
    assert!(
        starts(&triggered)
            .iter()
            .all(|ticket| ticket.job != history)
    );
    let changed = scheduler.handle(SchedulerEvent::DemandChanged {
        at: at_millis(300),
        demand: plan([source.clone(), history.clone()], [], []),
    });
    assert!(starts(&changed).iter().all(|ticket| ticket.job != history));

    let mut replacement = config(Duration::from_secs(10), specs, [source, history.clone()]);
    replacement.generation = ConfigGeneration(2);
    let replaced = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: at_millis(400),
        config: replacement,
    });
    assert!(starts(&replaced).iter().all(|ticket| ticket.job != history));
}

#[test]
fn confirmed_source_absence_invalidates_dependent_history() {
    let (mut scheduler, source, history, source_ticket) = history_setup();
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        source_ticket,
        CompletionKind::Captured,
    );
    let trigger = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(10),
        job: source,
        trigger: RefreshTrigger::Reconnect,
    });
    let absent = finish(
        &mut scheduler,
        at_millis(10),
        first_start(&trigger),
        CompletionKind::ConfirmedAbsent,
    );

    assert!(absent.actions.iter().any(|action| {
        matches!(action, SchedulerAction::InvalidateJob { job, .. } if job == &history)
    }));
    assert!(!scheduler.has_sample(&history));
}
