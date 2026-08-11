#![allow(clippy::expect_used)]

use std::cell::Cell;
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, UNIX_EPOCH};

use crate::config::NotificationConfig;
use crate::domain::boundary::{
    BoundaryError, CommandOutput, CommandRunner, CommandStatus, DbusFacade, DbusOutput,
    DbusRequest, FilesystemRoots, NotificationError, NotificationFacade, NotificationPayload,
};
use crate::page_commands::{CommandLookup, FULL_PAGE, Page, PageCommandSpec, PageSource};
use crate::scheduler::{
    CancelReason, DemandPlan, EventDisposition, JobSpec, OwnerId, PageId, PublishReason,
    RescanKind, SchedulerConfig, SourceIdentity,
};

use super::*;

fn ticket(run_id: u64, kind: JobKind) -> JobTicket {
    JobTicket {
        run_id: RunId(run_id),
        job: JobId::singleton(OwnerId::Network, kind),
        config_generation: ConfigGeneration(1),
        inventory_generation: InventoryGeneration(1),
        history_deadline: None,
    }
}

fn transition(actions: Vec<SchedulerAction>) -> Transition {
    Transition {
        disposition: EventDisposition::Accepted,
        actions,
    }
}

fn action_trace(queue: &ActionQueue) -> Vec<&'static str> {
    queue
        .pending
        .iter()
        .map(|action| match action {
            QueuedAction::Scheduler(SchedulerAction::InvalidateJob { .. }) => "invalidate",
            QueuedAction::Scheduler(SchedulerAction::CancelJob { .. }) => "cancel",
            QueuedAction::Scheduler(SchedulerAction::ResetCounterBaseline { .. }) => "reset",
            QueuedAction::Scheduler(SchedulerAction::PublishDisplay { .. }) => "publish",
            QueuedAction::Scheduler(SchedulerAction::EvaluateNotifications { .. }) => "notify",
            QueuedAction::Scheduler(SchedulerAction::RescanHardware { .. }) => "rescan",
            QueuedAction::Scheduler(SchedulerAction::ApplyBackoff { .. }) => "backoff",
            QueuedAction::Scheduler(SchedulerAction::StartJob { ticket })
                if ticket.run_id == RunId(1) =>
            {
                "retained-start"
            }
            QueuedAction::Scheduler(SchedulerAction::StartJob { .. }) => "new-start",
            QueuedAction::Scheduler(SchedulerAction::ScheduleDeadline { .. }) => "deadline",
            QueuedAction::Scheduler(SchedulerAction::Terminate { .. }) => "terminate",
            QueuedAction::RunStart(_) => "run-start",
            QueuedAction::Commit(_) => "commit",
            QueuedAction::PostCompletion {
                inventory_changed: true,
                ..
            } => "inventory-changed",
            QueuedAction::PostCompletion { .. } => "post-completion",
        })
        .collect()
}

#[test]
fn newly_emitted_correctness_trace_precedes_retained_start() {
    let retained = ticket(1, JobKind::NetworkIdentity);
    let newly_emitted = ticket(2, JobKind::NetworkRate);
    let mut queue = ActionQueue::default();
    queue
        .pending
        .push_back(QueuedAction::Scheduler(SchedulerAction::StartJob {
            ticket: retained.clone(),
        }));
    queue
        .pending
        .push_back(QueuedAction::Scheduler(SchedulerAction::ScheduleDeadline {
            at: SchedulerTime::from_duration(Duration::from_secs(1)),
        }));

    queue.extend(transition(vec![
        SchedulerAction::InvalidateJob {
            job: retained.job.clone(),
            reason: CancelReason::SourceReplaced,
        },
        SchedulerAction::CancelJob {
            ticket: retained,
            reason: CancelReason::SourceReplaced,
        },
        SchedulerAction::ResetCounterBaseline {
            job: newly_emitted.job.clone(),
        },
        SchedulerAction::PublishDisplay {
            publication: PublicationId(1),
            reason: PublishReason::DisplayDeadline,
            panel: true,
            tooltip: false,
        },
        SchedulerAction::EvaluateNotifications {
            ticket: newly_emitted.clone(),
        },
        SchedulerAction::RescanHardware {
            kind: RescanKind::Hardware,
            resume_reconciliation: None,
        },
        SchedulerAction::ApplyBackoff {
            job: newly_emitted.job.clone(),
            failures: 1,
            retry_at: SchedulerTime::from_duration(Duration::from_secs(2)),
        },
        SchedulerAction::StartJob {
            ticket: newly_emitted,
        },
        SchedulerAction::ScheduleDeadline {
            at: SchedulerTime::from_duration(Duration::from_secs(2)),
        },
        SchedulerAction::Terminate { component: None },
    ]));

    assert_eq!(
        action_trace(&queue),
        [
            "invalidate",
            "cancel",
            "reset",
            "publish",
            "notify",
            "rescan",
            "backoff",
            "terminate",
            "retained-start",
            "deadline",
            "new-start",
            "deadline",
        ]
    );
}

