#![allow(clippy::expect_used)]

use super::*;

#[test]
fn sleep_prepare_then_resume_are_drained_in_order() {
    let (events, mut receiver) = event_stream();
    events.prepare_for_sleep(true);
    events.prepare_for_sleep(false);

    assert_eq!(
        receiver.drain(),
        vec![
            IoEvent::PrepareForSleep(true),
            IoEvent::PrepareForSleep(false)
        ]
    );
}

#[test]
fn duplicate_sleep_signals_coalesce() {
    let (events, mut receiver) = event_stream();
    for _ in 0..8 {
        events.prepare_for_sleep(true);
    }
    for _ in 0..8 {
        events.prepare_for_sleep(false);
    }

    assert_eq!(
        receiver.drain(),
        vec![
            IoEvent::PrepareForSleep(true),
            IoEvent::PrepareForSleep(false)
        ]
    );
}

#[test]
fn repeated_sleep_cycles_keep_the_latest_reset_and_current_state() {
    let (events, mut receiver) = event_stream();
    for _ in 0..8 {
        events.prepare_for_sleep(true);
        events.prepare_for_sleep(false);
    }
    events.prepare_for_sleep(true);

    assert_eq!(
        receiver.drain(),
        vec![
            IoEvent::PrepareForSleep(true),
            IoEvent::PrepareForSleep(false),
            IoEvent::PrepareForSleep(true),
        ]
    );
}

#[test]
fn upower_refresh_can_refill_the_channel_after_a_drain() {
    let (events, mut receiver) = event_stream();
    for _ in 0..8 {
        events.upower_changed();
    }
    assert_eq!(receiver.drain(), vec![IoEvent::UpowerChanged]);

    events.upower_changed();
    assert_eq!(receiver.drain(), vec![IoEvent::UpowerChanged]);
}

#[test]
fn unfinished_shell_is_not_joined_after_the_deadline() {
    let (release, wait) = std::sync::mpsc::channel();
    let shell = thread::spawn(move || {
        let _ = wait.recv();
    });

    assert!(!join_finished_before(shell, Instant::now()));
    release.send(()).expect("release shell thread");
}

#[test]
fn startup_sets_orchestration_expectation_before_readiness() {
    let mut daemon = ProductionIo::start_inner(false, true).expect("daemon I/O shell");
    assert!(daemon.orchestration_registered.load(Ordering::Acquire));
    daemon
        .orchestration_done()
        .send(true)
        .expect("complete daemon orchestration");
    daemon.shutdown();
    assert!(!daemon.shutdown_timed_out());

    let mut diagnostics = ProductionIo::start_without_signals().expect("diagnostic I/O shell");
    assert!(!diagnostics.orchestration_registered.load(Ordering::Acquire));
    diagnostics.shutdown();
    assert!(!diagnostics.shutdown_timed_out());
}

#[test]
fn early_external_stop_keeps_runtime_alive_for_expected_orchestration() {
    let mut io = ProductionIo::start_inner(false, true).expect("daemon I/O shell");
    let runtime = io.runtime_handle();
    io.stopped.store(true, Ordering::Release);
    io.external_shutdown.send(true).expect("request early stop");
    io.shell_stop_selected
        .recv_timeout(Duration::from_secs(1))
        .expect("shell selects external stop");

    let orchestration_done = io.orchestration_done();
    let (task_finished, task_finished_receiver) = std::sync::mpsc::channel();
    runtime.spawn(async move {
        let _ = orchestration_done.send(true);
        let _ = task_finished.send(());
    });
    task_finished_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("orchestration runs after early stop");

    io.shutdown();
    assert_eq!(io.critical_failure(), None);
    assert!(!io.shutdown_timed_out());
}

#[test]
fn unexpected_command_service_exit_is_classified_as_critical() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let (_shutdown_sender, mut shutdown) = watch::channel(false);
        let mut command = tokio::spawn(async {});
        let mut system = tokio::spawn(std::future::pending::<()>());
        let mut session = tokio::spawn(std::future::pending::<()>());
        assert_eq!(
            wait_for_shell_stop(&mut shutdown, &mut command, &mut system, &mut session).await,
            Some(CriticalService::Command)
        );
        system.abort();
        session.abort();
    });
}

async fn assert_shell_stop_result(ready: [bool; 3], expected: Option<CriticalService>) {
    let (external_stop_sender, mut external_stop) = watch::channel(false);
    let mut command = tokio::spawn(async move {
        if !ready[0] {
            std::future::pending::<()>().await;
        }
    });
    let mut system = tokio::spawn(async move {
        if !ready[1] {
            std::future::pending::<()>().await;
        }
    });
    let mut session = tokio::spawn(async move {
        if !ready[2] {
            std::future::pending::<()>().await;
        }
    });
    while [
        command.is_finished(),
        system.is_finished(),
        session.is_finished(),
    ] != ready
    {
        tokio::task::yield_now().await;
    }
    external_stop_sender.send(true).expect("request shutdown");

    assert_eq!(
        wait_for_shell_stop(&mut external_stop, &mut command, &mut system, &mut session,).await,
        expected
    );

    command.abort();
    system.abort();
    session.abort();
}

#[test]
fn critical_service_priority_precedes_simultaneous_shutdown() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        assert_shell_stop_result([true, true, true], Some(CriticalService::Command)).await;
        assert_shell_stop_result([false, true, true], Some(CriticalService::SystemDbus)).await;
        assert_shell_stop_result([false, false, true], Some(CriticalService::SessionDbus)).await;
        assert_shell_stop_result([false, false, false], None).await;
    });
}

#[test]
fn external_stop_is_selected_before_services_receive_shutdown() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let (external_stop_sender, mut external_stop) = watch::channel(false);
        let (service_shutdown_sender, service_shutdown) = watch::channel(false);
        let mut command = tokio::spawn(wait_for_test_shutdown(service_shutdown.clone()));
        let mut system = tokio::spawn(wait_for_test_shutdown(service_shutdown.clone()));
        let mut session = tokio::spawn(wait_for_test_shutdown(service_shutdown));

        external_stop_sender.send(true).expect("request stop");
        assert_eq!(
            wait_for_shell_stop(&mut external_stop, &mut command, &mut system, &mut session,).await,
            None
        );
        assert!(!command.is_finished());
        assert!(!system.is_finished());
        assert!(!session.is_finished());

        service_shutdown_sender
            .send(true)
            .expect("broadcast service shutdown");
        command.await.expect("command service stops");
        system.await.expect("system service stops");
        session.await.expect("session service stops");
    });
}

async fn wait_for_test_shutdown(mut shutdown: watch::Receiver<bool>) {
    while !*shutdown.borrow() {
        shutdown.changed().await.expect("shutdown sender remains");
    }
}
