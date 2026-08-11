use super::*;

use crate::scheduler::PublicationId;

fn tooltip_refresh_count(transition: &Transition) -> usize {
    transition
        .actions
        .iter()
        .filter(|action| {
            matches!(
                action,
                SchedulerAction::PublishDisplay {
                    reason: PublishReason::TooltipRefresh,
                    ..
                }
            )
        })
        .count()
}

fn job_completion(
    ticket: JobTicket,
    rendered_page: Option<String>,
    decision: Option<oneshot::Sender<bool>>,
) -> OwnerCompletion {
    OwnerCompletion::Job(Box::new(worker::JobCompletion {
        ticket,
        completion: CompletionKind::Captured,
        readings: DisplaySnapshot::default(),
        notifications: DisplaySnapshot::default(),
        notification_ready: false,
        hw: HardwareInventory::default(),
        resolved_mounts: vec![String::from("/")],
        command_cache: None,
        rendered_page,
        style_generation: 1,
        render_generation: 1,
        decoder_outcome: None,
        gpu_history_point: None,
        decision,
    }))
}

fn graph_completion_actions(
    label: &str,
    activation_timed_out: bool,
) -> (PathBuf, RuntimeState, VecDeque<SchedulerAction>) {
    let (root, paths) = test_paths(label);
    fs::create_dir_all(&paths.state).expect("state root");
    write_atomic(&paths.page, "1").expect("graph page state");
    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs")];
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(None, &paths, cfg, HardwareInventory::default(), active);
    let graph = JobId::singleton(OwnerId::Page, JobKind::PageRender);
    let demand = DemandPlan {
        pages: [(PageId::Graphs, [graph.clone()].into_iter().collect())]
            .into_iter()
            .collect(),
        ..DemandPlan::default()
    };
    let mut scheduler = Scheduler::new();
    let _ = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: SchedulerConfig {
            generation: ConfigGeneration(1),
            display_interval: Duration::from_secs(1),
            jobs: vec![JobSpec::fast(graph, Duration::from_secs(1))],
            demand,
        },
        inventory_generation: InventoryGeneration(1),
    });
    let _ = scheduler.handle(SchedulerEvent::SelectedPageChanged {
        at: SchedulerTime::ZERO,
        page: PageId::Graphs,
    });
    let activated = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let ticket = started_ticket(&activated);
    let completion_at = if activation_timed_out {
        let timeout_at = SchedulerTime::from_duration(Duration::from_millis(100));
        let timeout = scheduler.handle(SchedulerEvent::TimeAdvanced { at: timeout_at });
        assert_eq!(tooltip_refresh_count(&timeout), 1);
        SchedulerTime::from_duration(Duration::from_millis(101))
    } else {
        SchedulerTime::ZERO
    };
    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let mut actions = VecDeque::new();
    handle_completion(
        job_completion(ticket, Some(String::from("committed graph")), None),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        completion_at,
        &mut actions,
        &mut VecDeque::new(),
        &DispatchValidity::default(),
    );
    (root, state, actions)
}

fn assert_one_tooltip_only_refresh(actions: &VecDeque<SchedulerAction>) {
    let publications = actions
        .iter()
        .filter_map(|action| match action {
            SchedulerAction::PublishDisplay {
                reason,
                panel,
                tooltip,
                ..
            } => Some((*reason, *panel, *tooltip)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        publications,
        vec![(PublishReason::TooltipRefresh, false, true)]
    );
}

fn scheduler_with_introduced_tooltip_job() -> (Scheduler, JobTicket) {
    let discovery = JobId::singleton(OwnerId::Discovery, JobKind::HardwareDiscovery);
    let introduced = JobId::singleton(OwnerId::Power, JobKind::PeripheralBattery);
    let discovery_spec = JobSpec::periodic(discovery.clone(), Duration::from_secs(1));
    let introduced_spec = JobSpec::periodic(introduced.clone(), Duration::from_secs(1));
    let demand = |jobs: Vec<JobId>| DemandPlan {
        main_tooltip: jobs.into_iter().collect(),
        ..DemandPlan::default()
    };
    let mut scheduler = Scheduler::new();
    let _ = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: SchedulerConfig {
            generation: ConfigGeneration(1),
            display_interval: Duration::from_secs(1),
            jobs: vec![discovery_spec.clone()],
            demand: demand(vec![discovery.clone()]),
        },
        inventory_generation: InventoryGeneration(1),
    });
    let activated = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::from_duration(Duration::from_millis(1)),
        presented: true,
    });
    let discovery_ticket = activated
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } if ticket.job == discovery => Some(ticket.clone()),
            _ => None,
        })
        .expect("discovery start");
    let mut discovery_finished = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::from_duration(Duration::from_millis(10)),
        ticket: discovery_ticket,
        completion: CompletionKind::Captured,
        notification_ready: false,
    });
    assert_eq!(tooltip_refresh_count(&discovery_finished), 1);
    suppress_tooltip_refresh(&mut discovery_finished);
    assert_eq!(tooltip_refresh_count(&discovery_finished), 0);

    let inventory = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::from_duration(Duration::from_millis(10)),
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: vec![discovery_spec, introduced_spec],
            demand: demand(vec![discovery, introduced.clone()]),
            resume_acknowledgement: None,
        },
    });
    assert_eq!(tooltip_refresh_count(&inventory), 0);
    let introduced_ticket = inventory
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } if ticket.job == introduced => {
                Some(ticket.clone())
            }
            _ => None,
        })
        .expect("introduced tooltip job start");
    (scheduler, introduced_ticket)
}

