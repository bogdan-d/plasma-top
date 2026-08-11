#![allow(clippy::expect_used)]

use super::*;

#[test]
fn initial_system_bus_readiness_reports_the_first_connection_outcome() {
    for (connected, expected) in [
        (true, InitialSystemBusAttempt::Connected),
        (false, InitialSystemBusAttempt::Unavailable),
    ] {
        let (mut sender, readiness) = initial_system_bus_readiness();
        sender.complete(connected);
        sender.complete(!connected);

        assert_eq!(
            readiness
                .receiver
                .recv_timeout(Duration::from_millis(10))
                .expect("initial readiness"),
            expected
        );
    }
}

#[test]
fn initial_system_bus_readiness_waits_for_the_bounded_connection_attempt() {
    let (mut sender, readiness) = initial_system_bus_readiness();
    let started = Instant::now();
    let attempt = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        sender.complete(false);
    });

    readiness.wait();

    attempt.join().expect("connection attempt");
    assert!(started.elapsed() >= Duration::from_millis(20));
}

#[test]
fn reconnect_backoff_is_serial_bounded_and_resets_after_connection() {
    let mut state = ReconnectBackoff::new();
    let mut observed = Vec::new();
    for _ in 0..8 {
        let before = Instant::now();
        state.failed();
        observed.push(state.retry_at.saturating_duration_since(before));
        assert!(
            !state.is_due(),
            "another reconnect must not start immediately"
        );
        state.retry_at = Instant::now();
    }
    let expected = [100, 200, 400, 800, 1_600, 2_000, 2_000, 2_000];
    for (actual, milliseconds) in observed.into_iter().zip(expected) {
        assert!(actual >= Duration::from_millis(milliseconds));
        assert!(actual < Duration::from_millis(milliseconds + 25));
    }

    state.connected();
    state.failed();
    assert!(state.wait() <= Duration::from_millis(100));
}

#[test]
fn every_typed_request_has_stable_nonempty_error_metadata() {
    let requests = [
        DbusRequest::UpowerEnumerate,
        DbusRequest::UpowerDeviceProperties {
            object_path: "/device".to_owned(),
        },
        DbusRequest::UdisksManagedObjects,
        DbusRequest::UdisksSmartUpdate {
            object_path: "/drive".to_owned(),
            kind: UdisksSmartKind::Nvme,
            timeout: Duration::from_secs(15),
        },
        DbusRequest::UdisksSmartProperty {
            object_path: "/drive".to_owned(),
            kind: UdisksSmartKind::Ata,
        },
    ];
    for request in requests {
        let (_, service, path, interface, member) = request.metadata();
        assert!(!service.is_empty());
        assert!(!path.is_empty());
        assert!(!interface.is_empty());
        assert!(!member.is_empty());
    }
}

#[test]
fn requests_are_observed_promptly_while_one_reconnect_is_pending() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let (sender, mut receiver) = mpsc::channel(1);
        let (reply, _response) = oneshot::channel();
        sender
            .send(DbusEnvelope {
                request: DbusRequest::UpowerEnumerate,
                reply,
            })
            .await
            .expect("queued request");
        let mut calls = JoinSet::new();
        let mut connecting = Some(tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            None
        }));
        let mut signals = None;
        let (_shutdown_sender, mut shutdown) = watch::channel(false);
        let event = tokio::time::timeout(
            Duration::from_millis(50),
            wait_system_event(
                &mut receiver,
                &mut calls,
                &mut connecting,
                &mut signals,
                &mut shutdown,
                Duration::from_secs(60),
                true,
            ),
        )
        .await
        .expect("prompt disconnected request");
        assert!(matches!(event, SystemEvent::Request(_)));
        if let Some(task) = connecting {
            task.abort();
        }
    });
}

#[test]
fn ended_signal_stream_is_reported_instead_of_silently_killing_monitoring() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime");
    runtime.block_on(async {
        let (_sender, mut receiver) = mpsc::channel(1);
        let mut calls = JoinSet::new();
        let mut connecting = None;
        let mut signals = Some(tokio::spawn(async {}));
        let (_shutdown_sender, mut shutdown) = watch::channel(false);
        let event = wait_system_event(
            &mut receiver,
            &mut calls,
            &mut connecting,
            &mut signals,
            &mut shutdown,
            Duration::from_secs(60),
            true,
        )
        .await;
        assert!(matches!(event, SystemEvent::SignalsEnded));
    });
}

#[test]
fn stale_disconnect_completion_does_not_match_a_new_connection() {
    let old = ConnectionGeneration(4);
    let current = ConnectionGeneration(5);

    assert!(!connection_matches(Some(current), old));
    assert!(connection_matches(Some(current), current));
}
