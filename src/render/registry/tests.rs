#![allow(clippy::expect_used)]

use super::*;
use crate::config::BatteryConfig;
use crate::config::Config;
use crate::domain::ItemRendering;

fn bare_hw() -> HardwareInventory {
    HardwareInventory {
        net_device: Some(String::from("enp0s3")),
        disk_io_device: Some(String::from("sda")),
        cpu_count: 2,
        ..HardwareInventory::default()
    }
}

#[test]
fn bar_form_css_token_depends_on_orientation() {
    assert_eq!(form_token(Some(Form::Bar), true), Some("bar"));
    assert_eq!(form_token(Some(Form::Bar), false), Some("column"));
}

#[test]
fn resolve_item_parses_valid_tokens() {
    let cpu = resolve_item("cpu_usage:spark_value", false).expect("cpu token");
    let net = resolve_item("net_speed", false).expect("intrinsic token");

    assert_eq!(cpu.form_token, Some("spark_value"));
    assert!(matches!(
        cpu.token.rendering(),
        ItemRendering::Generic(Form::SparkValue)
    ));
    assert_eq!(net.form_token, None);
}

#[test]
fn item_gates_match_python_rules() {
    let mut cfg = Config::default();
    let hw = bare_hw();
    let readings = DisplaySnapshot::default();

    assert!(!item_gate(
        &cfg,
        &hw,
        &ItemToken::from_str("cpu_temp").expect("token"),
        &readings,
    ));
    assert!(item_gate(
        &cfg,
        &hw,
        &ItemToken::from_str("net_speed").expect("token"),
        &readings,
    ));

    cfg.battery = BatteryConfig {
        kbd_bolt: Some(1),
        ..BatteryConfig::default()
    };
    assert!(item_gate(
        &cfg,
        &hw,
        &ItemToken::from_str("battery_kbd").expect("token"),
        &readings,
    ));
}
