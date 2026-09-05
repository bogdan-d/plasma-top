#![allow(clippy::expect_used)]

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::{mpsc, oneshot};

use super::*;
use crate::config::Config;
use crate::domain::boundary::IoEvent;
use crate::domain::readings::{DisplaySnapshot, HardwareInventory, RetainedMetricSample};
use crate::page_commands::build_pages;
use crate::scheduler::{
    CompletionKind, ConfigGeneration, DemandPlan, EventDisposition, HistoryDeadline,
    InventoryGeneration, InventoryUpdate, JobId, JobSpec, JobTicket, PublishReason, RunId,
    SchedulerConfig, SourceIdentity,
};
use worker::OwnerCompletion;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .start_paused(true)
        .build()
        .expect("paused runtime")
}

#[path = "tests/completion.rs"]
mod completion_tests;
#[path = "tests/control.rs"]
mod control_tests;
#[path = "tests/graph_render.rs"]
mod graph_render_tests;
#[path = "tests/integration.rs"]
mod integration;
#[path = "tests/reload.rs"]
mod reload_tests;
#[path = "tests/state.rs"]
mod state_tests;

fn job(owner: OwnerId, kind: JobKind, startup_panel: bool) -> JobSpec {
    let mut spec = JobSpec::fast(JobId::singleton(owner, kind), Duration::from_millis(250));
    spec.startup_panel = startup_panel;
    spec
}

fn scheduler_config(generation: u64, jobs: Vec<JobSpec>) -> SchedulerConfig {
    let hidden = jobs.iter().map(|spec| spec.id.clone()).collect();
    SchedulerConfig {
        generation: ConfigGeneration(generation),
        display_interval: Duration::from_millis(250),
        jobs,
        demand: DemandPlan {
            hidden,
            ..DemandPlan::default()
        },
    }
}

fn startup(scheduler: &mut Scheduler, jobs: Vec<JobSpec>) -> Transition {
    scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: scheduler_config(1, jobs),
        inventory_generation: InventoryGeneration(1),
    })
}

fn started_ticket(transition: &Transition) -> JobTicket {
    transition
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } => Some(ticket.clone()),
            _ => None,
        })
        .expect("start ticket")
}

#[test]
fn blocked_owner_does_not_delay_two_hundred_millisecond_first_paint() {
    runtime().block_on(async {
        let mut scheduler = Scheduler::new();
        let transition = startup(
            &mut scheduler,
            vec![job(OwnerId::Process, JobKind::PanelProcesses, true)],
        );
        let ticket = started_ticket(&transition);
        let (request_sender, mut requests) = mpsc::channel(1);
        let (release_sender, release_receiver) = oneshot::channel::<()>();
        let owner = tokio::spawn(async move {
            let received = requests.recv().await;
            let _ = release_receiver.await;
            received
        });
        request_sender.send(ticket).await.expect("dispatch owner");

        tokio::time::advance(Duration::from_millis(200)).await;
        let deadline = scheduler.handle(SchedulerEvent::TimeAdvanced {
            at: SchedulerTime::from_duration(Duration::from_millis(200)),
        });
        assert!(deadline.actions.iter().any(|action| {
            matches!(
                action,
                SchedulerAction::PublishDisplay {
                    reason: PublishReason::FirstPaintTimeout,
                    panel: true,
                    ..
                }
            )
        }));
        assert!(!owner.is_finished(), "blocked owner unexpectedly completed");
        let _ = release_sender.send(());
        let _ = owner.await;
    });
}

