use super::*;

struct DemandFixture {
    scheduler: Scheduler,
    initial: Transition,
    panel: JobId,
    graph_history: JobId,
    main: JobId,
    cores: JobId,
    command: JobId,
}

fn demand_fixture() -> DemandFixture {
    let panel = job(OwnerId::Memory, JobKind::Memory);
    let graph_source = job(OwnerId::Network, JobKind::NetworkRate);
    let graph_history = job(OwnerId::Network, JobKind::NetworkHistory);
    let main = job(OwnerId::External, JobKind::Brightness);
    let cores = job(OwnerId::Cpu, JobKind::CpuCores);
    let command = JobId::with_source(
        OwnerId::Page,
        JobKind::PageCommand,
        SourceIdentity::Page(PageId::Connections),
    );
    let specs = vec![
        JobSpec::periodic(panel.clone(), Duration::from_secs(1)),
        JobSpec::periodic(graph_source.clone(), Duration::from_secs(1)),
        JobSpec::history(
            graph_history.clone(),
            graph_source.clone(),
            Duration::from_secs(1),
        ),
        JobSpec::periodic(main.clone(), Duration::from_secs(1)),
        JobSpec::periodic(cores.clone(), Duration::from_secs(1)),
        JobSpec::periodic(command.clone(), Duration::from_secs(1)),
    ];
    let demand = plan(
        [panel.clone(), graph_source, graph_history.clone()],
        [main.clone()],
        [
            (PageId::CpuCores, BTreeSet::from([cores.clone()])),
            (PageId::Connections, BTreeSet::from([command.clone()])),
        ],
    );
    let mut scheduler = Scheduler::new();
    let mut cfg = config(Duration::from_secs(1), specs, []);
    cfg.demand = demand;
    let initial = startup(&mut scheduler, cfg);
    DemandFixture {
        scheduler,
        initial,
        panel,
        graph_history,
        main,
        cores,
        command,
    }
}

#[test]
fn hidden_demand_keeps_panel_and_configured_histories_but_not_tooltip_jobs() {
    let fixture = demand_fixture();
    let jobs = starts(&fixture.initial)
        .into_iter()
        .map(|ticket| ticket.job)
        .collect::<BTreeSet<_>>();
    assert!(jobs.contains(&fixture.panel));
    assert!(!jobs.contains(&fixture.main));
    assert!(!jobs.contains(&fixture.cores));
    assert!(!jobs.contains(&fixture.command));
    assert!(!jobs.contains(&fixture.graph_history));
}

#[test]
fn presented_main_and_selected_page_add_only_their_jobs() {
    let mut fixture = demand_fixture();
    let presented = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(1),
        presented: true,
    });
    assert!(
        starts(&presented)
            .iter()
            .any(|ticket| ticket.job == fixture.main)
    );
    assert!(
        starts(&presented)
            .iter()
            .all(|ticket| ticket.job != fixture.cores)
    );

    let selected = fixture
        .scheduler
        .handle(SchedulerEvent::SelectedPageChanged {
            at: at_millis(2),
            page: PageId::CpuCores,
        });
    assert!(
        starts(&selected)
            .iter()
            .any(|ticket| ticket.job == fixture.cores)
    );
    assert!(
        starts(&selected)
            .iter()
            .all(|ticket| ticket.job != fixture.command)
    );
}

#[test]
fn tooltip_activation_publishes_retained_frame_then_one_refresh() {
    let mut fixture = demand_fixture();
    let activation = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(1),
        presented: true,
    });
    assert!(
        publications(&activation)
            .iter()
            .any(|(_, reason, panel, tooltip)| {
                *reason == PublishReason::TooltipActivated && !panel && *tooltip
            })
    );
    let main_ticket = starts(&activation)
        .into_iter()
        .find(|ticket| ticket.job == fixture.main)
        .expect("main tooltip start");
    let refreshed = finish(
        &mut fixture.scheduler,
        at_millis(2),
        main_ticket,
        CompletionKind::Captured,
    );
    assert!(
        publications(&refreshed)
            .iter()
            .any(|(_, reason, _, tooltip)| {
                *reason == PublishReason::TooltipRefresh && *tooltip
            })
    );
    let later = fixture
        .scheduler
        .handle(SchedulerEvent::TimeAdvanced { at: at_millis(101) });
    assert!(
        publications(&later)
            .iter()
            .all(|(_, reason, _, _)| *reason != PublishReason::TooltipRefresh)
    );
}

