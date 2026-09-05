use super::*;

fn graph_scheduler_config() -> SchedulerConfig {
    let graph = JobId::singleton(OwnerId::Page, JobKind::PageRender);
    let cpu = JobId::singleton(OwnerId::Cpu, JobKind::Cpu);
    let memory = JobId::singleton(OwnerId::Memory, JobKind::Memory);
    let disk = JobId::singleton(OwnerId::Disk, JobKind::DiskUsage);
    SchedulerConfig {
        generation: ConfigGeneration(1),
        display_interval: Duration::from_secs(1),
        jobs: vec![
            JobSpec::fast(graph.clone(), Duration::from_secs(1)),
            JobSpec::fast(cpu.clone(), Duration::from_secs(1)),
            JobSpec::fast(memory.clone(), Duration::from_secs(1)),
            JobSpec::fast(disk.clone(), Duration::from_secs(1)),
        ],
        demand: DemandPlan {
            hidden: [cpu, memory, disk].into_iter().collect(),
            pages: [(PageId::Graphs, [graph].into_iter().collect())]
                .into_iter()
                .collect(),
            ..DemandPlan::default()
        },
    }
}

fn graph_fixture(
    label: &str,
) -> (
    PathBuf,
    DaemonPaths,
    RuntimeState,
    Scheduler,
    Vec<JobTicket>,
) {
    let (root, paths) = test_paths(label);
    fs::create_dir_all(&paths.state).expect("state root");
    write_atomic(&paths.page, "1").expect("graph page state");
    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs")];
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    let mut scheduler = Scheduler::new();
    let started = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: graph_scheduler_config(),
        inventory_generation: InventoryGeneration(1),
    });
    let mut tickets = started
        .actions
        .iter()
        .filter_map(|action| match action {
            SchedulerAction::StartJob { ticket } => Some(ticket.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    let _ = scheduler.handle(SchedulerEvent::SelectedPageChanged {
        at: SchedulerTime::ZERO,
        page: PageId::Graphs,
    });
    let activated = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let graph_ticket = started_ticket(&activated);
    state.mark_graph_render_requested(&graph_ticket);
    tickets.push(graph_ticket);
    (root, paths, state, scheduler, tickets)
}

fn completion_with_readings(
    ticket: JobTicket,
    readings: DisplaySnapshot,
    rendered_page: Option<&str>,
    render_generation: u64,
    decision: oneshot::Sender<bool>,
) -> OwnerCompletion {
    OwnerCompletion::Job(Box::new(worker::JobCompletion {
        ticket,
        completion: CompletionKind::Captured,
        readings,
        notifications: DisplaySnapshot::default(),
        notification_ready: false,
        hw: HardwareInventory::default(),
        resolved_mounts: vec![String::from("/")],
        command_cache: None,
        rendered_page: rendered_page.map(String::from),
        style_generation: 1,
        render_generation,
        decoder_outcome: None,
        gpu_history_point: None,
        decision: Some(decision),
    }))
}

fn take_ticket(tickets: &mut Vec<JobTicket>, kind: JobKind) -> JobTicket {
    let index = tickets
        .iter()
        .position(|ticket| ticket.job.kind == kind)
        .expect("job ticket");
    tickets.swap_remove(index)
}

#[test]
fn retained_activation_graph_stays_visible_until_fresh_render_publishes() {
    let (root, paths, mut state, mut scheduler, mut tickets) =
        graph_fixture("graph-activation-coalescing");
    let graph_ticket = take_ticket(&mut tickets, JobKind::PageRender);
    let retained = String::from("retained activation graph");
    state.rendered_graph = Some((PageId::Graphs, 1, retained.clone()));
    state.render_generation = 2;
    state.graph_render_requested = None;
    state.tooltip_html = Some(retained.clone());
    write_atomic(&paths.tooltip, &retained).expect("retained tooltip");
    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let validity = DispatchValidity::default();
    let mut actions = VecDeque::from([
        SchedulerAction::PublishDisplay {
            publication: crate::scheduler::PublicationId(99),
            reason: PublishReason::TooltipActivated,
            display_deadline: None,
            skipped_display_deadlines: 0,
            panel: false,
            tooltip: true,
        },
        SchedulerAction::StartJob {
            ticket: graph_ticket.clone(),
        },
    ]);
    let mut owner_messages = VecDeque::new();
    let (notification_sender, _notification_receiver) = mpsc::channel(1);

    drain_actions(
        &mut actions,
        &mut owner_messages,
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        &notification_sender,
        &validity,
    )
    .expect("drain activation");
    assert_eq!(state.tooltip_html.as_deref(), Some(retained.as_str()));
    assert_eq!(
        fs::read_to_string(&paths.tooltip).expect("tooltip frame"),
        retained
    );
    assert_eq!(
        owner_messages
            .iter()
            .filter(|(_, message)| {
                matches!(message, OwnerMessage::Run(input) if input.ticket.job.kind == JobKind::PageRender)
            })
            .count(),
        1
    );
    owner_messages.clear();

    let (decision, mut result) = oneshot::channel();
    handle_completion(
        completion_with_readings(
            graph_ticket,
            DisplaySnapshot::default(),
            Some("fresh activation graph"),
            2,
            decision,
        ),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        now(&clock),
        &mut actions,
        &mut owner_messages,
        &validity,
    );
    assert_eq!(result.try_recv(), Ok(true));
    assert!(!actions.iter().any(|action| {
        matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::PageRender)
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn render_generation_rollover_keeps_new_graph_input_requestable() {
    let (root, paths, mut state, mut scheduler, mut tickets) =
        graph_fixture("graph-generation-rollover");
    state.render_generation = u64::MAX;
    state.graph_render_requested = Some((RunId(99), u64::MAX));
    assert!(!state.selected_graph_needs_render());
    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let validity = DispatchValidity::default();
    let mut actions = VecDeque::new();
    let mut owner_messages = VecDeque::new();
    let (decision, mut result) = oneshot::channel();

    handle_completion(
        completion_with_readings(
            take_ticket(&mut tickets, JobKind::Cpu),
            DisplaySnapshot {
                cpu_usage: Some(31),
                ..DisplaySnapshot::default()
            },
            None,
            u64::MAX,
            decision,
        ),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        SchedulerTime::ZERO,
        &mut actions,
        &mut owner_messages,
        &validity,
    );

    assert_eq!(result.try_recv(), Ok(true));
    assert_eq!(state.render_generation, 0);
    assert!(state.selected_graph_needs_render());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn invalidated_cpu_graph_inputs_become_requestable_but_unrelated_input_does_not() {
    let (root, paths, mut state, mut scheduler, _) = graph_fixture("graph-invalidation");
    state.readings.cpu_usage = Some(42);
    state.readings.cpu_history = vec![30, 42];
    state.rendered_graph = Some((PageId::Graphs, 1, String::from("current graph")));
    state.graph_render_requested = None;
    assert!(!state.selected_graph_needs_render());

    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let validity = DispatchValidity::default();
    let mut owner_messages = VecDeque::new();
    let (notification_sender, _notification_receiver) = mpsc::channel(1);
    for (job, expected_generation) in [
        (JobId::singleton(OwnerId::Cpu, JobKind::Cpu), 2),
        (JobId::singleton(OwnerId::Cpu, JobKind::CpuHistory), 3),
    ] {
        let mut actions = VecDeque::from([SchedulerAction::InvalidateJob {
            metrics: None,
            job,
            reason: crate::scheduler::CancelReason::SourceReplaced,
        }]);
        drain_actions(
            &mut actions,
            &mut owner_messages,
            &mut scheduler,
            &mut state,
            &roots,
            &paths,
            &clock,
            ClockSnapshot::default(),
            &notification_sender,
            &validity,
        )
        .expect("invalidate CPU graph input");
        assert_eq!(state.render_generation, expected_generation);
        assert!(state.selected_graph_needs_render());
        state.rendered_graph = Some((
            PageId::Graphs,
            expected_generation,
            String::from("replacement graph"),
        ));
    }
    assert_eq!(state.readings.cpu_usage, None);
    assert!(state.readings.cpu_history.is_empty());
    assert!(!state.selected_graph_needs_render());

    state.readings.screen_brightness = Some(75);
    let mut actions = VecDeque::from([SchedulerAction::InvalidateJob {
        metrics: None,
        job: JobId::singleton(OwnerId::External, JobKind::Brightness),
        reason: crate::scheduler::CancelReason::SourceReplaced,
    }]);
    drain_actions(
        &mut actions,
        &mut owner_messages,
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        &notification_sender,
        &validity,
    )
    .expect("invalidate unrelated input");
    assert_eq!(state.readings.screen_brightness, None);
    assert_eq!(state.render_generation, 3);
    assert!(!state.selected_graph_needs_render());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn cancelled_dirty_graph_reservation_allows_newest_replacement_render() {
    let (root, paths, mut state, mut scheduler, mut tickets) =
        graph_fixture("graph-cancelled-reservation");
    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let validity = DispatchValidity::default();
    let mut actions = VecDeque::new();
    let mut owner_messages = VecDeque::new();

    let accepted_ticket = take_ticket(&mut tickets, JobKind::PageRender);
    let (decision, mut accepted) = oneshot::channel();
    handle_completion(
        completion_with_readings(
            accepted_ticket.clone(),
            DisplaySnapshot::default(),
            Some("accepted graph"),
            1,
            decision,
        ),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        SchedulerTime::ZERO,
        &mut actions,
        &mut owner_messages,
        &validity,
    );
    assert_eq!(accepted.try_recv(), Ok(true));
    state.clear_graph_render_requested(&accepted_ticket);
    assert!(
        !state.selected_graph_needs_render(),
        "clearing a reservation must not invalidate a current graph"
    );
    actions.clear();

    let newest_readings = DisplaySnapshot {
        cpu_usage: Some(73),
        cpu_history: vec![61, 73],
        ..DisplaySnapshot::default()
    };
    let (decision, mut dirty) = oneshot::channel();
    handle_completion(
        completion_with_readings(
            take_ticket(&mut tickets, JobKind::Cpu),
            newest_readings.clone(),
            None,
            1,
            decision,
        ),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        SchedulerTime::ZERO,
        &mut actions,
        &mut owner_messages,
        &validity,
    );
    assert_eq!(dirty.try_recv(), Ok(true));
    assert_eq!(state.render_generation, 2);
    actions.clear();

    enqueue(
        &mut actions,
        scheduler.handle(SchedulerEvent::RefreshTriggered {
            at: SchedulerTime::ZERO,
            job: JobId::singleton(OwnerId::Page, JobKind::PageRender),
            trigger: RefreshTrigger::Signal,
        }),
    );
    let (notification_sender, _notification_receiver) = mpsc::channel(1);
    drain_actions(
        &mut actions,
        &mut owner_messages,
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        &notification_sender,
        &validity,
    )
    .expect("dispatch dirty graph");
    let cancelled_ticket = owner_messages
        .iter()
        .find_map(|(_, message)| match message {
            OwnerMessage::Run(input) if input.ticket.job.kind == JobKind::PageRender => {
                Some(input.ticket.clone())
            }
            _ => None,
        })
        .expect("dirty graph ticket");
    assert_eq!(
        state.graph_render_requested,
        Some((cancelled_ticket.run_id, 2))
    );

    let config = graph_scheduler_config();
    enqueue(
        &mut actions,
        scheduler.handle(SchedulerEvent::InventoryChanged {
            at: SchedulerTime::ZERO,
            update: InventoryUpdate {
                generation: InventoryGeneration(2),
                jobs: config.jobs,
                demand: config.demand,
                resume_acknowledgement: None,
            },
        }),
    );
    drain_actions(
        &mut actions,
        &mut owner_messages,
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        &notification_sender,
        &validity,
    )
    .expect("cancel dirty graph");
    assert_eq!(state.graph_render_requested, None);
    assert!(state.selected_graph_needs_render());

    request_selected_graph(&mut scheduler, &state, &clock, &mut actions);
    drain_actions(
        &mut actions,
        &mut owner_messages,
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        &notification_sender,
        &validity,
    )
    .expect("dispatch replacement graph");
    let replacements = owner_messages
        .iter()
        .filter_map(|(_, message)| match message {
            OwnerMessage::Run(input) if input.ticket.job.kind == JobKind::PageRender => Some(input),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(replacements.len(), 1);
    assert_ne!(replacements[0].ticket.run_id, cancelled_ticket.run_id);
    assert_eq!(replacements[0].render_generation, 2);
    assert_eq!(
        replacements[0].readings.cpu_usage,
        newest_readings.cpu_usage
    );
    assert_eq!(
        replacements[0].readings.cpu_history,
        newest_readings.cpu_history
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_input_burst_rejects_stale_render_and_dispatches_one_newest_follow_up() {
    let (root, paths, mut state, mut scheduler, mut tickets) = graph_fixture("graph-input-burst");
    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let validity = DispatchValidity::default();
    let mut actions = VecDeque::new();
    let mut owner_messages = VecDeque::new();

    for (kind, readings) in [
        (
            JobKind::Cpu,
            DisplaySnapshot {
                cpu_usage: Some(31),
                cpu_history: vec![20, 31],
                ..DisplaySnapshot::default()
            },
        ),
        (
            JobKind::Memory,
            DisplaySnapshot {
                mem_usage: Some(47),
                mem_history: vec![40, 47],
                ..DisplaySnapshot::default()
            },
        ),
    ] {
        let (decision, mut result) = oneshot::channel();
        handle_completion(
            completion_with_readings(take_ticket(&mut tickets, kind), readings, None, 1, decision),
            &mut scheduler,
            &mut state,
            &roots,
            &paths,
            &clock,
            ClockSnapshot::default(),
            SchedulerTime::ZERO,
            &mut actions,
            &mut owner_messages,
            &validity,
        );
        assert_eq!(result.try_recv(), Ok(true));
        assert!(!actions.iter().any(|action| {
            matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::PageRender)
        }));
        actions.clear();
    }
    assert_eq!(state.render_generation, 3);

    let generation_before_unrelated = state.render_generation;
    let (decision, mut result) = oneshot::channel();
    handle_completion(
        completion_with_readings(
            take_ticket(&mut tickets, JobKind::DiskUsage),
            DisplaySnapshot::default(),
            None,
            1,
            decision,
        ),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        SchedulerTime::ZERO,
        &mut actions,
        &mut owner_messages,
        &validity,
    );
    assert_eq!(result.try_recv(), Ok(true));
    assert_eq!(state.render_generation, generation_before_unrelated);
    assert!(
        !actions.iter().any(|action| {
            matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::PageRender)
        }),
        "unrelated completion caused graph churn"
    );
    actions.clear();

    let (decision, mut stale_result) = oneshot::channel();
    handle_completion(
        completion_with_readings(
            take_ticket(&mut tickets, JobKind::PageRender),
            DisplaySnapshot::default(),
            Some("obsolete graph"),
            1,
            decision,
        ),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        SchedulerTime::ZERO,
        &mut actions,
        &mut owner_messages,
        &validity,
    );
    assert_eq!(stale_result.try_recv(), Ok(false));
    assert!(state.rendered_graph.is_none());

    let (notification_sender, _notification_receiver) = mpsc::channel(1);
    drain_actions(
        &mut actions,
        &mut owner_messages,
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        &notification_sender,
        &validity,
    )
    .expect("drain graph follow-up");
    let follow_ups = owner_messages
        .iter()
        .filter_map(|(_, message)| match message {
            OwnerMessage::Run(input) if input.ticket.job.kind == JobKind::PageRender => Some(input),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(follow_ups.len(), 1);
    assert_eq!(follow_ups[0].render_generation, 3);
    assert_eq!(follow_ups[0].readings.cpu_history, vec![20, 31]);
    assert_eq!(follow_ups[0].readings.mem_history, vec![40, 47]);
    let follow_up_ticket = follow_ups[0].ticket.clone();
    owner_messages.clear();
    let (decision, mut follow_up_result) = oneshot::channel();
    handle_completion(
        completion_with_readings(
            follow_up_ticket,
            DisplaySnapshot::default(),
            Some("newest graph"),
            3,
            decision,
        ),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        now(&clock),
        &mut actions,
        &mut owner_messages,
        &validity,
    );
    assert_eq!(follow_up_result.try_recv(), Ok(true));
    assert!(!actions.iter().any(|action| {
        matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::PageRender)
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_render_cancels_only_after_deactivation_grace() {
    let (root, _paths, _state, mut scheduler, tickets) = graph_fixture("graph-grace-cancel");
    let graph_ticket = tickets
        .into_iter()
        .find(|ticket| ticket.job.kind == JobKind::PageRender)
        .expect("graph ticket");

    let dismissed = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: false,
    });
    assert!(!dismissed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &graph_ticket)
    }));
    let before_grace = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_millis(999)),
    });
    assert!(!before_grace.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &graph_ticket)
    }));
    let expired = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_secs(1)),
    });
    assert!(expired.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &graph_ticket)
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_render_cancels_for_source_replacement_and_shutdown() {
    let (root, _paths, state, mut scheduler, tickets) = graph_fixture("graph-generation-cancel");
    let graph_ticket = tickets
        .into_iter()
        .find(|ticket| ticket.job.kind == JobKind::PageRender)
        .expect("graph ticket");
    let config = state.scheduler_config();

    let replaced = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::ZERO,
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: config.jobs,
            demand: config.demand,
            resume_acknowledgement: None,
        },
    });
    assert!(replaced.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &graph_ticket)
    }));

    let cancelled = scheduler.handle(SchedulerEvent::JobCancelled {
        at: SchedulerTime::ZERO,
        ticket: graph_ticket,
    });
    let replacement = cancelled
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::PageRender => {
                Some(ticket.clone())
            }
            _ => None,
        })
        .expect("replacement graph render");
    let shutdown = scheduler.handle(SchedulerEvent::Shutdown {
        at: SchedulerTime::ZERO,
    });
    assert!(shutdown.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &replacement)
    }));
    let _ = fs::remove_dir_all(root);
}
