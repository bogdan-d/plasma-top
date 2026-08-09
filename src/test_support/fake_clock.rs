//! Deterministic clock for rate/history/state-machine tests.
//!
//! [`FakeClock`] replaces real monotonic/wall clocks so sensor and notification tests
//! can drive history cadence and hysteresis without sleeping. The two clocks
//! advance together by a configurable step.

use std::time::Duration;

use crate::domain::boundary::ClockSnapshot;

/// Controllable clock used by rate/history/state-machine tests.
///
/// Produces a deterministic sequence of [`ClockSnapshot`] values: the wall
/// clock and the monotonic clock advance together by a configurable step so
/// sensor history cadence and notify hysteresis can be exercised without
/// sleeping. Construct with [`FakeClock::at`] to pin the starting time, then
/// call [`FakeClock::tick`] (or [`FakeClock::advance`] for explicit durations)
/// to move time forward.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FakeClock {
    /// Last snapshot handed to the code under test. Public so tests can assert
    /// exact time after a sequence of advances.
    pub now: ClockSnapshot,
    /// Step applied by [`FakeClock::tick`]. Defaults to one second. Private
    /// because callers should mutate it through [`FakeClock::set_advance_step`]
    /// so the step is always observable via [`FakeClock::advance_step`].
    advance_step: Duration,
}

impl FakeClock {
    /// Creates a clock pinned at `now` with a one-second tick step.
    #[must_use]
    pub const fn at(now: ClockSnapshot) -> Self {
        Self {
            now,
            advance_step: Duration::from_secs(1),
        }
    }

    /// Advances both clocks by `by` and returns the new snapshot.
    ///
    /// Wall-clock strategy: the wall clock advances by the same duration as
    /// the monotonic clock. This keeps the two clocks consistent under
    /// fixtures and avoids surprising payloads in notify/state-machine tests
    /// that read wall time.
    ///
    /// Both clocks saturate on overflow rather than panicking: monotonic via
    /// [`Duration::saturating_add`], wall via
    /// [`SystemTime::checked_add`](std::time::SystemTime::checked_add) falling
    /// back to the previous value. In practice neither saturates with any
    /// realistic fixture, but the saturating behavior keeps the helper safe
    /// under hostile inputs.
    pub fn advance(&mut self, by: Duration) -> ClockSnapshot {
        self.now.monotonic = self.now.monotonic.saturating_add(by);
        self.now.wall = self.now.wall.checked_add(by).unwrap_or(self.now.wall);
        self.now
    }

    /// Advances the clock by [`advance_step`](Self::advance_step) and returns
    /// the new snapshot.
    ///
    /// Equivalent to `self.advance(self.advance_step())` but does not require
    /// the caller to thread the step through.
    pub fn tick(&mut self) -> ClockSnapshot {
        let step = self.advance_step;
        self.advance(step)
    }

    /// Sets the duration applied on each subsequent [`tick`](Self::tick).
    pub fn set_advance_step(&mut self, step: Duration) {
        self.advance_step = step;
    }

    /// Returns the duration applied on each [`tick`](Self::tick).
    #[must_use]
    pub const fn advance_step(&self) -> Duration {
        self.advance_step
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::at(ClockSnapshot::default())
    }
}

#[cfg(test)]
mod tests;
