use super::*;

#[test]
fn repeated_due_events_coalesce_to_one_pending_follow_up() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let cores = job(OwnerId::Cpu, JobKind::CpuCores);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            vec![
                JobSpec::periodic(cpu.clone(), Duration::from_secs(1)),
                JobSpec::periodic(cores.clone(), Duration::from_secs(1)),
            ],
            [cpu.clone(), cores.clone()],
        ),
    );
    let running = first_start(&initial);
    assert_eq!(running.job, cpu);

    for seconds in 1..=5 {
        let transition = scheduler.handle(SchedulerEvent::TimeAdvanced {
            at: at_seconds(seconds),
        });
        assert!(starts(&transition).is_empty());
    }
    let follow_up = finish(
        &mut scheduler,
        at_millis(5_001),
        running,
        CompletionKind::Captured,
    );
    let started = starts(&follow_up);
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].job, cores);
    let next = finish(
        &mut scheduler,
        at_millis(5_002),
        started[0].clone(),
        CompletionKind::Captured,
    );
    assert_eq!(starts(&next).len(), 1);
    assert_eq!(starts(&next)[0].job, cpu);
}

#[test]
fn replacement_waits_for_cancellation_ack_and_invalidates_queued_start() {
    let old = sourced(OwnerId::Network, JobKind::NetworkRate, "eth0");
    let new = sourced(OwnerId::Network, JobKind::NetworkRate, "wlan0");
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(1),
            vec![JobSpec::fast(old.clone(), Duration::from_secs(1))],
            [old],
        ),
    );
    let obsolete = first_start(&initial);
    let changed = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: at_millis(1),
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: vec![JobSpec::fast(new.clone(), Duration::from_secs(1))],
            demand: plan([new.clone()], [], []),
        },
    });
    assert!(starts(&changed).is_empty());
    assert!(!scheduler.is_current_ticket(&obsolete));

    let acknowledged = scheduler.handle(SchedulerEvent::JobCancelled {
        at: at_millis(1),
        ticket: obsolete,
    });
    assert_eq!(first_start(&acknowledged).job, new);
}

#[test]
fn completion_racing_config_cancellation_releases_owner_without_committing() {
    let memory = job(OwnerId::Memory, JobKind::Memory);
    let spec = JobSpec::periodic(memory.clone(), Duration::from_secs(1));
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(Duration::from_secs(1), vec![spec.clone()], [memory.clone()]),
    );
    let obsolete = first_start(&initial);
    let mut replacement = config(Duration::from_secs(1), vec![spec], [memory.clone()]);
    replacement.generation = ConfigGeneration(2);
    let changed = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: at_millis(1),
        config: replacement,
    });
    assert!(changed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &obsolete)
    }));

    let completed = finish(
        &mut scheduler,
        at_millis(2),
        obsolete.clone(),
        CompletionKind::Captured,
    );
    assert_eq!(completed.disposition, EventDisposition::Accepted);
    assert_eq!(first_start(&completed).job, memory);
    assert!(completed.actions.iter().all(|action| {
        !matches!(
            action,
            SchedulerAction::PublishDisplay { .. }
                | SchedulerAction::EvaluateNotifications { .. }
                | SchedulerAction::InvalidateJob { .. }
                | SchedulerAction::ApplyBackoff { .. }
        )
    }));
    let stale_ack = scheduler.handle(SchedulerEvent::JobCancelled {
        at: at_millis(2),
        ticket: obsolete,
    });
    assert_eq!(stale_ack.disposition, EventDisposition::RejectedStale);
    assert!(stale_ack.actions.is_empty());
}

