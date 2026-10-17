use super::*;

#[test]
fn page_steps_accept_only_unit_deltas_and_wrap_both_ways() {
    assert_eq!(PageStep::from_delta(1), Some(PageStep::Next));
    assert_eq!(PageStep::from_delta(-1), Some(PageStep::Prev));
    for delta in [0, 2, -2, i32::MAX, i32::MIN] {
        assert_eq!(PageStep::from_delta(delta), None, "delta {delta}");
    }
    assert_eq!(PageStep::Next.apply(2, 3), 0);
    assert_eq!(PageStep::Prev.apply(0, 3), 2);
    assert_eq!(PageStep::Next.apply(0, 1), 0);
    assert_eq!(PageStep::Prev.apply(0, 0), 0);
}

#[test]
fn geometry_bounds_reject_degenerate_and_non_finite_values() {
    assert!(AppletGeometry::new(16_384.0, 512.0, true, 512.0).is_some());
    assert!(AppletGeometry::new(0.5, 0.5, false, 0.5).is_some());
    for (usable, glyph, tooltip) in [
        (0.0, 8.0, 8.0),
        (16_384.5, 8.0, 8.0),
        (100.0, 0.0, 8.0),
        (100.0, 512.5, 8.0),
        (100.0, 8.0, -1.0),
        (f64::NAN, 8.0, 8.0),
        (100.0, f64::INFINITY, 8.0),
        (100.0, 8.0, f64::NAN),
    ] {
        assert!(
            AppletGeometry::new(usable, glyph, true, tooltip).is_none(),
            "{usable} {glyph} {tooltip}"
        );
    }
}

#[test]
fn instance_ids_must_be_positive() {
    assert_eq!(InstanceId::new(0), None);
    assert_eq!(InstanceId::new(7).map(InstanceId::get), Some(7));
}
