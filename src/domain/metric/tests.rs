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

#[test]
fn amd_tokens_have_independent_value_only_capabilities() -> Result<(), Box<dyn std::error::Error>> {
    use crate::domain::{ItemToken, list_items, needed_capabilities};
    let pairs = [
        ("gpu_amd_usage", Capability::GpuAmdUsage),
        ("gpu_amd_codec_usage", Capability::GpuAmdCodec),
        ("gpu_amd_mem_usage", Capability::GpuAmdMemory),
        ("gpu_amd_freq", Capability::GpuAmdFrequency),
        ("gpu_amd_temp", Capability::GpuAmdTemperature),
        ("gpu_amd_power", Capability::GpuAmdPower),
        ("gpu_amd_fan_speed", Capability::GpuAmdFanSpeed),
    ];
    for (name, capability) in pairs {
        let metric = name.parse::<Metric>()?;
        assert_eq!(metric.to_string(), name);
        assert_eq!(metric.capabilities(), &[capability]);
        assert_eq!(metric.spec().generic_forms, &[Form::Value]);
        assert_eq!(metric.surfaces(), SurfaceSet::ALL);
        assert!(name.parse::<ItemToken>().is_ok());
        for form in ["bar", "spark", "braille", "pair"] {
            assert!(format!("{name}:{form}").parse::<ItemToken>().is_err());
        }
        assert!(list_items().contains(&(String::from(name), "panel + tooltip")));
        assert!(
            needed_capabilities(
                std::iter::once(name.parse::<ItemToken>()?),
                std::iter::empty(),
                std::iter::empty()
            )
            .contains(&capability)
        );
    }
    Ok(())
}