#[test]
fn tooltip_activation_refreshes_at_one_hundred_millisecond_timeout() {
    let mut fixture = demand_fixture();
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(1),
        presented: true,
    });
    let timeout = fixture
        .scheduler
        .handle(SchedulerEvent::TimeAdvanced { at: at_millis(101) });

    assert!(
        publications(&timeout)
            .iter()
            .any(|(_, reason, panel, tooltip)| {
                *reason == PublishReason::TooltipRefresh && !panel && *tooltip
            })
    );
}

#[test]
fn tooltip_deactivation_keeps_page_demand_for_one_second_grace() {
    let mut fixture = demand_fixture();
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let selected = fixture
        .scheduler
        .handle(SchedulerEvent::SelectedPageChanged {
            at: at_millis(1),
            page: PageId::Connections,
        });
    if let Some(ticket) = starts(&selected)
        .into_iter()
        .find(|ticket| ticket.job == fixture.command)
    {
        let _ = finish(
            &mut fixture.scheduler,
            at_millis(1),
            ticket,
            CompletionKind::Captured,
        );
    }
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(2),
        presented: false,
    });
    let before = fixture.scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_millis(1_001),
        job: fixture.command.clone(),
        trigger: RefreshTrigger::Signal,
    });
    assert!(
        starts(&before)
            .iter()
            .any(|ticket| ticket.job == fixture.command)
    );
    let expiry = fixture.scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: at_millis(1_002),
    });
    assert!(expiry.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, reason: CancelReason::DemandEnded } if ticket.job == fixture.command)
    }));
}

#[test]
fn duplicate_hidden_observation_does_not_extend_grace() {
    let mut fixture = demand_fixture();
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: false,
    });
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(900),
        presented: false,
    });
    let expired = fixture
        .scheduler
        .handle(SchedulerEvent::TimeAdvanced { at: at_seconds(1) });
    assert!(starts(&expired).is_empty());
    let hidden = fixture.scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: at_seconds(1),
        job: fixture.command,
        trigger: RefreshTrigger::Signal,
    });
    assert!(starts(&hidden).is_empty());
}

#[test]
fn hidden_grace_keeps_demand_but_never_publishes_tooltip() {
    let mut fixture = demand_fixture();
    let activated = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let main_ticket = starts(&activated)
        .into_iter()
        .find(|ticket| ticket.job == fixture.main)
        .expect("main tooltip start");
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(1),
        presented: false,
    });
    let completed = finish(
        &mut fixture.scheduler,
        at_millis(2),
        main_ticket,
        CompletionKind::Captured,
    );
    let refresh = fixture
        .scheduler
        .handle(SchedulerEvent::TooltipRefreshRequested { at: at_millis(3) });
    let config = fixture
        .scheduler
        .handle(SchedulerEvent::DisplayRefreshRequested { at: at_millis(4) });
    let page = fixture
        .scheduler
        .handle(SchedulerEvent::SelectedPageChanged {
            at: at_millis(5),
            page: PageId::CpuCores,
        });
    let deadline = fixture
        .scheduler
        .handle(SchedulerEvent::TimeAdvanced { at: at_millis(100) });

    for transition in [&completed, &refresh, &config, &page, &deadline] {
        assert!(
            publications(transition)
                .iter()
                .all(|(_, _, _, tooltip)| !tooltip)
        );
    }
}

#[test]
fn representation_within_grace_immediately_republishes_retained_tooltip() {
    let mut fixture = demand_fixture();
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(1),
        presented: false,
    });

    let represented = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(500),
        presented: true,
    });

    assert!(
        publications(&represented)
            .iter()
            .any(|(_, reason, panel, tooltip)| {
                *reason == PublishReason::TooltipActivated && !panel && *tooltip
            })
    );
}

