//! Deterministic desktop-notification facade.

use std::collections::VecDeque;

use crate::domain::boundary::{NotificationError, NotificationFacade, NotificationPayload};

/// In-memory notification facade with ordered call recording and queued results.
#[derive(Debug, Clone, Default)]
pub struct FakeNotificationFacade {
    calls: Vec<NotificationPayload>,
    results: VecDeque<Result<(), NotificationError>>,
}

impl FakeNotificationFacade {
    /// Creates an empty, successful fake.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            calls: Vec::new(),
            results: VecDeque::new(),
        }
    }

    /// Queues the result returned by the next call.
    pub fn push_result(&mut self, result: Result<(), NotificationError>) {
        self.results.push_back(result);
    }

    /// Returns calls in exact emission order.
    #[must_use]
    pub fn calls(&self) -> &[NotificationPayload] {
        &self.calls
    }
}

impl NotificationFacade for FakeNotificationFacade {
    fn send(&mut self, payload: &NotificationPayload) -> Result<(), NotificationError> {
        self.calls.push(payload.clone());
        self.results.pop_front().unwrap_or(Ok(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::boundary::{NotificationTimeout, NotificationUrgency};

    fn payload(body: &str) -> NotificationPayload {
        NotificationPayload {
            title: "PlasmaTop".to_owned(),
            body: body.to_owned(),
            icon: "dialog-error".to_owned(),
            urgency: NotificationUrgency::Critical,
            timeout: NotificationTimeout::Never,
        }
    }

    #[test]
    fn records_order_even_when_a_queued_call_fails() {
        let mut fake = FakeNotificationFacade::new();
        fake.push_result(Err(NotificationError {
            detail: "service absent".to_owned(),
        }));

        assert!(fake.send(&payload("first")).is_err());
        assert!(fake.send(&payload("second")).is_ok());
        assert_eq!(fake.calls(), &[payload("first"), payload("second")]);
    }
}
