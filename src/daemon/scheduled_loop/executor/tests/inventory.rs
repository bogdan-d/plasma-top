use super::*;

#[test]
fn failed_hardware_reconciliation_retains_inventory_and_retries_promptly() {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-discovery-backoff-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let (roots, _paths) = test_paths(&root);
    let discovery = JobId::with_source(
        OwnerId::Discovery,
        JobKind::HardwareDiscovery,
        SourceIdentity::Inventory(InventoryFamily::Network),
    );
    let mut demand = DemandPlan::default();
    demand.hidden.insert(discovery.clone());
    let mut scheduler = Scheduler::new();
    let startup = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: SchedulerConfig {
            generation: ConfigGeneration(1),
            display_interval: Duration::from_secs(10),
            jobs: vec![JobSpec::periodic(
                discovery.clone(),
                Duration::from_secs(60),
            )],
            demand,
        },
        inventory_generation: InventoryGeneration(1),
    });
    let ticket = startup
        .actions
        .into_iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } => Some(ticket),
            _ => None,
        })
        .expect("hardware discovery start");
    let mut state = runtime_state(&root);
    state.hw.net_device = Some(String::from("eth0"));
    let mut commands = CountingCommands::default();
    let mut dbus = AbsentDbus;
    let mut notifications = RecordingNotifications;
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        nvml: None,
        bolt: None,
    };
    let mut control = FixedControl {
        now: Cell::new(Duration::ZERO),
    };

    let staged = execute_start(
        ticket,
        &state,
        &roots,
        &mut boundaries,
        &mut control,
        &CommandLookup::new(),
    );

    assert_eq!(staged.completion, CompletionKind::Failed);
    assert_eq!(staged.state.hw.net_device.as_deref(), Some("eth0"));
    let mut queue = ActionQueue::default();
    commit_attempt(staged, &mut queue, &mut scheduler, &mut state, &mut control);
    assert_eq!(state.hw.net_device.as_deref(), Some("eth0"));
    assert!(queue.pending.iter().any(|action| {
        matches!(
            action,
            QueuedAction::Scheduler(SchedulerAction::ApplyBackoff {
                job,
                failures: 1,
                retry_at,
            }) if job == &discovery
                && *retry_at == SchedulerTime::from_duration(Duration::from_millis(100))
        )
    }));
    assert!(
        scheduler
            .handle(SchedulerEvent::TimeAdvanced {
                at: SchedulerTime::from_duration(Duration::from_millis(99)),
            })
            .actions
            .iter()
            .all(|action| !matches!(action, SchedulerAction::StartJob { .. }))
    );
    assert!(
        scheduler
            .handle(SchedulerEvent::TimeAdvanced {
                at: SchedulerTime::from_duration(Duration::from_millis(100)),
            })
            .actions
            .iter()
            .any(|action| {
                matches!(action, SchedulerAction::StartJob { ticket } if ticket.job == discovery)
            })
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn inventory_changing_completion_trace_rejects_old_source_before_io() {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-inventory-barrier-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let (roots, paths) = test_paths(&root);
    let inventory_job = JobId::singleton(OwnerId::Network, JobKind::NetworkIdentity);
    let stale_source_job = JobId::with_source(
        OwnerId::Network,
        JobKind::NetworkIdentity,
        SourceIdentity::Device(String::from("eth0")),
    );
    let mut inventory_spec = JobSpec::periodic(inventory_job.clone(), Duration::from_secs(60));
    inventory_spec.startup_panel = true;
    let stale_source_spec = JobSpec::periodic(stale_source_job.clone(), Duration::from_secs(10));
    let mut demand = DemandPlan::default();
    demand
        .hidden
        .extend([inventory_job.clone(), stale_source_job]);
    let mut scheduler = Scheduler::new();
    let startup = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: SchedulerConfig {
            generation: ConfigGeneration(1),
            display_interval: Duration::from_secs(1),
            jobs: vec![inventory_spec, stale_source_spec],
            demand,
        },
        inventory_generation: InventoryGeneration(1),
    });
    let inventory_ticket = startup
        .actions
        .into_iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } => Some(ticket),
            _ => None,
        })
        .expect("inventory start ticket");
    assert_eq!(inventory_ticket.job, inventory_job);

    let mut state = runtime_state(&root);
    let mut attempted = clone_attempt_state(&state);
    attempted.hw.net_device = Some(String::from("wlan0"));
    let staged = StagedAttempt {
        ticket: inventory_ticket,
        state: attempted,
        completion: CompletionKind::Captured,
        notification_ready: false,
        notifications: DisplaySnapshot::default(),
    };
    let mut control = FixedControl {
        now: Cell::new(Duration::ZERO),
    };
    let mut queue = ActionQueue::default();
    commit_attempt(staged, &mut queue, &mut scheduler, &mut state, &mut control);

    assert_eq!(
        action_trace(&queue),
        ["inventory-changed", "publish", "new-start", "deadline"],
        "the typed inventory update must be the barrier before completion-generated work"
    );
    let stale_ticket = queue
        .pending
        .iter()
        .find_map(|action| match action {
            QueuedAction::Scheduler(SchedulerAction::StartJob { ticket }) => Some(ticket.clone()),
            _ => None,
        })
        .expect("completion-generated stale-source ticket");

    state.action_queue = queue;
    let mut commands = CountingCommands::default();
    let mut dbus = AbsentDbus;
    let mut notifications = RecordingNotifications;
    let mut boundaries = DaemonBoundaries {
        commands: &mut commands,
        dbus: &mut dbus,
        notifications: &mut notifications,
        nvml: None,
        bolt: None,
    };
    execute_transition(
        transition(Vec::new()),
        &mut scheduler,
        &mut state,
        &roots,
        &paths,
        &mut boundaries,
        &mut control,
        &CommandLookup::new(),
        ClockSnapshot {
            monotonic: Duration::ZERO,
            wall: UNIX_EPOCH,
        },
    )
    .expect("inventory barrier transition");

    assert_eq!(state.inventory_generation, InventoryGeneration(2));
    assert!(!scheduler.is_current_ticket(&stale_ticket));
    assert_eq!(commands.calls, 0, "stale route work performed I/O");
    assert!(state.action_queue.pending.is_empty());
    let _ = fs::remove_dir_all(root);
}