#[test]
fn introduced_tooltip_jobs_emit_one_refresh_when_all_complete() {
    let (mut scheduler, introduced_ticket) = scheduler_with_introduced_tooltip_job();

    let completed = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::from_duration(Duration::from_millis(20)),
        ticket: introduced_ticket,
        completion: CompletionKind::Captured,
        notification_ready: false,
    });
    assert_eq!(tooltip_refresh_count(&completed), 1);
    let after_timeout = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_millis(110)),
    });
    assert_eq!(tooltip_refresh_count(&after_timeout), 0);
}

#[test]
fn introduced_tooltip_jobs_emit_one_refresh_at_timeout() {
    let (mut scheduler, introduced_ticket) = scheduler_with_introduced_tooltip_job();

    let timeout = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_millis(110)),
    });
    assert_eq!(tooltip_refresh_count(&timeout), 1);
    let completed = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::from_duration(Duration::from_millis(120)),
        ticket: introduced_ticket,
        completion: CompletionKind::Captured,
        notification_ready: false,
    });
    assert_eq!(tooltip_refresh_count(&completed), 0);
}

#[test]
fn graph_completion_reuses_queued_tooltip_refresh_without_panel_publication() {
    let (root, state, actions) = graph_completion_actions("graph-existing-refresh", false);

    assert_eq!(
        state
            .rendered_graph
            .as_ref()
            .map(|(_, _, html)| html.as_str()),
        Some("committed graph")
    );
    assert_one_tooltip_only_refresh(&actions);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_completion_requests_one_tooltip_only_refresh_when_none_is_queued() {
    let (root, state, actions) = graph_completion_actions("graph-new-refresh", true);

    assert_eq!(
        state
            .rendered_graph
            .as_ref()
            .map(|(_, _, html)| html.as_str()),
        Some("committed graph")
    );
    assert_one_tooltip_only_refresh(&actions);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn completion_after_deadline_publication_uses_non_regressing_time_and_releases_owner() {
    let (root, paths) = test_paths("completion-after-publication");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(None, &paths, cfg, HardwareInventory::default(), active);
    let mut scheduler = Scheduler::new();
    let initial = startup(&mut scheduler, vec![job(OwnerId::Cpu, JobKind::Cpu, true)]);
    let ticket = started_ticket(&initial);
    let received_at = SchedulerTime::from_duration(Duration::from_millis(200));
    let deadline = scheduler.handle(SchedulerEvent::TimeAdvanced { at: received_at });
    assert_eq!(deadline.disposition, EventDisposition::Accepted);
    let mut queued_actions = VecDeque::from(deadline.actions);
    let published_at = SchedulerTime::from_duration(Duration::from_millis(201));
    let mut publication_acknowledged = false;
    while let Some(action) = queued_actions.pop_front() {
        if let SchedulerAction::PublishDisplay {
            publication,
            reason: PublishReason::FirstPaintTimeout,
            panel: true,
            ..
        } = action
        {
            let acknowledged = scheduler.handle(SchedulerEvent::PanelPublished {
                at: published_at,
                publication,
            });
            assert_eq!(acknowledged.disposition, EventDisposition::Accepted);
            publication_acknowledged = true;
        }
    }
    assert!(
        publication_acknowledged,
        "deadline panel was not acknowledged"
    );

    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let validity = DispatchValidity::default();
    let mut actions = VecDeque::new();
    let mut owner_messages = VecDeque::new();
    let (stale_decision, mut stale_result) = oneshot::channel();
    handle_completion(
        job_completion(ticket.clone(), None, Some(stale_decision)),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        received_at,
        &mut actions,
        &mut owner_messages,
        &validity,
    );
    assert_eq!(stale_result.try_recv(), Ok(false));
    assert!(!state.last_committed.contains_key(&OwnerId::Cpu));
    assert!(actions.is_empty());

    let completion_at = completion_time_after_drain(
        received_at,
        SchedulerTime::from_duration(Duration::from_millis(202)),
    );
    assert!(completion_at >= published_at);
    let (decision, mut result) = oneshot::channel();
    handle_completion(
        job_completion(ticket.clone(), None, Some(decision)),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &clock,
        ClockSnapshot::default(),
        completion_at,
        &mut actions,
        &mut owner_messages,
        &validity,
    );

    assert_eq!(result.try_recv(), Ok(true));
    assert_eq!(
        state.last_committed.get(&OwnerId::Cpu),
        Some(&ticket.run_id)
    );
    let future = actions.iter().find_map(|action| match action {
        SchedulerAction::StartJob { ticket: future } if future.job == ticket.job => Some(future),
        _ => None,
    });
    assert!(future.is_some_and(|future| scheduler.is_current_ticket(future)));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deferred_first_paint_suppresses_inventory_display_refresh() {
    let (root, paths) = test_paths("inventory-first-paint");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(None, &paths, cfg, HardwareInventory::default(), active);
    state.deferred_first_paint = Some(SchedulerAction::PublishDisplay {
        publication: PublicationId(99),
        reason: PublishReason::FirstPaintReady,
        panel: true,
        tooltip: true,
    });
    let clock = ProductionClock::default();
    let boot = clock.snapshot();
    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, Vec::new());
    let mut actions = VecDeque::from([SchedulerAction::PublishDisplay {
        publication: PublicationId(98),
        reason: PublishReason::TooltipRefresh,
        panel: false,
        tooltip: true,
    }]);

    reconcile_inventory_publication(
        true,
        &mut state,
        &mut scheduler,
        &clock,
        boot,
        SchedulerTime::ZERO,
        &mut actions,
    );

    assert_eq!(actions.len(), 1);
    assert!(matches!(
        actions.front(),
        Some(SchedulerAction::PublishDisplay {
            reason: PublishReason::FirstPaintReady,
            panel: true,
            tooltip: true,
            ..
        })
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inventory_refresh_replaces_queued_tooltip_only_with_one_full_publication() {
    let (root, paths) = test_paths("inventory-tooltip-coalescing");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(None, &paths, cfg, HardwareInventory::default(), active);
    let clock = ProductionClock::default();
    let boot = clock.snapshot();
    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, Vec::new());
    let _ = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let tooltip_refresh = scheduler.handle(SchedulerEvent::TooltipRefreshRequested {
        at: SchedulerTime::ZERO,
    });
    let removed_publication = tooltip_refresh
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::PublishDisplay {
                publication,
                panel: false,
                tooltip: true,
                ..
            } => Some(*publication),
            _ => None,
        })
        .expect("tooltip-only publication");
    let mut actions = VecDeque::from(tooltip_refresh.actions);

    reconcile_inventory_publication(
        true,
        &mut state,
        &mut scheduler,
        &clock,
        boot,
        SchedulerTime::ZERO,
        &mut actions,
    );

    let publications = actions
        .iter()
        .filter_map(|action| match action {
            SchedulerAction::PublishDisplay {
                publication,
                reason,
                panel,
                tooltip,
            } => Some((*publication, *reason, *panel, *tooltip)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(publications.len(), 1);
    let (full_publication, reason, panel, tooltip) = publications[0];
    assert_eq!(reason, PublishReason::ConfigChanged);
    assert!(panel && tooltip);
    assert_ne!(full_publication, removed_publication);
    assert_eq!(
        scheduler
            .handle(SchedulerEvent::PanelPublished {
                at: SchedulerTime::ZERO,
                publication: removed_publication,
            })
            .disposition,
        EventDisposition::RejectedStale
    );
    assert_eq!(
        scheduler
            .handle(SchedulerEvent::PanelPublished {
                at: SchedulerTime::ZERO,
                publication: full_publication,
            })
            .disposition,
        EventDisposition::Accepted
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inventory_refresh_preserves_one_existing_panel_publication_unchanged() {
    let (root, paths) = test_paths("inventory-panel-coalescing");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(None, &paths, cfg, HardwareInventory::default(), active);
    let clock = ProductionClock::default();
    let boot = clock.snapshot();
    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, Vec::new());
    let _ = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let full = scheduler.handle(SchedulerEvent::DisplayRefreshRequested {
        at: SchedulerTime::ZERO,
    });
    let panel_before = full
        .actions
        .iter()
        .find(|action| matches!(action, SchedulerAction::PublishDisplay { panel: true, .. }))
        .cloned()
        .expect("full publication");
    let tooltip_refresh = scheduler.handle(SchedulerEvent::TooltipRefreshRequested {
        at: SchedulerTime::ZERO,
    });
    let mut actions = VecDeque::from(full.actions);
    actions.extend(tooltip_refresh.actions);

    reconcile_inventory_publication(
        true,
        &mut state,
        &mut scheduler,
        &clock,
        boot,
        SchedulerTime::ZERO,
        &mut actions,
    );

    let publications = actions
        .iter()
        .filter(|action| matches!(action, SchedulerAction::PublishDisplay { .. }))
        .collect::<Vec<_>>();
    assert_eq!(publications, vec![&panel_before]);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn deferred_first_paint_limits_sleep_to_original_deadline() {
    let (root, paths) = test_paths("deferred-first-paint-sleep");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(None, &paths, cfg, HardwareInventory::default(), active);
    state.deferred_first_paint = Some(SchedulerAction::PublishDisplay {
        publication: PublicationId(99),
        reason: PublishReason::FirstPaintReady,
        panel: true,
        tooltip: true,
    });
    let scheduler = Scheduler::new();
    let boot = ClockSnapshot::default();

    assert_eq!(
        sleep_duration(
            &scheduler,
            &state,
            boot,
            ClockSnapshot {
                monotonic: Duration::from_millis(190),
                ..ClockSnapshot::default()
            },
        ),
        Duration::from_millis(10)
    );
    assert_eq!(
        sleep_duration(
            &scheduler,
            &state,
            boot,
            ClockSnapshot {
                monotonic: FIRST_PAINT_DEADLINE,
                ..ClockSnapshot::default()
            },
        ),
        Duration::ZERO
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn stale_resume_rescan_retry_preserves_acknowledgement_until_accepted() {
    let (root, paths) = test_paths("resume-rescan-retry");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(None, &paths, cfg, HardwareInventory::default(), active);
    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, Vec::new());
    let _ = scheduler.handle(SchedulerEvent::Suspend {
        at: SchedulerTime::ZERO,
    });
    let resumed = scheduler.handle(SchedulerEvent::Resume {
        at: SchedulerTime::ZERO,
    });
    let acknowledgement = resumed.actions.iter().find_map(|action| match action {
        SchedulerAction::RescanHardware {
            resume_reconciliation,
            ..
        } => *resume_reconciliation,
        _ => None,
    });
    let _ = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::ZERO,
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: Vec::new(),
            demand: DemandPlan::default(),
            resume_acknowledgement: None,
        },
    });
    state.inventory_generation = InventoryGeneration(2);

    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let validity = DispatchValidity::default();
    let mut actions = VecDeque::new();
    let mut owner_messages = VecDeque::new();
    handle_completion(
        OwnerCompletion::Rescan {
            config_generation: ConfigGeneration(1),
            inventory_generation: InventoryGeneration(1),
            lifecycle_generation: 0,
            resume_reconciliation: acknowledgement,
            hw: Box::new(HardwareInventory::default()),
        },
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
    let Some((OwnerId::Discovery, OwnerMessage::Rescan(retry))) = owner_messages.pop_front() else {
        panic!("resume rescan retry")
    };
    assert_eq!(retry.inventory_generation, InventoryGeneration(2));
    assert_eq!(retry.resume_reconciliation, acknowledgement);

    handle_completion(
        OwnerCompletion::Rescan {
            config_generation: retry.config_generation,
            inventory_generation: retry.inventory_generation,
            lifecycle_generation: retry.lifecycle_generation,
            resume_reconciliation: retry.resume_reconciliation,
            hw: Box::new(retry.hw),
        },
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

    assert!(
        actions
            .iter()
            .any(|action| matches!(action, SchedulerAction::StartJob { .. }))
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn job_finished_fallback_start_suppresses_explicit_nvml_trigger() {
    let mut scheduler = Scheduler::new();
    let initial = startup(
        &mut scheduler,
        vec![
            job(OwnerId::Nvidia, JobKind::NvidiaNvml, false),
            job(OwnerId::Nvidia, JobKind::NvidiaFallback, false),
        ],
    );
    let nvml_ticket = initial
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::NvidiaNvml => {
                Some(ticket.clone())
            }
            _ => None,
        })
        .expect("NVML start");
    let finished = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::ZERO,
        ticket: nvml_ticket,
        completion: CompletionKind::Failed,
        notification_ready: false,
    });

    assert!(transition_starts_job(&finished, JobKind::NvidiaFallback));
    assert!(!should_trigger_nvidia_fallback(
        true,
        JobKind::NvidiaNvml,
        CompletionKind::Failed,
        &finished,
    ));
}
