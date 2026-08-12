use super::*;

#[test]
fn due_jobs_use_stable_identity_order_and_one_start_per_owner() {
    let disk_a = sourced(OwnerId::Disk, JobKind::DiskTemperature, "a");
    let disk_b = sourced(OwnerId::Disk, JobKind::DiskTemperature, "b");
    let memory = job(OwnerId::Memory, JobKind::Memory);
    let specs = vec![
        JobSpec::periodic(disk_b.clone(), Duration::from_secs(30)),
        JobSpec::periodic(memory.clone(), Duration::from_secs(30)),
        JobSpec::periodic(disk_a.clone(), Duration::from_secs(30)),
    ];
    let mut scheduler = Scheduler::new();

    let transition = startup(
        &mut scheduler,
        config(
            Duration::from_secs(1),
            specs,
            [disk_a.clone(), disk_b.clone(), memory.clone()],
        ),
    );
    let started = starts(&transition);

    assert_eq!(started.len(), 2);
    assert_eq!(started[0].job, memory);
    assert_eq!(started[1].job, disk_a);
    let transition = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        started[1].clone(),
        CompletionKind::Captured,
    );
    assert_eq!(starts(&transition)[0].job, disk_b);
}

#[test]
fn fast_jobs_prefetch_fifty_milliseconds_before_display_deadline() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let mut spec = JobSpec::fast(cpu.clone(), Duration::from_secs(1));
    spec.startup_panel = true;
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(Duration::from_secs(1), vec![spec], [cpu.clone()]),
    );
    let ticket = first_start(&initial);
    let _ = finish(
        &mut scheduler,
        at_millis(1),
        ticket,
        CompletionKind::Captured,
    );

    let early = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(949) });
    assert!(starts(&early).is_empty());
    let due = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(950) });
    assert_eq!(first_start(&due).job, cpu);
}

#[test]
fn publication_deadline_does_not_wait_for_prefetched_job() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let mut spec = JobSpec::fast(cpu.clone(), Duration::from_secs(1));
    spec.startup_panel = true;
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(Duration::from_secs(1), vec![spec], [cpu]),
    );
    let initial_ticket = first_start(&initial);
    let first_paint = finish(
        &mut scheduler,
        at_millis(1),
        initial_ticket,
        CompletionKind::Captured,
    );
    let publication = publications(&first_paint)[0].0;
    let _ = scheduler.handle(SchedulerEvent::PanelPublished {
        at: at_millis(1),
        publication,
    });
    let prefetch = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(950) });
    assert_eq!(starts(&prefetch).len(), 1);

    let deadline = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_seconds(1) });
    assert!(
        publications(&deadline)
            .iter()
            .any(|(_, reason, panel, _)| { *reason == PublishReason::DisplayDeadline && *panel })
    );
}

#[test]
fn display_publication_carries_nominal_deadline_and_real_skips() {
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(Duration::from_secs(1), Vec::new(), std::iter::empty()),
    );
    let publication = publications(&initial)[0].0;
    let _ = scheduler.handle(SchedulerEvent::PanelPublished {
        at: at_millis(200),
        publication,
    });

    let late = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: at_millis(3_200),
    });
    let timing = late.actions.iter().find_map(|action| match action {
        SchedulerAction::PublishDisplay {
            reason: PublishReason::DisplayDeadline,
            display_deadline,
            skipped_display_deadlines,
            ..
        } => Some((*display_deadline, *skipped_display_deadlines)),
        _ => None,
    });

    assert_eq!(timing, Some((Some(at_seconds(1)), 2)));
}

#[test]
fn display_deadlines_suppressed_before_first_paint_are_skipped() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let mut spec = JobSpec::fast(cpu.clone(), Duration::from_millis(100));
    spec.startup_panel = true;
    let mut scheduler = Scheduler::new();
    let _ = startup(
        &mut scheduler,
        config(Duration::from_millis(100), vec![spec], [cpu]),
    );

    let timeout = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(200) });
    let skipped = timeout.actions.iter().find_map(|action| match action {
        SchedulerAction::PublishDisplay {
            reason: PublishReason::FirstPaintTimeout,
            skipped_display_deadlines,
            ..
        } => Some(*skipped_display_deadlines),
        _ => None,
    });

    assert_eq!(skipped, Some(2));
}

#[test]
fn missed_periodic_ticks_skip_catch_up_and_keep_phase() {
    let network = job(OwnerId::Network, JobKind::NetworkIdentity);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            vec![JobSpec::periodic(network.clone(), Duration::from_secs(1))],
            [network.clone()],
        ),
    );
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        first_start(&initial),
        CompletionKind::Captured,
    );

    let late = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: at_millis(3_500),
    });
    let ticket = first_start(&late);
    let completed = finish(
        &mut scheduler,
        at_millis(3_501),
        ticket,
        CompletionKind::Captured,
    );
    assert!(
        starts(&completed).is_empty(),
        "must not replay missed ticks"
    );
    assert_eq!(scheduler.next_wake(), Some(at_seconds(4)));
}

#[test]
fn failure_backoff_doubles_and_caps_at_freshness_budget() {
    let updates = job(OwnerId::External, JobKind::UpdatesFile);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            vec![JobSpec::triggered(
                updates.clone(),
                Duration::from_millis(500),
            )],
            [updates],
        ),
    );
    let mut ticket = first_start(&initial);
    let mut now = 0;
    for (failure, expected_delay) in [(1, 100), (2, 200), (3, 400), (4, 500)] {
        let transition = finish(
            &mut scheduler,
            at_millis(now),
            ticket,
            CompletionKind::Failed,
        );
        let retry_at = transition
            .actions
            .iter()
            .find_map(|action| match action {
                SchedulerAction::ApplyBackoff {
                    failures, retry_at, ..
                } if *failures == failure => Some(*retry_at),
                _ => None,
            })
            .expect("backoff action");
        assert_eq!(retry_at, at_millis(now + expected_delay));
        now += expected_delay;
        let due = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(now) });
        ticket = first_start(&due);
    }
}

