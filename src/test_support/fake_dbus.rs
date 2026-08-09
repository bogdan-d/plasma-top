//! D-Bus facade boundary and its in-memory fake.
//!
//! [`DbusFacade`] is implemented by the production `busctl` adapter and this
//! in-memory fake. The trait captures only the shared call shape so domain logic stays
//! testable without a live session or system bus.

use std::collections::{HashMap, VecDeque};

use crate::domain::boundary::{BoundaryError, BusKind, DbusFacade, DbusOutput, DbusRequest};

/// `(bus, service, object_path, interface, member)` signature of a D-Bus call.
///
/// Public so differential tests can hold a `&[DbusCall]` slice returned by
/// [`FakeDbus::call_trace`] without unpacking the tuple on every assertion.
pub type DbusCall = DbusRequest;

type DbusSignature = (BusKind, String, String, String, String);

/// In-memory `DbusFacade` fake keyed by `(bus, service, path, iface, member)`.
///
/// Each enqueued reply is popped FIFO when the matching call signature is
/// invoked, so repeated calls to the same method return distinct replies in
/// registration order. Every invocation is appended to a call trace for
/// differential assertions against the Python oracle.
#[derive(Debug, Default)]
pub struct FakeDbus {
    /// Signature-keyed FIFO of pending replies.
    outputs: HashMap<DbusSignature, VecDeque<DbusOutput>>,
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
        let key: DbusSignature = (
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

    /// Calls the facade without arguments or a custom timeout.
    ///
    /// This convenience keeps generic fake tests compact; production sensor
    /// tests call through [`DbusFacade`] and assert the full request trace.
    pub fn call(
        &mut self,
        bus: BusKind,
        service: &str,
        path: &str,
        iface: &str,
        member: &str,
    ) -> Result<DbusOutput, BoundaryError> {
        DbusFacade::call(
            self,
            DbusRequest {
                bus,
                service: service.to_owned(),
                object_path: path.to_owned(),
                interface: iface.to_owned(),
                member: member.to_owned(),
                arguments: Vec::new(),
                timeout: None,
            },
        )
    }
}

impl DbusFacade for FakeDbus {
    fn call(&mut self, request: DbusRequest) -> Result<DbusOutput, BoundaryError> {
        let key: DbusSignature = (
            request.bus,
            request.service.clone(),
            request.object_path.clone(),
            request.interface.clone(),
            request.member.clone(),
        );
        self.call_trace.push(request);
        match self.outputs.get_mut(&key).and_then(VecDeque::pop_front) {
            Some(output) => Ok(output),
            None => Err(BoundaryError::DbusCallNotQueued {
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
mod tests;