#[derive(Default)]
struct CountingCommands {
    calls: usize,
}

impl CommandRunner for CountingCommands {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        _timeout: Duration,
    ) -> std::result::Result<CommandOutput, BoundaryError> {
        self.calls = self.calls.saturating_add(1);
        Err(BoundaryError::CommandFailed {
            program: program.to_path_buf(),
            args: args.to_vec(),
            detail: String::from("unexpected stale-source I/O"),
        })
    }
}

#[derive(Default)]
struct SerialCommands {
    events: Vec<&'static str>,
    active: bool,
}

impl CommandRunner for SerialCommands {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        _timeout: Duration,
    ) -> std::result::Result<CommandOutput, BoundaryError> {
        let (started, finished) = match program {
            path if path == Path::new("/fixture/sibling-a") => ("a-start", "a-end"),
            path if path == Path::new("/fixture/sibling-b") => ("b-start", "b-end"),
            _ => panic!("unexpected command path: {program:?}"),
        };
        assert!(!self.active, "same-owner command attempts overlapped");
        self.active = true;
        self.events.push(started);
        self.active = false;
        self.events.push(finished);
        Ok(CommandOutput {
            program: program.to_path_buf(),
            args: args.to_vec(),
            status: CommandStatus::Exit(0),
            stdout: b"sample\n".to_vec(),
            stderr: Vec::new(),
            truncation: Default::default(),
        })
    }
}

struct AbsentDbus;

impl DbusFacade for AbsentDbus {
    fn call(&mut self, request: DbusRequest) -> std::result::Result<DbusOutput, BoundaryError> {
        let (bus, service, path, interface, member) = request.metadata();
        Err(BoundaryError::DbusCallFailed {
            bus,
            service: service.to_owned(),
            path: path.to_owned(),
            interface: interface.to_owned(),
            member: member.to_owned(),
            detail: String::from("fixture absent"),
        })
    }
}

struct RecordingNotifications;

impl NotificationFacade for RecordingNotifications {
    fn send(
        &mut self,
        _payload: &NotificationPayload,
    ) -> std::result::Result<(), NotificationError> {
        Ok(())
    }
}

struct FixedControl {
    now: Cell<Duration>,
}

impl LoopControl for FixedControl {
    fn snapshot(&mut self) -> ClockSnapshot {
        let now = self.now.get();
        ClockSnapshot {
            monotonic: now,
            wall: UNIX_EPOCH + now,
        }
    }

    fn sleep(&mut self, duration: Duration) {
        self.now.set(self.now.get().saturating_add(duration));
    }

    fn should_stop(&self) -> bool {
        false
    }
}