#[test]
fn pending_due_and_trigger_cannot_bypass_failure_backoff() {
    let updates = job(OwnerId::External, JobKind::UpdatesFile);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            vec![JobSpec::periodic(updates.clone(), Duration::from_secs(1))],
            [updates.clone()],
        ),
    );
    let running = first_start(&initial);
    let _ = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_seconds(1) });
    let failed = finish(
        &mut scheduler,
        at_millis(1_001),
        running,
        CompletionKind::Failed,
    );
    assert!(starts(&failed).is_empty());
    let triggered = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(1_050),
        job: updates.clone(),
        trigger: RefreshTrigger::Signal,
    });
    assert!(starts(&triggered).is_empty());
    assert!(
        starts(&scheduler.handle(SchedulerEvent::TimeAdvanced {
            at: at_millis(1_100)
        }))
        .is_empty()
    );
    assert_eq!(
        first_start(&scheduler.handle(SchedulerEvent::TimeAdvanced {
            at: at_millis(1_101),
        }))
        .job,
        updates
    );
}

#[test]
fn config_change_reanchors_fast_prefetch_to_new_display_phase() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let spec = JobSpec::fast(cpu.clone(), Duration::from_secs(1));
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(Duration::from_secs(1), vec![spec.clone()], [cpu.clone()]),
    );
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        first_start(&initial),
        CompletionKind::Captured,
    );
    let mut replacement = config(Duration::from_secs(1), vec![spec], [cpu.clone()]);
    replacement.generation = ConfigGeneration(2);
    let _ = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: at_millis(100),
        config: replacement,
    });
    assert!(
        starts(&scheduler.handle(SchedulerEvent::TimeAdvanced {
            at: at_millis(1_049)
        }))
        .is_empty()
    );
    assert_eq!(
        first_start(&scheduler.handle(SchedulerEvent::TimeAdvanced {
            at: at_millis(1_050),
        }))
        .job,
        cpu
    );
}

#[test]
fn initial_counter_baseline_retries_after_one_hundred_milliseconds() {
    let rate = job(OwnerId::Network, JobKind::NetworkRate);
    let mut spec = JobSpec::periodic(rate.clone(), Duration::from_secs(10));
    spec.counter = true;
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(Duration::from_secs(20), vec![spec], [rate.clone()]),
    );
    let baseline = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        first_start(&initial),
        CompletionKind::Baseline,
    );
    assert!(starts(&baseline).is_empty());
    assert!(
        starts(&scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(99) })).is_empty()
    );
    assert_eq!(
        first_start(&scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(100) })).job,
        rate
    );
}

#[test]
fn pending_trigger_waits_for_prompt_baseline_retry_deadline() {
    let rate = job(OwnerId::Network, JobKind::NetworkRate);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            vec![JobSpec::periodic(rate.clone(), Duration::from_secs(10))],
            [rate.clone()],
        ),
    );
    let ticket = first_start(&initial);
    let _ = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(1),
        job: rate.clone(),
        trigger: RefreshTrigger::Signal,
    });
    let baseline = finish(
        &mut scheduler,
        at_millis(2),
        ticket,
        CompletionKind::Baseline,
    );
    assert!(starts(&baseline).is_empty());
    assert!(
        starts(&scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(101) })).is_empty()
    );
    assert_eq!(
        first_start(&scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(102) })).job,
        rate
    );
}

#[test]
fn first_paint_uses_completion_or_two_hundred_millisecond_timeout() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let mut spec = JobSpec::fast(cpu.clone(), Duration::from_secs(1));
    spec.startup_panel = true;
    let mut ready_scheduler = Scheduler::new();
    let initial = startup(
        &mut ready_scheduler,
        config(Duration::from_secs(1), vec![spec.clone()], [cpu.clone()]),
    );
    let ready = finish(
        &mut ready_scheduler,
        at_millis(50),
        first_start(&initial),
        CompletionKind::Captured,
    );
    assert_eq!(publications(&ready)[0].1, PublishReason::FirstPaintReady);

    let mut timeout_scheduler = Scheduler::new();
    let _ = startup(
        &mut timeout_scheduler,
        config(Duration::from_secs(1), vec![spec], [cpu]),
    );
    let timeout = timeout_scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(200) });
    assert_eq!(
        publications(&timeout)[0].1,
        PublishReason::FirstPaintTimeout
    );
}

#[test]
fn delayed_first_paint_consumes_elapsed_display_deadlines() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let mut spec = JobSpec::fast(cpu.clone(), Duration::from_millis(100));
    spec.startup_panel = true;
    let mut ready = Scheduler::new();
    let initial = startup(
        &mut ready,
        config(
            Duration::from_millis(100),
            vec![spec.clone()],
            [cpu.clone()],
        ),
    );
    let painted = finish(
        &mut ready,
        at_millis(150),
        first_start(&initial),
        CompletionKind::Captured,
    );
    assert_eq!(publications(&painted).len(), 1);
    assert_eq!(publications(&painted)[0].1, PublishReason::FirstPaintReady);

    let mut timeout = Scheduler::new();
    let _ = startup(
        &mut timeout,
        config(Duration::from_millis(100), vec![spec], [cpu]),
    );
    let painted = timeout.handle(SchedulerEvent::TimeAdvanced { at: at_millis(200) });
    assert_eq!(publications(&painted).len(), 1);
    assert_eq!(
        publications(&painted)[0].1,
        PublishReason::FirstPaintTimeout
    );
}
