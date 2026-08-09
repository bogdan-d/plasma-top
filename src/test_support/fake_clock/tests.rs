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