#[test]
fn signal_and_file_bursts_coalesce_while_job_runs() {
    let updates = job(OwnerId::External, JobKind::UpdatesFile);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(
            Duration::from_secs(10),
            vec![JobSpec::triggered(updates.clone(), Duration::from_secs(1))],
            [updates.clone()],
        ),
    );
    let running = first_start(&initial);
    for (index, trigger) in [
        RefreshTrigger::FileChanged,
        RefreshTrigger::FileChanged,
        RefreshTrigger::Signal,
    ]
    .into_iter()
    .enumerate()
    {
        let transition = scheduler.handle(SchedulerEvent::RefreshTriggered {
            at: at_millis(u64::try_from(index + 1).expect("small index")),
            job: updates.clone(),
            trigger,
        });
        assert!(starts(&transition).is_empty());
    }
    let follow_up = finish(
        &mut scheduler,
        at_millis(4),
        running,
        CompletionKind::Captured,
    );
    assert_eq!(starts(&follow_up).len(), 1);
    let done = finish(
        &mut scheduler,
        at_millis(5),
        first_start(&follow_up),
        CompletionKind::Captured,
    );
    assert!(starts(&done).is_empty());
}

#[test]
fn page_change_cancels_old_page_work_before_new_page_dispatch() {
    let mut fixture = demand_fixture();
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let connections = fixture
        .scheduler
        .handle(SchedulerEvent::SelectedPageChanged {
            at: at_millis(1),
            page: PageId::Connections,
        });
    let old = starts(&connections)
        .into_iter()
        .find(|ticket| ticket.job == fixture.command)
        .expect("connections command start");
    let cores = fixture
        .scheduler
        .handle(SchedulerEvent::SelectedPageChanged {
            at: at_millis(2),
            page: PageId::CpuCores,
        });
    assert!(cores.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, reason: CancelReason::DemandEnded } if ticket == &old)
    }));
    assert!(!fixture.scheduler.is_current_ticket(&old));
    assert!(
        starts(&cores)
            .iter()
            .any(|ticket| ticket.job == fixture.cores)
    );
}

#[test]
fn demand_change_cancels_ended_page_work_and_activates_added_work() {
    let mut fixture = demand_fixture();
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let selected = fixture
        .scheduler
        .handle(SchedulerEvent::SelectedPageChanged {
            at: at_millis(1),
            page: PageId::Connections,
        });
    let old = starts(&selected)
        .into_iter()
        .find(|ticket| ticket.job == fixture.command)
        .expect("connections command start");
    let changed = fixture.scheduler.handle(SchedulerEvent::DemandChanged {
        at: at_millis(2),
        demand: plan(
            [fixture.panel.clone(), fixture.graph_history.clone()],
            [fixture.cores.clone()],
            [],
        ),
    });

    assert!(changed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, reason: CancelReason::DemandEnded } if ticket == &old)
    }));
    assert!(
        starts(&changed)
            .iter()
            .any(|ticket| ticket.job == fixture.cores)
    );
}

#[test]
fn demand_change_rebuilds_first_paint_blockers_and_paints_when_empty() {
    let cpu = job(OwnerId::Cpu, JobKind::Cpu);
    let memory = job(OwnerId::Memory, JobKind::Memory);
    let mut cpu_spec = JobSpec::periodic(cpu.clone(), Duration::from_secs(1));
    cpu_spec.startup_panel = true;
    let mut memory_spec = JobSpec::periodic(memory.clone(), Duration::from_secs(1));
    memory_spec.startup_panel = true;
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        config(Duration::from_secs(1), vec![cpu_spec, memory_spec], [cpu]),
    );
    assert!(publications(&initial).is_empty());

    let changed = scheduler.handle(SchedulerEvent::DemandChanged {
        at: at_millis(1),
        demand: plan([memory.clone()], [], []),
    });
    assert!(publications(&changed).is_empty());
    let memory_ticket = starts(&changed)
        .into_iter()
        .find(|ticket| ticket.job == memory)
        .expect("new panel blocker start");
    let ready = finish(
        &mut scheduler,
        at_millis(2),
        memory_ticket,
        CompletionKind::Captured,
    );
    assert_eq!(publications(&ready)[0].1, PublishReason::FirstPaintReady);
}

