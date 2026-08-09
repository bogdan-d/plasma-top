use super::*;
use crate::domain::form::Surface;

#[test]
fn cpu_usage_keeps_historied_forms() {
    let spec = Metric::CpuUsage.spec();

    assert!(spec.generic_forms.contains(&Form::Bar));
    assert!(spec.generic_forms.contains(&Form::SparkValue));
    assert!(!spec.generic_forms.contains(&Form::Pair));
}

#[test]
fn tooltip_only_metric_stays_out_of_panel() {
    let spec = Metric::Uptime.spec();

    assert!(spec.surfaces.contains(Surface::Tooltip));
    assert!(!spec.surfaces.contains(Surface::PanelHorizontal));
    assert!(!spec.surfaces.contains(Surface::PanelVertical));
}

#[test]
fn intrinsic_metrics_keep_shapes() {
    assert_eq!(Metric::NetSpeed.intrinsic_shape(), Some(Shape::Duo));
    assert_eq!(Metric::TopProcess.intrinsic_shape(), Some(Shape::TripleL));
    assert!(!Metric::NetSpeed.supports_form(Form::Value));
}

#[test]
fn parses_known_metric_tokens() {
    assert_eq!("cpu_usage".parse::<Metric>(), Ok(Metric::CpuUsage));
    assert_eq!("server_check".parse::<Metric>(), Ok(Metric::ServerCheck));
}