#[test]
fn cancellation_acknowledgement_processes_elapsed_display_and_activation_deadlines() {
    let tooltip = job(OwnerId::External, JobKind::Brightness);
    let spec = JobSpec::periodic(tooltip.clone(), Duration::from_secs(1));
    let demand = plan([], [tooltip.clone()], []);
    let mut scheduler = Scheduler::new();
    let mut initial_config = config(Duration::from_millis(100), vec![spec.clone()], []);
    initial_config.demand = demand.clone();
    let _ = startup(&mut scheduler, initial_config);
    let activated = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let obsolete = first_start(&activated);

    let changed = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: at_millis(1),
        config: SchedulerConfig {
            generation: ConfigGeneration(2),
            display_interval: Duration::from_millis(100),
            jobs: vec![spec],
            demand,
        },
    });
    assert!(starts(&changed).is_empty());

    let acknowledged = scheduler.handle(SchedulerEvent::JobCancelled {
        at: at_millis(101),
        ticket: obsolete,
    });
    assert!(
        publications(&acknowledged)
            .iter()
            .any(
                |(_, reason, panel, tooltip)| *reason == PublishReason::DisplayDeadline
                    && *panel
                    && *tooltip
            )
    );
    assert_eq!(first_start(&acknowledged).job, tooltip);
    let publication_position = acknowledged
        .actions
        .iter()
        .position(|action| matches!(action, SchedulerAction::PublishDisplay { .. }))
        .expect("elapsed publication");
    let start_position = acknowledged
        .actions
        .iter()
        .position(|action| matches!(action, SchedulerAction::StartJob { .. }))
        .expect("replacement start");
    assert!(publication_position < start_position);
    let same_time_follow_up = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(101) });
    assert!(
        publications(&same_time_follow_up)
            .iter()
            .all(|(_, reason, _, _)| *reason != PublishReason::TooltipRefresh)
    );
}

#[test]
fn cancellation_racing_completion_processes_first_paint_without_committing() {
    let memory = job(OwnerId::Memory, JobKind::Memory);
    let mut spec = JobSpec::periodic(memory.clone(), Duration::from_secs(1));
    spec.startup_panel = true;
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_millis(100),
            vec![spec.clone()],
            [memory.clone()],
        ),
    );
    let obsolete = first_start(&initial);
    let mut replacement = config(Duration::from_millis(100), vec![spec], [memory.clone()]);
    replacement.generation = ConfigGeneration(2);
    let _ = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: at_millis(1),
        config: replacement,
    });

    let completed = finish(
        &mut scheduler,
        at_millis(200),
        obsolete,
        CompletionKind::Captured,
    );
    assert_eq!(
        publications(&completed)[0].1,
        PublishReason::FirstPaintTimeout
    );
    assert_eq!(first_start(&completed).job, memory);
    assert!(!scheduler.has_sample(&memory));
    assert!(completed.actions.iter().all(|action| {
        !matches!(
            action,
            SchedulerAction::EvaluateNotifications { .. } | SchedulerAction::ApplyBackoff { .. }
        )
    }));
}

#[test]
fn unrelated_inventory_update_preserves_backoff_and_failure_count() {
    let updates = job(OwnerId::External, JobKind::UpdatesFile);
    let spec = JobSpec::periodic(updates.clone(), Duration::from_secs(1));
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            vec![spec.clone()],
            [updates.clone()],
        ),
    );
    let failed = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        first_start(&initial),
        CompletionKind::Failed,
    );
    assert!(starts(&failed).is_empty());

    let replaced = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: at_millis(50),
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: vec![spec],
            demand: plan([updates.clone()], [], []),
        },
    });
    assert!(starts(&replaced).is_empty());
    assert!(
        starts(&scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(99) })).is_empty()
    );
    let retry = first_start(&scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(100) }));
    let failed_again = finish(
        &mut scheduler,
        at_millis(100),
        retry,
        CompletionKind::Failed,
    );
    assert!(failed_again.actions.iter().any(|action| {
        matches!(
            action,
            SchedulerAction::ApplyBackoff {
                job,
                failures: 2,
                retry_at,
            } if job == &updates && *retry_at == at_millis(300)
        )
    }));
}

#[test]
fn unrelated_inventory_update_preserves_pending_age() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let cores = job(OwnerId::Cpu, JobKind::CpuCores);
    let specs = vec![
        JobSpec::periodic(cpu.clone(), Duration::from_secs(1)),
        JobSpec::periodic(cores.clone(), Duration::from_secs(1)),
    ];
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            specs.clone(),
            [cpu.clone(), cores.clone()],
        ),
    );
    let obsolete = first_start(&initial);
    let _ = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(10),
        job: cpu,
        trigger: RefreshTrigger::Signal,
    });
    let changed = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: at_millis(20),
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: specs,
            demand: plan([obsolete.job.clone(), cores.clone()], [], []),
        },
    });
    assert!(starts(&changed).is_empty());
    let acknowledged = scheduler.handle(SchedulerEvent::JobCancelled {
        at: at_millis(20),
        ticket: obsolete,
    });
    assert_eq!(first_start(&acknowledged).job, cores);
}

