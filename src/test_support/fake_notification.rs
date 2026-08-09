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
mod tests;
