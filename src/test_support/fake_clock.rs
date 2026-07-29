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
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn default_pins_at_zeroed_snapshot_with_one_second_step() {
        let clock = FakeClock::default();

        assert_eq!(clock.now.monotonic, Duration::ZERO);
        assert_eq!(clock.now.wall, UNIX_EPOCH);
        assert_eq!(clock.advance_step(), Duration::from_secs(1));
    }

    #[test]
    fn at_pins_clock_and_uses_default_step() {
        let snap = ClockSnapshot {
            monotonic: Duration::from_secs(100),
            wall: UNIX_EPOCH + Duration::from_secs(100),
        };
        let clock = FakeClock::at(snap);

        assert_eq!(clock.now, snap);
        assert_eq!(clock.advance_step(), Duration::from_secs(1));
    }

    #[test]
    fn tick_advances_both_clocks_by_step() {
        let mut clock = FakeClock::at(ClockSnapshot {
            monotonic: Duration::from_secs(10),
            wall: UNIX_EPOCH + Duration::from_secs(10),
        });

        let advanced = clock.tick();

        assert_eq!(advanced.monotonic, Duration::from_secs(11));
        assert_eq!(advanced.wall, UNIX_EPOCH + Duration::from_secs(11));
        assert_eq!(clock.now, advanced, "tick must update internal now");
    }

    #[test]
    fn set_advance_step_changes_tick_duration() {
        let mut clock = FakeClock::default();
        clock.set_advance_step(Duration::from_millis(500));

        assert_eq!(clock.advance_step(), Duration::from_millis(500));

        let advanced = clock.tick();
        assert_eq!(advanced.monotonic, Duration::from_millis(500));
    }

    #[test]
    fn advance_accepts_explicit_duration_independent_of_step() {
        let mut clock = FakeClock::default();
        clock.set_advance_step(Duration::from_secs(1));

        let advanced = clock.advance(Duration::from_secs(60));

        assert_eq!(advanced.monotonic, Duration::from_secs(60));
        assert_eq!(advanced.wall, UNIX_EPOCH + Duration::from_secs(60));
    }

    #[test]
    fn monotonic_advances_saturate_instead_of_panicking() {
        let mut clock = FakeClock::at(ClockSnapshot {
            monotonic: Duration::MAX,
            // Constructed rather than using a hypothetical SystemTime::MAX so
            // the saturating path is exercised without depending on a constant
            // the standard library does not expose.
            wall: UNIX_EPOCH + Duration::from_secs(60 * 60 * 24 * 365 * 10_000),
        });

        // Adding any positive duration saturates; no panic.
        let advanced = clock.advance(Duration::from_secs(1));
        assert_eq!(advanced.monotonic, Duration::MAX);
    }

    #[test]
    fn repeated_ticks_produce_monotonic_sequence() {
        let mut clock = FakeClock::default();

        let first = clock.tick();
        let second = clock.tick();
        let third = clock.tick();

        assert!(first.monotonic < second.monotonic);
        assert!(second.monotonic < third.monotonic);
        assert_eq!(third.monotonic, Duration::from_secs(3));
    }
}