#[test]
fn coincident_activation_refresh_and_display_deadline_publish_tooltip_once() {
    let panel = job(OwnerId::Memory, JobKind::Memory);
    let tooltip = job(OwnerId::External, JobKind::Brightness);
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        SchedulerConfig {
            generation: ConfigGeneration(1),
            display_interval: Duration::from_millis(100),
            jobs: vec![
                JobSpec::fast(panel.clone(), Duration::from_millis(100)),
                JobSpec::periodic(tooltip.clone(), Duration::from_secs(1)),
            ],
            demand: plan([panel], [tooltip], []),
        },
    );
    let panel_ticket = starts(&initial)
        .into_iter()
        .find(|ticket| ticket.job.kind == JobKind::Memory)
        .expect("panel start");
    let _ = finish(
        &mut scheduler,
        SchedulerTime::ZERO,
        panel_ticket,
        CompletionKind::Captured,
    );
    let _ = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });

    let coincident = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(100) });
    let tooltip_publications = publications(&coincident)
        .into_iter()
        .filter(|(_, _, _, tooltip)| *tooltip)
        .collect::<Vec<_>>();
    assert_eq!(tooltip_publications.len(), 1);
    assert_eq!(tooltip_publications[0].1, PublishReason::DisplayDeadline);
    assert!(tooltip_publications[0].2);
    assert!(tooltip_publications[0].3);
}

#[test]
fn same_time_completion_config_and_page_events_each_publish_tooltip_updates() {
    let main = job(OwnerId::External, JobKind::Brightness);
    let page = job(OwnerId::Cpu, JobKind::CpuCores);
    let specs = vec![
        JobSpec::periodic(main.clone(), Duration::from_secs(1)),
        JobSpec::periodic(page.clone(), Duration::from_secs(1)),
    ];
    let demand = plan(
        [],
        [main.clone()],
        [(PageId::CpuCores, BTreeSet::from([page]))],
    );
    let mut scheduler = Scheduler::new();
    let mut initial_config = config(Duration::from_secs(1), specs.clone(), []);
    initial_config.demand = demand.clone();
    let _ = startup(&mut scheduler, initial_config);
    let activated = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });

    let completed = finish(
        &mut scheduler,
        at_millis(1),
        first_start(&activated),
        CompletionKind::Captured,
    );
    assert_eq!(publications(&completed)[0].1, PublishReason::TooltipRefresh);
    assert!(publications(&completed)[0].3);

    let configured = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: at_millis(1),
        config: SchedulerConfig {
            generation: ConfigGeneration(2),
            display_interval: Duration::from_secs(1),
            jobs: specs,
            demand,
        },
    });
    assert!(
        publications(&configured)
            .iter()
            .any(
                |(_, reason, panel, tooltip)| *reason == PublishReason::ConfigChanged
                    && *panel
                    && *tooltip
            )
    );

    let paged = scheduler.handle(SchedulerEvent::SelectedPageChanged {
        at: at_millis(1),
        page: PageId::CpuCores,
    });
    assert!(
        publications(&paged)
            .iter()
            .any(
                |(_, reason, panel, tooltip)| *reason == PublishReason::PageChanged
                    && !panel
                    && *tooltip
            )
    );
}

#[test]
fn page_activation_unions_waiters_and_keeps_earliest_timeout() {
    let main = job(OwnerId::External, JobKind::Brightness);
    let page = job(OwnerId::Cpu, JobKind::CpuCores);
    let mut scheduler = Scheduler::new();
    let mut cfg = config(
        Duration::from_secs(1),
        vec![
            JobSpec::periodic(main.clone(), Duration::from_secs(1)),
            JobSpec::periodic(page.clone(), Duration::from_secs(1)),
        ],
        [],
    );
    cfg.demand = plan(
        [],
        [main],
        [(PageId::CpuCores, BTreeSet::from([page.clone()]))],
    );
    let _ = startup(&mut scheduler, cfg);
    let _ = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let changed = scheduler.handle(SchedulerEvent::SelectedPageChanged {
        at: at_millis(50),
        page: PageId::CpuCores,
    });
    let page_ticket = starts(&changed)
        .into_iter()
        .find(|ticket| ticket.job == page)
        .expect("page start");
    let page_finished = finish(
        &mut scheduler,
        at_millis(60),
        page_ticket,
        CompletionKind::Captured,
    );
    assert!(
        publications(&page_finished)
            .iter()
            .all(|(_, reason, _, _)| *reason != PublishReason::TooltipRefresh)
    );

    let timeout = scheduler.handle(SchedulerEvent::TimeAdvanced { at: at_millis(100) });
    assert!(
        publications(&timeout)
            .iter()
            .any(
                |(_, reason, panel, tooltip)| *reason == PublishReason::TooltipRefresh
                    && !panel
                    && *tooltip
            )
    );
}

