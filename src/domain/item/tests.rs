use super::*;
use crate::domain::form::Surface;

#[test]
fn bare_metric_defaults_to_value_form() {
    let token = "cpu_usage".parse::<ItemToken>();

    assert_eq!(
        token,
        Ok(ItemToken {
            metric: Metric::CpuUsage,
            rendering: ItemRendering::Generic(Form::Value),
        })
    );
    assert_eq!(
        token.map(|item| item.to_string()),
        Ok(String::from("cpu_usage"))
    );
}

#[test]
fn explicit_generic_form_round_trips() {
    let token = "cpu_usage:spark_value".parse::<ItemToken>();

    assert_eq!(
        token.map(|item| item.to_string()),
        Ok(String::from("cpu_usage:spark_value"))
    );
}

#[test]
fn intrinsic_metric_rejects_forms() {
    let token = "net_speed:value".parse::<ItemToken>();

    assert_eq!(
        token,
        Err(ItemParseError::IntrinsicMetricWithForm {
            metric: Metric::NetSpeed,
            form: String::from("value"),
        })
    );
}

#[test]
fn unsupported_form_is_rejected() {
    let token = "cpu_temp:bar".parse::<ItemToken>();

    assert_eq!(
        token,
        Err(ItemParseError::UnsupportedForm {
            metric: Metric::CpuTemp,
            form: Form::Bar,
        })
    );
}

#[test]
fn effective_surfaces_follow_metric_and_form_rules() {
    let panel_token = match "cpu_usage:bar".parse::<ItemToken>() {
        Ok(token) => token,
        Err(error) => panic!("unexpected parse error: {error}"),
    };
    let tooltip_token = match "uptime".parse::<ItemToken>() {
        Ok(token) => token,
        Err(error) => panic!("unexpected parse error: {error}"),
    };

    assert!(
        panel_token
            .effective_surfaces()
            .contains(Surface::PanelHorizontal)
    );
    assert!(!panel_token.effective_surfaces().contains(Surface::Tooltip));
    assert!(
        tooltip_token
            .effective_surfaces()
            .contains(Surface::Tooltip)
    );
}