#[test]
fn same_owner_jobs_execute_without_overlap() {
    runtime().block_on(async {
        let mut scheduler = Scheduler::new();
        let transition = startup(
            &mut scheduler,
            vec![
                job(OwnerId::Cpu, JobKind::Cpu, false),
                job(OwnerId::Cpu, JobKind::CpuCores, false),
            ],
        );
        let starts = transition
            .actions
            .iter()
            .filter(|action| matches!(action, SchedulerAction::StartJob { .. }))
            .count();
        assert_eq!(starts, 1);
        let first = started_ticket(&transition);
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let (sender, mut receiver) = mpsc::channel(2);
        let worker_active = Arc::clone(&active);
        let worker_maximum = Arc::clone(&maximum);
        let worker = tokio::spawn(async move {
            while receiver.recv().await.is_some() {
                let current = worker_active.fetch_add(1, Ordering::SeqCst) + 1;
                worker_maximum.fetch_max(current, Ordering::SeqCst);
                tokio::task::yield_now().await;
                worker_active.fetch_sub(1, Ordering::SeqCst);
            }
        });
        sender.send(first.clone()).await.expect("first dispatch");
        tokio::task::yield_now().await;
        let completed = scheduler.handle(SchedulerEvent::JobFinished {
            at: SchedulerTime::ZERO,
            ticket: first,
            completion: CompletionKind::Captured,
            notification_ready: false,
        });
        let second = started_ticket(&completed);
        sender.send(second).await.expect("second dispatch");
        drop(sender);
        worker.await.expect("owner worker");
        assert_eq!(maximum.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn filesystem_sensor_attempts_use_the_single_blocking_lane() {
    for kind in [JobKind::UpdatesFile, JobKind::ServerFile] {
        assert!(worker::uses_blocking_lane(&JobId::singleton(
            OwnerId::External,
            kind
        )));
    }
    for kind in [
        JobKind::Cpu,
        JobKind::Memory,
        JobKind::DiskTemperature,
        JobKind::FanSpeed,
        JobKind::Brightness,
        JobKind::CpuHistory,
    ] {
        assert!(!worker::uses_blocking_lane(&JobId::singleton(
            OwnerId::Cpu,
            kind
        )));
    }
}

#[test]
fn full_owner_channel_retains_correctness_message_until_capacity_returns() {
    let owner = OwnerId::Cpu;
    let reset = JobId::singleton(owner, JobKind::Cpu);
    let (sender, mut receiver) = mpsc::channel(1);
    sender
        .try_send(OwnerMessage::Reset(reset.clone()))
        .expect("fill channel");
    let senders = OwnerSenders::single(owner, sender);
    let mut pending = VecDeque::from([(owner, OwnerMessage::Invalidate(reset, None))]);

    assert_eq!(flush_owner_messages(&mut pending, &senders), None);
    assert_eq!(pending.len(), 1, "full channel dropped correctness work");
    assert!(matches!(receiver.try_recv(), Ok(OwnerMessage::Reset(_))));
    assert_eq!(flush_owner_messages(&mut pending, &senders), None);
    assert!(pending.is_empty());
    assert!(matches!(
        receiver.try_recv(),
        Ok(OwnerMessage::Invalidate(..))
    ));
}

#[test]
fn full_completion_channel_backpressures_owner_until_control_drains() {
    runtime().block_on(async {
        let (sender, mut receiver) = mpsc::channel(1);
        sender
            .send(OwnerCompletion::Cancelled(test_ticket(RunId(1))))
            .await
            .expect("fill completion channel");
        let blocked = tokio::spawn(async move {
            sender
                .send(OwnerCompletion::Cancelled(test_ticket(RunId(2))))
                .await
        });
        tokio::task::yield_now().await;
        assert!(!blocked.is_finished());
        assert!(receiver.recv().await.is_some());
        assert!(blocked.await.expect("blocked completion sender").is_ok());
    });
}

#[test]
fn repeated_replaceable_owner_controls_coalesce_to_latest() {
    let mut pending = VecDeque::new();
    let first = build_pages(&[]);
    let latest = build_pages(&[String::from("graphs")]);
    assert!(queue_owner_message(
        &mut pending,
        (OwnerId::Page, OwnerMessage::PagesChanged(first)),
    ));
    assert!(queue_owner_message(
        &mut pending,
        (OwnerId::Page, OwnerMessage::PagesChanged(latest.clone())),
    ));

    assert_eq!(pending.len(), 1);
    let Some((_, OwnerMessage::PagesChanged(pages))) = pending.pop_front() else {
        panic!("latest page control")
    };
    assert_eq!(pages, latest);
}

#[test]
fn distinct_owner_controls_cannot_exceed_pending_capacity() {
    let mut pending = VecDeque::new();
    for index in 0..OWNER_PENDING_CAPACITY {
        let job = JobId::with_source(
            OwnerId::Disk,
            JobKind::DiskTemperature,
            SourceIdentity::Device(index.to_string()),
        );
        assert!(queue_owner_message(
            &mut pending,
            (OwnerId::Disk, OwnerMessage::Reset(job)),
        ));
    }
    let overflow = JobId::with_source(
        OwnerId::Disk,
        JobKind::DiskTemperature,
        SourceIdentity::Device(String::from("overflow")),
    );
    assert!(!queue_owner_message(
        &mut pending,
        (OwnerId::Disk, OwnerMessage::Reset(overflow)),
    ));
    assert_eq!(pending.len(), OWNER_PENDING_CAPACITY);
}

#[test]
fn progressive_slow_discovery_dispatches_during_first_paint_window() {
    let owner = OwnerId::Discovery;
    let ticket = JobTicket {
        metrics: Default::default(),
        run_id: RunId(1),
        job: JobId::with_source(
            owner,
            JobKind::HardwareDiscovery,
            SourceIdentity::Inventory(crate::domain::readings::InventoryFamily::Network),
        ),
        config_generation: ConfigGeneration(1),
        inventory_generation: InventoryGeneration(1),
        history_deadline: None,
    };
    let input = JobInput {
        ticket,
        cfg: Config::default(),
        hw: HardwareInventory::default(),
        readings: DisplaySnapshot::default(),
        active: build_pages(&[]),
        selected_index: 0,
        css: String::new(),
        style_generation: 1,
        render_generation: 1,
        resolved_mounts: vec![String::from("/")],
        gpu_decoder_outcome: crate::sensors::gpu_history::DecoderOutcome::Unmeasured,
        gpu_history_point: None,
    };
    let (sender, mut receiver) = mpsc::channel(1);
    let senders = OwnerSenders::single(owner, sender);
    let mut pending = VecDeque::from([(owner, OwnerMessage::Run(Box::new(input)))]);

    assert_eq!(flush_owner_messages(&mut pending, &senders), None);
    assert!(pending.is_empty());
    assert!(matches!(receiver.try_recv(), Ok(OwnerMessage::Run(_))));
}

#[test]
fn owner_exit_becomes_critical_scheduler_termination() {
    runtime().block_on(async {
        let mut scheduler = Scheduler::new();
        let _ = startup(&mut scheduler, Vec::new());
        let clock = ProductionClock::default();
        let mut tasks = JoinSet::new();
        tasks.spawn(async { OwnerId::Disk });
        assert!(matches!(tasks.join_next().await, Some(Ok(OwnerId::Disk))));
        let mut actions = VecDeque::new();
        critical_failure("disk owner", &mut scheduler, &clock, &mut actions);
        assert!(actions.iter().any(|action| {
            matches!(
                action,
                SchedulerAction::Terminate {
                    component: Some("disk owner")
                }
            )
        }));
    });
}

#[test]
fn panicked_owner_is_classified_as_critical_task_exit() {
    runtime().block_on(async {
        let mut tasks = JoinSet::new();
        tasks.spawn(async { panic!("fault-injected owner failure") });
        let exit = tasks.join_next().await;
        assert_eq!(owner_exit_component(exit), "owner task");
    });
}

#[test]
fn first_paint_deadline_writes_real_panel_without_owner_completion() {
    let (root, paths) = test_paths("first-paint");
    fs::create_dir_all(&paths.state).expect("state root");
    write_atomic(&paths.page, "0").expect("page state");
    let cfg = Config::default();
    let hw = HardwareInventory::default();
    let active = build_pages(&cfg.pages.order);
    let mut runtime_state = RuntimeState::new(&paths, cfg, hw, active);
    let roots = FilesystemRoots {
        runtime_root: Some(paths.runtime.clone()),
        ..FilesystemRoots::default()
    };
    let mut scheduler = Scheduler::new();
    let _ = startup(
        &mut scheduler,
        vec![job(OwnerId::Disk, JobKind::DiskUsage, true)],
    );
    let deadline = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_millis(200)),
    });
    let publication = deadline.actions.iter().find_map(|action| match action {
        SchedulerAction::PublishDisplay {
            publication,
            reason: PublishReason::FirstPaintTimeout,
            ..
        } => Some(*publication),
        _ => None,
    });
    let publication = publication.expect("first paint publication");
    runtime_state
        .publish(
            publication,
            PublishReason::FirstPaintTimeout,
            None,
            0,
            true,
            true,
            &mut scheduler,
            &roots,
            &paths,
            ClockSnapshot {
                monotonic: Duration::from_millis(200),
                wall: SystemTime::UNIX_EPOCH,
            },
            ClockSnapshot::default(),
        )
        .expect("publish panel");
    assert!(fs::read_to_string(&paths.panel).is_ok_and(|html| html.contains("class=\"panel")));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graphs_page_change_publishes_selected_placeholder_not_previous_page() {
    let (root, paths) = test_paths("graph-placeholder");
    fs::create_dir_all(&paths.state).expect("state root");
    write_atomic(&paths.page, "1").expect("page state");
    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs")];
    let active = build_pages(&cfg.pages.order);
    let mut runtime_state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    runtime_state.tooltip_html = Some(String::from("OLD PAGE"));
    runtime_state.rendered_graph = Some((PageId::Graphs, 0, String::from("OLD GRAPH")));
    let roots = FilesystemRoots {
        runtime_root: Some(paths.runtime.clone()),
        ..FilesystemRoots::default()
    };
    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, Vec::new());

    runtime_state
        .publish(
            crate::scheduler::PublicationId(1),
            PublishReason::PageChanged,
            None,
            0,
            false,
            true,
            &mut scheduler,
            &roots,
            &paths,
            ClockSnapshot::default(),
            ClockSnapshot::default(),
        )
        .expect("publish graph placeholder");

    let html = fs::read_to_string(&paths.tooltip).expect("tooltip output");
    assert!(html.contains("GRAPHS"));
    assert!(!html.contains("OLD PAGE"));
    assert!(!html.contains("OLD GRAPH"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn graph_completion_from_old_style_generation_is_rejected() {
    let (root, paths) = test_paths("stale-graph-style");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut runtime_state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    runtime_state.style_generation = 2;
    let mut scheduler = Scheduler::new();
    let transition = startup(
        &mut scheduler,
        vec![job(OwnerId::Page, JobKind::PageRender, false)],
    );
    let ticket = started_ticket(&transition);
    let completion = worker::JobCompletion {
        ticket,
        completion: CompletionKind::Captured,
        readings: DisplaySnapshot::default(),
        notifications: DisplaySnapshot::default(),
        notification_ready: false,
        hw: HardwareInventory::default(),
        resolved_mounts: vec![String::from("/")],
        command_cache: None,
        rendered_page: Some(String::from("old style")),
        style_generation: 1,
        render_generation: 1,
        decoder_outcome: None,
        gpu_history_point: None,
        decision: None,
    };

    assert!(!runtime_state.can_commit(&scheduler, &completion));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn notifications_begin_only_after_first_panel_acknowledgement() {
    let mut scheduler = Scheduler::new();
    let initial = startup(&mut scheduler, vec![job(OwnerId::Cpu, JobKind::Cpu, true)]);
    let first = started_ticket(&initial);
    let completed = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::ZERO,
        ticket: first,
        completion: CompletionKind::Captured,
        notification_ready: true,
    });
    assert!(
        !completed
            .actions
            .iter()
            .any(|action| { matches!(action, SchedulerAction::EvaluateNotifications { .. }) })
    );
    let publication = completed.actions.iter().find_map(|action| match action {
        SchedulerAction::PublishDisplay { publication, .. } => Some(*publication),
        _ => None,
    });
    let _ = scheduler.handle(SchedulerEvent::PanelPublished {
        at: SchedulerTime::ZERO,
        publication: publication.expect("first publication"),
    });
    let job = JobId::singleton(OwnerId::Cpu, JobKind::Cpu);
    let refreshed = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: SchedulerTime::ZERO,
        job,
        trigger: RefreshTrigger::Signal,
    });
    let second = started_ticket(&refreshed);
    let completed = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::ZERO,
        ticket: second,
        completion: CompletionKind::Captured,
        notification_ready: true,
    });
    assert!(
        completed
            .actions
            .iter()
            .any(|action| { matches!(action, SchedulerAction::EvaluateNotifications { .. }) })
    );
}

