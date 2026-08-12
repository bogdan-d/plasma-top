use super::*;

use crate::scheduler::{CancelReason, PublicationId, RescanKind};

#[test]
fn shutdown_preempts_pending_completion_and_stale_work() {
    let (root, paths) = test_paths("shutdown-preemption");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    let mut scheduler = Scheduler::new();
    let initial = startup(&mut scheduler, vec![job(OwnerId::Cpu, JobKind::Cpu, false)]);
    let ticket = started_ticket(&initial);
    let (decision, mut decision_result) = oneshot::channel();
    let mut pending_completion = Some((
        OwnerCompletion::Job(Box::new(worker::JobCompletion {
            ticket: ticket.clone(),
            completion: CompletionKind::Captured,
            readings: DisplaySnapshot::default(),
            notifications: DisplaySnapshot::default(),
            notification_ready: true,
            hw: HardwareInventory::default(),
            resolved_mounts: vec![String::from("/")],
            command_cache: None,
            rendered_page: None,
            style_generation: state.style_generation,
            render_generation: state.render_generation,
            decoder_outcome: None,
            gpu_history_point: None,
            decision: Some(decision),
        })),
        SchedulerTime::ZERO,
    ));
    assert!(state.can_commit(
        &scheduler,
        match pending_completion.as_ref() {
            Some((OwnerCompletion::Job(completion), _)) => completion,
            _ => panic!("pending job completion"),
        }
    ));

    state
        .notification_samples
        .insert(ticket.run_id, DisplaySnapshot::default());
    let mut actions = VecDeque::from([
        SchedulerAction::PublishDisplay {
            publication: PublicationId(99),
            reason: PublishReason::DisplayDeadline,
            display_deadline: None,
            skipped_display_deadlines: 0,
            panel: true,
            tooltip: true,
        },
        SchedulerAction::EvaluateNotifications {
            ticket: ticket.clone(),
        },
        SchedulerAction::StartJob {
            ticket: ticket.clone(),
        },
        SchedulerAction::RescanHardware {
            kind: RescanKind::Hardware,
            resume_reconciliation: None,
        },
    ]);
    let mut owner_messages =
        VecDeque::from([(OwnerId::Cpu, OwnerMessage::Reset(ticket.job.clone()))]);
    let validity = DispatchValidity::default();
    validity.insert(ticket.run_id);

    preempt_for_shutdown(
        true,
        SchedulerTime::from_duration(Duration::from_millis(1)),
        &mut scheduler,
        &mut pending_completion,
        &mut actions,
        &mut owner_messages,
    );

    assert_eq!(decision_result.try_recv(), Ok(false));
    assert!(pending_completion.is_none());
    assert!(owner_messages.is_empty());
    assert_eq!(actions.len(), 2);
    assert!(actions.iter().any(|action| matches!(
        action,
        SchedulerAction::CancelJob {
            ticket: cancelled,
            reason: CancelReason::Shutdown,
        } if cancelled == &ticket
    )));
    assert!(
        actions
            .iter()
            .any(|action| matches!(action, SchedulerAction::Terminate { component: None }))
    );

    let roots = FilesystemRoots::default();
    let clock = ProductionClock::default();
    let (notification_sender, mut notifications) = mpsc::channel(1);
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
    .expect("drain shutdown actions");

    assert!(state.terminate);
    assert_eq!(state.critical_component, None);
    assert!(!state.last_committed.contains_key(&OwnerId::Cpu));
    assert!(state.notification_samples.is_empty());
    assert!(pending_completion.is_none());
    assert!(actions.is_empty());
    assert!(owner_messages.is_empty());
    assert!(!validity.remove(ticket.run_id));
    assert!(matches!(
        notifications.try_recv(),
        Err(mpsc::error::TryRecvError::Empty)
    ));
    assert!(!paths.panel.exists());
    assert!(!paths.tooltip.exists());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn shutdown_preemption_preserves_queued_critical_failure() {
    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, vec![job(OwnerId::Cpu, JobKind::Cpu, false)]);
    let clock = ProductionClock::default();
    let mut actions = VecDeque::new();
    critical_failure("CPU owner", &mut scheduler, &clock, &mut actions);
    let mut pending_completion = None;
    let mut owner_messages = VecDeque::new();

    preempt_for_shutdown(
        true,
        now(&clock),
        &mut scheduler,
        &mut pending_completion,
        &mut actions,
        &mut owner_messages,
    );

    assert!(actions.iter().any(|action| matches!(
        action,
        SchedulerAction::CancelJob {
            reason: CancelReason::CriticalFailure,
            ..
        }
    )));
    assert!(actions.iter().any(|action| matches!(
        action,
        SchedulerAction::Terminate {
            component: Some("CPU owner")
        }
    )));
}

#[test]
fn simultaneous_critical_exits_beat_shutdown_and_shutdown_beats_completion() {
    runtime().block_on(async {
        let mut owner_tasks = JoinSet::new();
        owner_tasks.spawn(async { OwnerId::Disk });
        let mut notification_task = tokio::spawn(async {});
        let (shutdown_sender, mut shutdown) = tokio::sync::watch::channel(false);
        let (completion_sender, mut completions) = mpsc::channel(1);
        completion_sender
            .send(OwnerCompletion::Cancelled(test_ticket(RunId(1))))
            .await
            .expect("ready completion");
        shutdown_sender.send(true).expect("ready shutdown");
        tokio::task::yield_now().await;

        let LoopWake::OwnerExit(exit) = wait_for_wake(
            &mut owner_tasks,
            &mut notification_task,
            &mut shutdown,
            &mut completions,
            Duration::from_secs(1),
        )
        .await
        else {
            panic!("owner exit priority")
        };
        let mut scheduler = Scheduler::new();
        let _ = startup(&mut scheduler, Vec::new());
        let clock = ProductionClock::default();
        let mut actions = VecDeque::new();
        critical_failure(
            owner_exit_component(exit),
            &mut scheduler,
            &clock,
            &mut actions,
        );
        assert!(actions.iter().any(|action| matches!(
            action,
            SchedulerAction::Terminate {
                component: Some("disk owner")
            }
        )));

        assert!(matches!(
            wait_for_wake(
                &mut owner_tasks,
                &mut notification_task,
                &mut shutdown,
                &mut completions,
                Duration::from_secs(1),
            )
            .await,
            LoopWake::NotificationExit
        ));
        notification_task = tokio::spawn(std::future::pending());
        assert!(matches!(
            wait_for_wake(
                &mut owner_tasks,
                &mut notification_task,
                &mut shutdown,
                &mut completions,
                Duration::from_secs(1),
            )
            .await,
            LoopWake::Shutdown
        ));
        assert!(matches!(
            completions.try_recv(),
            Ok(OwnerCompletion::Cancelled(_))
        ));
    });
}