#[test]
fn demand_activation_retains_demanded_waiters_and_drops_removed_ones() {
    let removed = job(OwnerId::External, JobKind::Brightness);
    let retained = job(OwnerId::Network, JobKind::NetworkIdentity);
    let added = job(OwnerId::Cpu, JobKind::CpuCores);
    let specs = vec![
        JobSpec::periodic(removed.clone(), Duration::from_secs(1)),
        JobSpec::periodic(retained.clone(), Duration::from_secs(1)),
        JobSpec::periodic(added.clone(), Duration::from_secs(1)),
    ];
    let mut scheduler = Scheduler::new();
    let mut cfg = config(Duration::from_secs(1), specs, []);
    cfg.demand = plan(
        [],
        [removed.clone()],
        [(PageId::Main, BTreeSet::from([retained.clone()]))],
    );
    let _ = startup(&mut scheduler, cfg);
    let activated = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let retained_ticket = starts(&activated)
        .into_iter()
        .find(|ticket| ticket.job == retained)
        .expect("retained start");

    let demand_changed = scheduler.handle(SchedulerEvent::DemandChanged {
        at: at_millis(25),
        demand: plan(
            [],
            [added.clone()],
            [(PageId::Main, BTreeSet::from([retained]))],
        ),
    });
    let added_ticket = starts(&demand_changed)
        .into_iter()
        .find(|ticket| ticket.job == added)
        .expect("added start");
    let added_finished = finish(
        &mut scheduler,
        at_millis(30),
        added_ticket,
        CompletionKind::Captured,
    );
    assert!(
        publications(&added_finished)
            .iter()
            .all(|(_, reason, _, _)| *reason != PublishReason::TooltipRefresh)
    );

    let retained_finished = finish(
        &mut scheduler,
        at_millis(40),
        retained_ticket,
        CompletionKind::Captured,
    );
    assert!(
        publications(&retained_finished)
            .iter()
            .any(
                |(_, reason, panel, tooltip)| *reason == PublishReason::TooltipRefresh
                    && !panel
                    && *tooltip
            )
    );
    assert!(!scheduler.has_sample(&removed));
}

#[test]
fn grace_cancellation_blocks_represented_page_until_acknowledged() {
    let mut fixture = demand_fixture();
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let selected = fixture
        .scheduler
        .handle(SchedulerEvent::SelectedPageChanged {
            at: at_millis(1),
            page: PageId::Connections,
        });
    let cancelled = starts(&selected)
        .into_iter()
        .find(|ticket| ticket.job == fixture.command)
        .expect("page command start");
    let _ = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(2),
        presented: false,
    });
    let expired = fixture.scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: at_millis(1_002),
    });
    assert!(expired.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, reason: CancelReason::DemandEnded } if ticket == &cancelled)
    }));

    let represented = fixture.scheduler.handle(SchedulerEvent::TooltipPresented {
        at: at_millis(1_003),
        presented: true,
    });
    assert!(
        starts(&represented)
            .iter()
            .all(|ticket| ticket.job != fixture.command)
    );
    let acknowledged = fixture.scheduler.handle(SchedulerEvent::JobCancelled {
        at: at_millis(1_004),
        ticket: cancelled,
    });
    assert!(
        starts(&acknowledged)
            .iter()
            .any(|ticket| ticket.job == fixture.command)
    );
}
