use super::*;

#[test]
fn upower_enumerate_returns_typed_paths_in_order() {
    let mut dbus = FakeDbus::new();
    dbus.enqueue_request(
        DbusRequest::UpowerEnumerate,
        DbusOutput::UpowerDevices(vec!["/battery_BAT0".to_owned(), "/battery_BAT1".to_owned()]),
    );

    assert_eq!(
        upower_enumerate(&mut dbus).expect("enumeration"),
        ["/battery_BAT0", "/battery_BAT1"]
    );
    assert_eq!(dbus.call_trace(), &[DbusRequest::UpowerEnumerate]);
}

#[test]
fn upower_enumerate_accepts_confirmed_empty_and_reports_failure() {
    let mut empty = FakeDbus::new();
    empty.enqueue_request(
        DbusRequest::UpowerEnumerate,
        DbusOutput::UpowerDevices(Vec::new()),
    );
    assert!(
        upower_enumerate(&mut empty)
            .expect("empty enumeration")
            .is_empty()
    );
    assert!(upower_enumerate(&mut FakeDbus::new()).is_err());
}

#[test]
fn upower_device_properties_are_not_flattened() {
    let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";
    let expected = UpowerDeviceProperties {
        percentage: Some(75.5),
        state: Some(2),
        energy_rate: Some(4.25),
        model: Some("MX Master".to_owned()),
        kind: Some(5),
    };
    let mut dbus = FakeDbus::new();
    dbus.enqueue_request(
        DbusRequest::UpowerDeviceProperties {
            object_path: path.to_owned(),
        },
        DbusOutput::UpowerDeviceProperties(expected.clone()),
    );

    assert_eq!(
        upower_device_props_result(&mut dbus, path).expect("properties"),
        expected
    );
}

#[test]
fn wrong_typed_reply_is_a_boundary_failure() {
    let path = "/device/one";
    let mut dbus = FakeDbus::new();
    dbus.enqueue_request(
        DbusRequest::UpowerDeviceProperties {
            object_path: path.to_owned(),
        },
        DbusOutput::UpowerDevices(Vec::new()),
    );

    assert!(matches!(
        upower_device_props_result(&mut dbus, path),
        Err(BoundaryError::DbusCallFailed { .. })
    ));
}

#[test]
fn find_battery_sys_filters_and_sorts_battery_paths() {
    let mut dbus = FakeDbus::new();
    dbus.enqueue_request(
        DbusRequest::UpowerEnumerate,
        DbusOutput::UpowerDevices(vec![
            "/org/freedesktop/UPower/devices/battery_BAT1".to_owned(),
            "/org/freedesktop/UPower/devices/battery_hidpp_mouse".to_owned(),
            "/org/freedesktop/UPower/devices/battery_BAT0".to_owned(),
        ]),
    );

    assert_eq!(
        find_battery_sys(&mut dbus).expect("enumeration"),
        [
            "/org/freedesktop/UPower/devices/battery_BAT0",
            "/org/freedesktop/UPower/devices/battery_BAT1",
        ]
    );
}
