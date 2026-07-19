//! D-Bus facade boundary and its in-memory fake.
//!
//! [`DbusFacade`] is the trait the production UPower/UDisks2/freedesktop-notify
//! adapters (Wave 4 POWER/NOTIFY lanes) and the in-memory fake both implement.
//! Each production facade is a separate trait owned by the matching lane — this
//! crate-level trait captures only the shared call shape so domain logic stays
//! testable without a live session or system bus.

use std::collections::{HashMap, VecDeque};

use crate::domain::boundary::{BusKind, DbusOutput};

use super::RuntimeError;

/// `(bus, service, object_path, interface, member)` signature of a D-Bus call.
///
/// Public so differential tests can hold a `&[DbusCall]` slice returned by
/// [`FakeDbus::call_trace`] without unpacking the tuple on every assertion.
pub type DbusCall = (BusKind, String, String, String, String);

/// Generic D-Bus call facade implemented by the production adapters (Wave 4
/// POWER/NOTIFY) and the in-memory fake used by integration tests.
///
/// The trait is intentionally narrow: it captures the decoded reply shape
/// ([`DbusOutput`]) without committing to zbus/zmnt connection types. The
/// production UPower, UDisks2, and freedesktop-notify facades are separate
/// traits owned by the matching lanes; this one captures only the shared
/// call shape so the fake can stand in for any of them.
///
/// The trait is dyn-compatible so callers may store `Box<dyn DbusFacade>`
/// when polymorphism is needed.
pub trait DbusFacade {
    /// Invokes `member` on `interface` at `object_path` exposed by `service`
    /// on `bus`, returning the decoded reply.
    ///
    /// # Errors
    ///
    /// Returns [`RuntimeError::DbusCallNotQueued`] when a fake facade is asked
    /// to dispatch a call for which no fixture reply was enqueued. The
    /// production facades return adapter-specific error variants (to be
    /// promoted into `error::Error` by the POWER/NOTIFY lanes).
    fn call(
        &mut self,
        bus: BusKind,
        service: &str,
        path: &str,
        iface: &str,
        member: &str,
    ) -> Result<DbusOutput, RuntimeError>;
}

/// In-memory `DbusFacade` fake keyed by `(bus, service, path, iface, member)`.
///
/// Each enqueued reply is popped FIFO when the matching call signature is
/// invoked, so repeated calls to the same method return distinct replies in
/// registration order. Every invocation is appended to a call trace for
/// differential assertions against the Python oracle.
#[derive(Debug, Default)]
pub struct FakeDbus {
    /// Signature-keyed FIFO of pending replies.
    outputs: HashMap<DbusCall, VecDeque<DbusOutput>>,
    /// Ordered signatures of every invocation seen by this fake.
    call_trace: Vec<DbusCall>,
}

impl FakeDbus {
    /// Creates an empty fake with no queued replies.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueues a reply for the given call signature, returning `&mut self`
    /// for chaining.
    ///
    /// Multiple [`enqueue`](Self::enqueue) calls for the same signature return
    /// in FIFO order: the first matching call gets the first reply, the second
    /// call gets the second reply, and so on.
    ///
    /// Note: this is a mutating `&mut self` builder, not a consuming one, so
    /// it is intentionally *not* marked `#[must_use]` — the side effect
    /// (recording the reply) happens regardless of whether the caller chains.
    pub fn enqueue(
        &mut self,
        bus: BusKind,
        service: impl Into<String>,
        path: impl Into<String>,
        iface: impl Into<String>,
        member: impl Into<String>,
        output: DbusOutput,
    ) -> &mut Self {
        let key = (
            bus,
            service.into(),
            path.into(),
            iface.into(),
            member.into(),
        );
        self.outputs.entry(key).or_default().push_back(output);
        self
    }

    /// Peeks the head of the call trace without consuming it.
    ///
    /// Returns `None` when the trace is empty. Useful for compact
    /// `assert_eq!`-style checks against the first recorded call; for full
    /// ordering assertions use [`call_trace`](Self::call_trace) instead.
    #[must_use]
    pub fn next_call(&self) -> Option<&DbusCall> {
        self.call_trace.first()
    }

    /// Returns the full ordered trace of call signatures seen by this fake.
    ///
    /// Each entry is `(bus, service, object_path, interface, member)` in
    /// invocation order. Differential tests compare this against the Python
    /// oracle to assert the Rust adapter issues exactly the expected calls.
    #[must_use]
    pub fn call_trace(&self) -> &[DbusCall] {
        &self.call_trace
    }
}

impl DbusFacade for FakeDbus {
    fn call(
        &mut self,
        bus: BusKind,
        service: &str,
        path: &str,
        iface: &str,
        member: &str,
    ) -> Result<DbusOutput, RuntimeError> {
        let key: DbusCall = (
            bus,
            service.to_owned(),
            path.to_owned(),
            iface.to_owned(),
            member.to_owned(),
        );
        self.call_trace.push(key.clone());
        match self.outputs.get_mut(&key).and_then(VecDeque::pop_front) {
            Some(output) => Ok(output),
            None => Err(RuntimeError::DbusCallNotQueued {
                bus: key.0,
                service: key.1,
                path: key.2,
                interface: key.3,
                member: key.4,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
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
        assert_eq!(trace[0].4, "Get");
        assert_eq!(trace[1].4, "Other");
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
        assert_eq!(head.4, "Get");
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
            RuntimeError::DbusCallNotQueued {
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
}
