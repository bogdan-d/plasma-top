use super::*;

// ── numeric helpers ──────────────────────────────────────────────────────

#[test]
fn round_half_even_ratio_matches_python_bankers_rounding() {
    // 1.5 → 2 (even), 2.5 → 2 (even), 0.5 → 0 (even).
    assert_eq!(round_half_even_ratio(1_500_000, 1_000_000), 2);
    assert_eq!(round_half_even_ratio(2_500_000, 1_000_000), 2);
    assert_eq!(round_half_even_ratio(500_000, 1_000_000), 0);
    // Not-at-half rounds normally.
    assert_eq!(round_half_even_ratio(1_600_000, 1_000_000), 2);
    assert_eq!(round_half_even_ratio(1_400_000, 1_000_000), 1);
}

#[test]
fn round_half_even_f64_handles_halfway_and_non_finite() {
    assert_eq!(round_half_even_f64(15.5), 16);
    assert_eq!(round_half_even_f64(14.5), 14);
    assert_eq!(round_half_even_f64(0.5), 0);
    assert_eq!(round_half_even_f64(15.0), 15);
    assert_eq!(round_half_even_f64(15.4), 15);
    assert_eq!(round_half_even_f64(15.6), 16);
    assert_eq!(round_half_even_f64(f64::NAN), 0);
    assert_eq!(round_half_even_f64(f64::INFINITY), 0);
}

#[test]
fn bat_name_from_id_extracts_power_supply_name() {
    assert_eq!(
        bat_name_from_id("/org/freedesktop/UPower/devices/battery_BAT0"),
        "BAT0",
    );
    assert_eq!(bat_name_from_id("BAT0"), "BAT0");
    assert_eq!(bat_name_from_id("battery_BAT1"), "BAT1");
}