#[test]
fn stale_completion_cannot_advance_deadlines_or_publish() {
    let memory = job(OwnerId::Memory, JobKind::Memory);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(1),
            vec![JobSpec::fast(memory.clone(), Duration::from_secs(1))],
            [memory],
        ),
    );
    let mut stale = first_start(&initial);
    stale.config_generation = ConfigGeneration(99);
    let rejected = finish(
        &mut scheduler,
        at_seconds(10),
        stale,
        CompletionKind::Captured,
    );
    assert_eq!(rejected.disposition, EventDisposition::RejectedStale);
    assert!(rejected.actions.is_empty());
    assert_eq!(scheduler.next_wake(), Some(at_millis(950)));
}

#[test]
fn replaced_source_completion_terminally_releases_cancelling_reservation() {
    let old = JobId::with_source(
        OwnerId::Network,
        JobKind::NetworkRate,
        SourceIdentity::Device("eth0".to_owned()),
    );
    let new = JobId::with_source(
        OwnerId::Network,
        JobKind::NetworkRate,
        SourceIdentity::Device("wlan0".to_owned()),
    );
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(1),
            vec![JobSpec::periodic(old.clone(), Duration::from_secs(1))],
            [old],
        ),
    );
    let obsolete = first_start(&initial);
    let update = InventoryUpdate {
        generation: InventoryGeneration(2),
        jobs: vec![JobSpec::periodic(new.clone(), Duration::from_secs(1))],
        demand: plan([new.clone()], [], []),
    };
    let changed = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: at_millis(1),
        update,
    });
    assert!(changed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, reason: CancelReason::SourceReplaced } if ticket == &obsolete)
    }));

    let completed = finish(
        &mut scheduler,
        at_millis(2),
        obsolete.clone(),
        CompletionKind::Captured,
    );
    assert_eq!(completed.disposition, EventDisposition::Accepted);
    assert_eq!(first_start(&completed).job, new);
    assert!(!has_notification(&completed, &obsolete));
    let stale_ack = scheduler.handle(SchedulerEvent::JobCancelled {
        at: at_millis(2),
        ticket: obsolete,
    });
    assert_eq!(stale_ack.disposition, EventDisposition::RejectedStale);
}

#[test]
fn duplicate_completion_is_rejected() {
    let updates = job(OwnerId::External, JobKind::UpdatesFile);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            vec![JobSpec::triggered(updates.clone(), Duration::from_secs(1))],
            [updates],
        ),
    );
    let ticket = first_start(&initial);
    let first = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        ticket.clone(),
        CompletionKind::Captured,
    );
    assert_eq!(first.disposition, EventDisposition::Accepted);
    let duplicate = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        ticket,
        CompletionKind::Captured,
    );
    assert_eq!(duplicate.disposition, EventDisposition::RejectedStale);
}

#[test]
fn history_is_invalidated_when_its_source_identity_changes() {
    let old_source = sourced(OwnerId::Network, JobKind::NetworkRate, "eth0");
    let new_source = sourced(OwnerId::Network, JobKind::NetworkRate, "wlan0");
    let history = job(OwnerId::Network, JobKind::NetworkHistory);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            vec![
                JobSpec::triggered(old_source.clone(), Duration::from_secs(1)),
                JobSpec::history(history.clone(), old_source.clone(), Duration::from_secs(1)),
            ],
            [old_source, history.clone()],
        ),
    );
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        first_start(&initial),
        CompletionKind::Captured,
    );
    let history_due = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_seconds(1) });
    let history_ticket = starts(&history_due)
        .into_iter()
        .find(|ticket| ticket.job == history)
        .expect("history start");
    let _ = finish(
        &mut scheduler,
        at_seconds(1),
        history_ticket,
        CompletionKind::Captured,
    );
    assert!(scheduler.has_sample(&history));

    let mut replacement = config(
        Duration::from_secs(10),
        vec![
            JobSpec::triggered(new_source.clone(), Duration::from_secs(1)),
            JobSpec::history(history.clone(), new_source.clone(), Duration::from_secs(1)),
        ],
        [new_source, history.clone()],
    );
    replacement.generation = ConfigGeneration(2);
    let changed = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: at_millis(1_001),
        config: replacement,
    });

    assert!(changed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::InvalidateJob { job, .. } if job == &history)
    }));
    assert!(!scheduler.has_sample(&history));
}

