use super::*;

#[test]
fn cadence_deserializes_at_minimum() {
    let cadence: Cadence = match toml::Value::Float(0.1).try_into() {
        Ok(cadence) => cadence,
        Err(error) => panic!("minimum cadence rejected: {error}"),
    };

    assert_eq!(cadence.duration(), MIN_CADENCE);
}

#[test]
fn cadence_rejects_non_finite_and_short_values() {
    for value in ["nan", "inf", "-inf", "0.099", "-1.0", "1e300"] {
        assert!(
            toml::from_str::<Cadence>(value).is_err(),
            "accepted {value}"
        );
    }
}