#[test]
fn suspend_resume_waits_for_cancellation_and_inventory_before_refresh() {
    let mut scheduler = Scheduler::new();
    let mut cpu = job(OwnerId::Cpu, JobKind::Cpu, false);
    cpu.counter = true;
    let initial = startup(&mut scheduler, vec![cpu.clone()]);
    let ticket = started_ticket(&initial);
    let suspended = scheduler.handle(SchedulerEvent::Suspend {
        at: SchedulerTime::from_duration(Duration::from_secs(1)),
    });
    assert!(
        suspended
            .actions
            .iter()
            .any(|action| { matches!(action, SchedulerAction::CancelJob { .. }) })
    );
    let resumed = scheduler.handle(SchedulerEvent::Resume {
        at: SchedulerTime::from_duration(Duration::from_secs(2)),
    });
    assert!(
        resumed
            .actions
            .iter()
            .any(|action| { matches!(action, SchedulerAction::ResetCounterBaseline { .. }) })
    );
    assert!(
        resumed
            .actions
            .iter()
            .any(|action| { matches!(action, SchedulerAction::RescanHardware { .. }) })
    );
    assert!(
        !resumed
            .actions
            .iter()
            .any(|action| { matches!(action, SchedulerAction::StartJob { .. }) })
    );
    let resume_reconciliation = resumed.actions.iter().find_map(|action| match action {
        SchedulerAction::RescanHardware {
            resume_reconciliation,
            ..
        } => *resume_reconciliation,
        _ => None,
    });
    let _ = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::from_duration(Duration::from_secs(2)),
        ticket,
        completion: CompletionKind::Captured,
        notification_ready: false,
    });
    let resumed_config = scheduler_config(1, vec![cpu]);
    let inventory = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::from_duration(Duration::from_secs(2)),
        update: InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: resumed_config.jobs,
            demand: resumed_config.demand,
            resume_acknowledgement: resume_reconciliation,
        },
    });
    assert!(
        inventory
            .actions
            .iter()
            .any(|action| { matches!(action, SchedulerAction::StartJob { .. }) })
    );
}