#[test]
fn duplicate_and_rollback_generations_are_rejected() {
    let memory = job(OwnerId::Memory, JobKind::Memory);
    let spec = JobSpec::periodic(memory.clone(), Duration::from_secs(1));
    let mut scheduler = Scheduler::new();
    let _ = startup(
        &mut scheduler,
        config(Duration::from_secs(1), vec![spec.clone()], [memory.clone()]),
    );
    for generation in [ConfigGeneration(1), ConfigGeneration(0)] {
        let mut duplicate = config(Duration::from_secs(2), vec![spec.clone()], [memory.clone()]);
        duplicate.generation = generation;
        let rejected = scheduler.handle(SchedulerEvent::ConfigChanged {
            at: at_millis(1),
            config: duplicate,
        });
        assert_eq!(rejected.disposition, EventDisposition::RejectedStale);
        assert!(rejected.actions.is_empty());
    }
    for generation in [InventoryGeneration(1), InventoryGeneration(0)] {
        let rejected = scheduler.handle(SchedulerEvent::InventoryChanged {
            at: at_millis(1),
            update: InventoryUpdate {
                generation,
                jobs: vec![spec.clone()],
                demand: plan([memory.clone()], [], []),
            },
        });
        assert_eq!(rejected.disposition, EventDisposition::RejectedStale);
        assert!(rejected.actions.is_empty());
    }
}

#[test]
fn startup_reconfiguration_rebuilds_all_first_paint_blockers() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let memory = job(OwnerId::Memory, JobKind::Memory);
    let mut cpu_spec = JobSpec::fast(cpu.clone(), Duration::from_secs(1));
    cpu_spec.startup_panel = true;
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(1),
            vec![cpu_spec.clone()],
            [cpu.clone()],
        ),
    );
    let obsolete = first_start(&initial);
    let mut memory_spec = JobSpec::fast(memory.clone(), Duration::from_secs(1));
    memory_spec.startup_panel = true;
    let mut replacement = config(
        Duration::from_secs(1),
        vec![cpu_spec, memory_spec],
        [cpu.clone(), memory.clone()],
    );
    replacement.generation = ConfigGeneration(2);
    let changed = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: at_millis(1),
        config: replacement,
    });
    assert!(publications(&changed).is_empty());
    let mut tickets = starts(&changed);
    let started = scheduler.handle(SchedulerEvent::JobCancelled {
        at: at_millis(1),
        ticket: obsolete,
    });
    tickets.extend(starts(&started));
    assert_eq!(tickets.len(), 2);
    let first = finish(
        &mut scheduler,
        at_millis(2),
        tickets[0].clone(),
        CompletionKind::Captured,
    );
    assert!(publications(&first).is_empty());
    let second = finish(
        &mut scheduler,
        at_millis(2),
        tickets[1].clone(),
        CompletionKind::Captured,
    );
    assert_eq!(publications(&second)[0].1, PublishReason::FirstPaintReady);
}

#[test]
fn replacement_invalidates_before_cancelling_old_work() {
    let old = sourced(OwnerId::Network, JobKind::NetworkRate, "eth0");
    let new = sourced(OwnerId::Network, JobKind::NetworkRate, "wlan0");
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(1),
            vec![JobSpec::periodic(old.clone(), Duration::from_secs(1))],
            [old.clone()],
        ),
    );
    let running = first_start(&initial);
    let changed = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: at_millis(1),
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: vec![JobSpec::periodic(new.clone(), Duration::from_secs(1))],
            demand: plan([new], [], []),
        },
    });
    let invalidation = changed
        .actions
        .iter()
        .position(
            |action| matches!(action, SchedulerAction::InvalidateJob { job, .. } if job == &old),
        )
        .expect("old source invalidation");
    let cancellation = changed
        .actions
        .iter()
        .position(|action| matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &running))
        .expect("old source cancellation");

    assert!(invalidation < cancellation);
}