#[test]
fn same_owner_fast_siblings_run_serially_in_deadline_order_within_one_drain() {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-same-owner-drain-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let (roots, paths) = test_paths(&root);
    let page_a = Page {
        id: "sibling-a",
        label: "Sibling A",
        source: PageSource::Command(PageCommandSpec {
            argv: &["sibling-a"],
            ttl: Duration::ZERO,
            max_lines: 0,
            pty: false,
            colorize: None,
        }),
        click: &[],
    };
    let page_b = Page {
        id: "sibling-b",
        label: "Sibling B",
        source: PageSource::Command(PageCommandSpec {
            argv: &["sibling-b"],
            ttl: Duration::ZERO,
            max_lines: 0,
            pty: false,
            colorize: None,
        }),
        click: &[],
    };
    let job_a = JobId::with_source(
        OwnerId::Page,
        JobKind::PageCommand,
        SourceIdentity::Page(PageId::Other(String::from(page_a.id))),
    );
    let job_b = JobId::with_source(
        OwnerId::Page,
        JobKind::PageCommand,
        SourceIdentity::Page(PageId::Other(String::from(page_b.id))),
    );
    let mut initial_demand = DemandPlan::default();
    initial_demand.hidden.extend([job_a.clone(), job_b.clone()]);
    let mut scheduler = Scheduler::new();
    let startup = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: SchedulerConfig {
            generation: ConfigGeneration(1),
            display_interval: Duration::from_secs(1),
            jobs: vec![
                JobSpec::fast(job_b.clone(), Duration::from_secs(1)),
                JobSpec::fast(job_a.clone(), Duration::from_secs(1)),
            ],
            demand: initial_demand,
        },
        inventory_generation: InventoryGeneration(1),
    });

    let mut state = runtime_state(&root);
    state.active = vec![FULL_PAGE, page_a, page_b];
    let mut commands = SerialCommands::default();
    let mut dbus = AbsentDbus;
    let mut notifications = RecordingNotifications;
    let mut control = FixedControl {
        now: Cell::new(Duration::ZERO),
    };
    let mut command_lookup = CommandLookup::new();
    command_lookup
        .insert("sibling-a", "/fixture/sibling-a")
        .insert("sibling-b", "/fixture/sibling-b");

    {
        let mut boundaries = DaemonBoundaries {
            commands: &mut commands,
            dbus: &mut dbus,
            notifications: &mut notifications,
            nvml: None,
            bolt: None,
        };
        execute_transition(
            startup,
            &mut scheduler,
            &mut state,
            &roots,
            &paths,
            &mut boundaries,
            &mut control,
            &command_lookup,
            ClockSnapshot {
                monotonic: Duration::ZERO,
                wall: UNIX_EPOCH,
            },
        )
        .expect("initial sibling attempt");
    }

    let due = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_millis(950)),
    });
    assert!(due.actions.iter().any(|action| {
        matches!(action, SchedulerAction::StartJob { ticket } if ticket.job == job_a)
    }));
    control.now.set(Duration::from_millis(950));
    {
        let mut boundaries = DaemonBoundaries {
            commands: &mut commands,
            dbus: &mut dbus,
            notifications: &mut notifications,
            nvml: None,
            bolt: None,
        };
        execute_transition(
            due,
            &mut scheduler,
            &mut state,
            &roots,
            &paths,
            &mut boundaries,
            &mut control,
            &command_lookup,
            ClockSnapshot {
                monotonic: Duration::from_millis(950),
                wall: UNIX_EPOCH + Duration::from_millis(950),
            },
        )
        .expect("deadline sibling attempts");
    }

    assert_eq!(
        commands.events,
        [
            "a-start", "a-end", "b-start", "b-end", "a-start", "a-end", "b-start", "b-end"
        ]
    );
    assert!(!commands.active);
    assert!(state.action_queue.pending.is_empty());
    assert_eq!(control.now.get(), Duration::from_millis(950));
    let _ = fs::remove_dir_all(root);
}

fn test_paths(root: &Path) -> (FilesystemRoots, DaemonPaths) {
    let runtime = root.join("runtime");
    let state = runtime.join("state");
    fs::create_dir_all(&state).expect("runtime fixture");
    let roots = FilesystemRoots {
        runtime_root: Some(runtime.clone()),
        cache_root: Some(root.join("cache")),
        config_root: Some(root.join("config")),
        proc_root: root.join("proc"),
        sys_root: root.join("sys"),
    };
    fs::create_dir_all(&roots.proc_root).expect("proc fixture");
    fs::create_dir_all(&roots.sys_root).expect("sys fixture");
    let paths = DaemonPaths {
        runtime: runtime.clone(),
        state,
        panel: runtime.join("panel.html"),
        tooltip: runtime.join("tooltip.html"),
        page: runtime.join("state/page"),
        npages: runtime.join("state/npages"),
        geom: runtime.join("state/geom"),
        plasma_config: root.join("appletsrc"),
        kdeglobals: root.join("kdeglobals"),
    };
    (roots, paths)
}

