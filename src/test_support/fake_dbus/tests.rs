use super::*;

fn ok_reply(bus: BusKind, member: &str, body: &[&str]) -> DbusOutput {
    DbusOutput {
        bus,
        service: "org.example.Svc".to_owned(),
        object_path: "/org/example/Obj".to_owned(),
        interface: "org.example.Iface".to_owned(),
        member: member.to_owned(),
        body: body.iter().map(|s| (*s).to_owned()).collect(),
    }
}

const SESSION: BusKind = BusKind::Session;
const SYSTEM: BusKind = BusKind::System;

#[test]
fn new_starts_empty() {
    let facade = FakeDbus::new();

    assert!(facade.call_trace().is_empty());
    assert!(facade.next_call().is_none());
}

#[test]
fn enqueue_then_call_returns_reply() {
    let mut facade = FakeDbus::new();
    let reply = ok_reply(SESSION, "GetAll", &["80%"]);
    facade.enqueue(
        SESSION,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "GetAll",
        reply.clone(),
    );

    let got = match facade.call(
        SESSION,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "GetAll",
    ) {
        Ok(reply) => reply,
        Err(error) => panic!("enqueued reply must be returned: {error}"),
    };

    assert_eq!(got, reply);
}

#[test]
fn repeated_calls_for_same_signature_return_replies_in_fifo_order() {
    let mut facade = FakeDbus::new();
    facade.enqueue(
        SYSTEM,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "Get",
        ok_reply(SYSTEM, "Get", &["first"]),
    );
    facade.enqueue(
        SYSTEM,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "Get",
        ok_reply(SYSTEM, "Get", &["second"]),
    );

    let first = match facade.call(
        SYSTEM,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "Get",
    ) {
        Ok(reply) => reply,
        Err(error) => panic!("first: {error}"),
    };
    let second = match facade.call(
        SYSTEM,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "Get",
    ) {
        Ok(reply) => reply,
        Err(error) => panic!("second: {error}"),
    };

    assert_eq!(first.body, vec!["first".to_owned()]);
    assert_eq!(second.body, vec!["second".to_owned()]);
}

#[test]
fn call_records_signature_in_trace_regardless_of_match() {
    let mut facade = FakeDbus::new();
    facade.enqueue(
        SESSION,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "Get",
        ok_reply(SESSION, "Get", &[]),
    );

    let _ = facade.call(
        SESSION,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "Get",
    );
    // Unmatched call (no enqueue) — still recorded.
    let _ = facade.call(
        SESSION,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "Other",
    );

    let trace = facade.call_trace();
    assert_eq!(trace.len(), 2, "every invocation is recorded");
    assert_eq!(trace[0].member, "Get");
    assert_eq!(trace[1].member, "Other");
}

#[test]
fn next_call_peeks_head_of_trace() {
    let mut facade = FakeDbus::new();
    facade.enqueue(
        SESSION,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "Get",
        ok_reply(SESSION, "Get", &[]),
    );

    assert!(facade.next_call().is_none());

    let _ = facade.call(
        SESSION,
        "org.example.Svc",
        "/org/example/Obj",
        "org.example.Iface",
        "Get",
    );
    let Some(head) = facade.next_call() else {
        panic!("trace non-empty");
    };
    assert_eq!(head.member, "Get");
}

#[test]
fn call_returns_not_queued_when_no_reply_enqueued() {
    let mut facade = FakeDbus::new();

    let err = match facade.call(
        SYSTEM,
        "org.freedesktop.UPower",
        "/org/freedesktop/UPower/devices/battery_BAT0",
        "org.freedesktop.UPower.Device",
        "GetAll",
    ) {
        Ok(reply) => panic!("expected error, got {reply:?}"),
        Err(error) => error,
    };

    match err {
        BoundaryError::DbusCallNotQueued {
            bus,
            service,
            path,
            interface,
            member,
        } => {
            assert_eq!(bus, SYSTEM);
            assert_eq!(service, "org.freedesktop.UPower");
            assert_eq!(path, "/org/freedesktop/UPower/devices/battery_BAT0");
            assert_eq!(interface, "org.freedesktop.UPower.Device");
            assert_eq!(member, "GetAll");
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn enqueue_is_chainable_via_mut_self() {
    let mut facade = FakeDbus::new();
    facade
        .enqueue(
            SESSION,
            "org.example.A",
            "/a",
            "org.example.Iface",
            "Ping",
            ok_reply(SESSION, "Ping", &["a"]),
        )
        .enqueue(
            SESSION,
            "org.example.B",
            "/b",
            "org.example.Iface",
            "Ping",
            ok_reply(SESSION, "Ping", &["b"]),
        );

    let a = match facade.call(SESSION, "org.example.A", "/a", "org.example.Iface", "Ping") {
        Ok(reply) => reply,
        Err(error) => panic!("a: {error}"),
    };
    let b = match facade.call(SESSION, "org.example.B", "/b", "org.example.Iface", "Ping") {
        Ok(reply) => reply,
        Err(error) => panic!("b: {error}"),
    };
    assert_eq!(a.body, vec!["a".to_owned()]);
    assert_eq!(b.body, vec!["b".to_owned()]);
}