#[test]
fn suspend_preempts_start_actions_already_queued_for_dispatch() {
    let (root, paths) = test_paths("suspend-priority");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    let mut scheduler = Scheduler::new();
    let transition = startup(&mut scheduler, vec![job(OwnerId::Cpu, JobKind::Cpu, false)]);
    let mut actions = VecDeque::from(transition.actions);
    let clock = ProductionClock::default();
    let validity = DispatchValidity::default();

    process_io_events(
        vec![IoEvent::PrepareForSleep(true)],
        &mut scheduler,
        &state,
        &clock,
        &mut actions,
        &validity,
    );

    assert!(matches!(
        actions.front(),
        Some(SchedulerAction::CancelJob { .. })
    ));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn final_suspend_prunes_intermediate_resume_rescan() {
    let (root, paths) = test_paths("sleep-burst");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, Vec::new());
    let mut actions = VecDeque::new();
    let clock = ProductionClock::default();
    let validity = DispatchValidity::default();

    process_io_events(
        vec![
            IoEvent::PrepareForSleep(true),
            IoEvent::PrepareForSleep(false),
            IoEvent::PrepareForSleep(true),
        ],
        &mut scheduler,
        &state,
        &clock,
        &mut actions,
        &validity,
    );

    assert!(!actions.iter().any(|action| {
        matches!(
            action,
            SchedulerAction::RescanHardware { .. } | SchedulerAction::ResetCounterBaseline { .. }
        )
    }));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rescan_completion_from_prior_lifecycle_is_rejected() {
    let (root, paths) = test_paths("stale-rescan-lifecycle");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, Vec::new());
    let validity = DispatchValidity::default();
    validity.advance_lifecycle();
    let discovered = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let mut actions = VecDeque::new();
    let mut owner_messages = VecDeque::new();

    handle_completion(
        OwnerCompletion::Rescan {
            config_generation: ConfigGeneration(1),
            inventory_generation: InventoryGeneration(1),
            lifecycle_generation: 0,
            resume_reconciliation: None,
            hw: Box::new(discovered),
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

    assert!(!state.hw.has_nvidia);
    assert!(owner_messages.is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn runtime_shutdown_timeout_abandons_blocking_work_within_budget() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");
    runtime.spawn_blocking(|| std::thread::sleep(Duration::from_millis(100)));
    runtime.block_on(tokio::task::yield_now());
    let started = Instant::now();
    runtime.shutdown_timeout(Duration::from_millis(10));
    assert!(started.elapsed() < Duration::from_millis(80));
}

fn test_ticket(run_id: RunId) -> JobTicket {
    JobTicket {
        metrics: Default::default(),
        run_id,
        job: JobId::singleton(OwnerId::Cpu, JobKind::Cpu),
        config_generation: ConfigGeneration(1),
        inventory_generation: InventoryGeneration(1),
        history_deadline: None,
    }
}

fn test_paths(label: &str) -> (PathBuf, DaemonPaths) {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-async-{label}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let runtime = root.join("run/plasma-top");
    let state = runtime.join("state");
    (
        root.clone(),
        DaemonPaths {
            runtime: runtime.clone(),
            state: state.clone(),
            panel: runtime.join("panel.html"),
            tooltip: runtime.join("tooltip.html"),
            page: state.join("page"),
            npages: state.join("npages"),
            geom: state.join("geom"),
            plasma_config: root.join("appletsrc"),
            kdeglobals: root.join("kdeglobals"),
        },
    )
}

#[test]
fn queued_partial_invalidations_accumulate_and_full_invalidation_wins() {
    use crate::domain::Metric;
    let job = JobId::with_source(
        OwnerId::AmdGpu,
        JobKind::AmdFast,
        SourceIdentity::Device("amd:0000:c3:00.0".into()),
    );
    let mut pending = VecDeque::new();
    for metric in [Metric::GpuAmdUsage, Metric::GpuAmdCodecUsage] {
        assert!(queue_owner_message(
            &mut pending,
            (
                job.owner,
                OwnerMessage::Invalidate(job.clone(), Some([metric].into()))
            )
        ));
    }
    assert_eq!(pending.len(), 1);
    assert!(
        matches!(&pending[0].1, OwnerMessage::Invalidate(_, Some(metrics)) if *metrics == [Metric::GpuAmdUsage, Metric::GpuAmdCodecUsage].into())
    );
    assert!(queue_owner_message(
        &mut pending,
        (job.owner, OwnerMessage::Invalidate(job.clone(), None))
    ));
    assert!(queue_owner_message(
        &mut pending,
        (
            job.owner,
            OwnerMessage::Invalidate(job.clone(), Some([Metric::GpuAmdFreq].into()))
        )
    ));
    assert_eq!(pending.len(), 1);
    assert!(matches!(&pending[0].1, OwnerMessage::Invalidate(_, None)));
}