fn runtime_state(root: &Path) -> RuntimeState {
    let mut cfg = Config::default();
    cfg.pages.order.clear();
    cfg.notifications = NotificationConfig {
        disk_usage: false,
        disk_smart: false,
        cpu_temp: false,
        gpu_nvidia_temp: false,
        hd_temp: false,
        battery_sys: false,
        battery_mouse: false,
        battery_kbd: false,
        server_check: false,
        load_avg: false,
    };
    RuntimeState {
        cfg,
        hw: HardwareInventory::default(),
        active: vec![Page {
            id: "full",
            label: "Main",
            source: PageSource::Full,
            click: &[],
        }],
        owners: Owners::new(),
        readings: DisplaySnapshot::default(),
        notifications: NotificationState::default(),
        notification_samples: BTreeMap::new(),
        command_cache: PageCommandCache::new(),
        config_generation: ConfigGeneration(1),
        inventory_generation: InventoryGeneration(1),
        css: String::new(),
        light: false,
        css_path: root.join("style.css"),
        overlay_path: None,
        css_stamp: None,
        overlay_stamp: None,
        config_stamp: None,
        machine_stamps: Vec::new(),
        plasma_stamp: None,
        geom_stamp: None,
        kde_stamp: None,
        updates_stamp: None,
        server_stamp: None,
        panel_html: None,
        tooltip_html: None,
        display_publication_count: 0,
        first_paint_published: false,
        theme_reconciliation_pending: true,
        shutdown_requested: false,
        action_queue: ActionQueue::default(),
        resolved_mounts: Vec::new(),
        boot_pending: BTreeSet::new(),
        terminate: false,
    }
}

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

