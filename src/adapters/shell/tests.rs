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