#[test]
fn reload_publishes_overdue_retained_snapshot_before_reanchor() {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-reload-deadline-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let (roots, paths) = test_paths(&root);
    let mut state = runtime_state(&root);
    let mut discovered_hw = state.hw.clone();
    discovered_hw.net_device = Some(String::from("wlan0"));
    let mut scheduler = Scheduler::new();
    scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: SchedulerConfig {
            generation: ConfigGeneration(1),
            display_interval: Duration::from_millis(250),
            jobs: Vec::new(),
            demand: DemandPlan::default(),
        },
        inventory_generation: InventoryGeneration(1),
    });
    scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_millis(100)),
    });
    let mut control = FixedControl {
        now: Cell::new(Duration::from_millis(300)),
    };
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

    reload::apply_config(
        Config::default(),
        Some(discovered_hw),
        &paths,
        &roots,
        &mut scheduler,
        &mut state,
        &mut control,
        &mut boundaries,
        &CommandLookup::new(),
        ClockSnapshot {
            monotonic: Duration::ZERO,
            wall: UNIX_EPOCH,
        },
    )
    .expect("reload transition");

    assert_eq!(
        state.display_publication_count, 1,
        "the overdue retained snapshot must publish exactly once before reanchor"
    );
    assert_eq!(state.inventory_generation, InventoryGeneration(2));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn activation_refresh_waits_for_inventory_source_removal_barrier() {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-activation-inventory-barrier-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let source = JobId::with_source(
        OwnerId::Network,
        JobKind::NetworkRate,
        SourceIdentity::Device(String::from("wlan0")),
    );
    let mut demand = DemandPlan::default();
    demand.main_tooltip.insert(source.clone());
    let mut scheduler = Scheduler::new();
    scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: SchedulerConfig {
            generation: ConfigGeneration(1),
            display_interval: Duration::from_secs(1),
            jobs: vec![JobSpec::fast(source.clone(), Duration::from_secs(1))],
            demand,
        },
        inventory_generation: InventoryGeneration(1),
    });
    let activated = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let ticket = activated
        .actions
        .into_iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } => Some(ticket),
            _ => None,
        })
        .expect("activation source start");

    let mut state = runtime_state(&root);
    state.hw.net_device = Some(String::from("wlan0"));
    let mut attempted = clone_attempt_state(&state);
    attempted.hw.net_device = None;
    let staged = StagedAttempt {
        ticket,
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
        action_trace(&queue)[..2],
        ["inventory-changed", "publish"],
        "source removal must reconcile before the completion-generated activation refresh"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn production_mount_inventory_replaces_only_from_confirmed_enumeration() {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-mount-inventory-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let (roots, _paths) = test_paths(&root);
    let mut state = runtime_state(&root);
    state.cfg.disks.mounts = crate::config::Mounts::Auto;
    state.cfg.disks.auto_roots = vec![String::from("/mnt")];
    state.cfg.notifications.disk_usage = true;
    state.resolved_mounts = vec![String::from("/"), String::from("/mnt/a")];
    state.readings.disk_usage.insert(
        String::from("/mnt/a"),
        Some(crate::domain::readings::DiskUsageReading {
            percent: 40,
            used_gib: 4,
            total_gib: 10,
        }),
    );
    let mount_jobs = |state: &RuntimeState| {
        state
            .scheduler_config(&roots)
            .jobs
            .into_iter()
            .filter_map(|spec| match spec.id.source {
                SourceIdentity::Mount(path) => Some(path),
                _ => None,
            })
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(
        mount_jobs(&state),
        BTreeSet::from([PathBuf::from("/"), PathBuf::from("/mnt/a")])
    );
    fs::write(
        roots.proc_root.join("mounts"),
        "/dev/root / ext4 rw 0 0\n/dev/b /mnt/b ext4 rw 0 0\n",
    )
    .expect("confirmed replacement mounts");
    let inventory_ticket = JobTicket {
        run_id: RunId(1),
        job: JobId::singleton(OwnerId::Discovery, JobKind::MountInventory),
        config_generation: ConfigGeneration(1),
        inventory_generation: InventoryGeneration(1),
        history_deadline: None,
    };
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

    let confirmed = execute_start(
        inventory_ticket.clone(),
        &state,
        &roots,
        &mut boundaries,
        &mut control,
        &CommandLookup::new(),
    );
    assert_eq!(confirmed.completion, CompletionKind::Captured);
    assert_eq!(confirmed.state.resolved_mounts, ["/", "/mnt/b"]);

    state.resolved_mounts = confirmed.state.resolved_mounts;
    assert_eq!(
        mount_jobs(&state),
        BTreeSet::from([PathBuf::from("/"), PathBuf::from("/mnt/b")])
    );
    state.readings.disk_usage.clear();
    state.readings.disk_usage.insert(
        String::from("/mnt/b"),
        Some(crate::domain::readings::DiskUsageReading {
            percent: 50,
            used_gib: 5,
            total_gib: 10,
        }),
    );
    fs::write(roots.proc_root.join("mounts"), "/dev/root / ext4\n").expect("malformed mounts");
    let failed = execute_start(
        inventory_ticket.clone(),
        &state,
        &roots,
        &mut boundaries,
        &mut control,
        &CommandLookup::new(),
    );
    assert_eq!(failed.completion, CompletionKind::Failed);
    assert_eq!(failed.state.resolved_mounts, ["/", "/mnt/b"]);
    assert_eq!(failed.state.readings.disk_usage, state.readings.disk_usage);
    assert_eq!(
        mount_jobs(&state),
        BTreeSet::from([PathBuf::from("/"), PathBuf::from("/mnt/b")])
    );

    fs::remove_file(roots.proc_root.join("mounts")).expect("unreadable mounts fixture");
    let unreadable = execute_start(
        inventory_ticket,
        &state,
        &roots,
        &mut boundaries,
        &mut control,
        &CommandLookup::new(),
    );
    assert_eq!(unreadable.completion, CompletionKind::Failed);
    assert_eq!(unreadable.state.resolved_mounts, ["/", "/mnt/b"]);
    assert_eq!(
        unreadable.state.readings.disk_usage,
        state.readings.disk_usage
    );
    let _ = fs::remove_dir_all(root);
}
