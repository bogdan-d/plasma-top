use super::*;

// ── parse helpers ────────────────────────────────────────────────────────

#[test]
fn parse_object_paths_skips_empty_strings() {
    let paths = parse_object_paths(&["/a".to_owned(), String::new(), "/b".to_owned()]);

    assert_eq!(paths, ["/a", "/b"]);
}

#[test]
fn parse_property_map_decodes_interleaved_pairs() {
    let map = parse_property_map(&[
        "Percentage".to_owned(),
        "85".to_owned(),
        "State".to_owned(),
        "2".to_owned(),
    ]);

    assert_eq!(map.get("Percentage").map(String::as_str), Some("85"));
    assert_eq!(map.get("State").map(String::as_str), Some("2"));
}

#[test]
fn parse_property_map_ignores_stray_trailing_key() {
    let map = parse_property_map(&["Orphan".to_owned()]);

    assert!(map.is_empty());
}

#[test]
fn parse_managed_objects_splits_on_empty_strings() {
    let objects = parse_managed_objects(&[
        "/block_devices/nvme0n1".to_owned(),
        UDISKS_BLOCK.to_owned(),
        format!("{BLOCK_DRIVE_PREFIX}/drives/NVMe_1"),
        String::new(),
        "/drives/NVMe_1".to_owned(),
        UDISKS_NVME.to_owned(),
    ]);

    assert_eq!(objects.len(), 2);
    assert!(objects[0].is_block);
    assert_eq!(objects[0].drive_path.as_deref(), Some("/drives/NVMe_1"));
    assert!(objects[1].has_nvme);
}

// ── upower_enumerate ─────────────────────────────────────────────────────

#[test]
fn upower_enumerate_returns_paths_on_success() {
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        upath(
            "EnumerateDevices",
            vec!["/battery_BAT0".to_owned(), "/battery_BAT1".to_owned()],
        ),
    );

    let paths = upower_enumerate(&mut dbus).expect("enumeration");

    assert_eq!(
        paths,
        ["/battery_BAT0".to_owned(), "/battery_BAT1".to_owned()]
    );
}

#[test]
fn upower_enumerate_accepts_valid_empty_single_and_multiple_busctl_arrays() {
    let cases = [
        (serde_json::json!({"type": "ao", "data": [[]]}), vec![]),
        (
            serde_json::json!({"type": "ao", "data": [["/device/one"]]}),
            vec![String::from("/device/one")],
        ),
        (
            serde_json::json!({"type": "ao", "data": [["/device/one", "/device/two"]]}),
            vec![String::from("/device/one"), String::from("/device/two")],
        ),
    ];

    for (reply, expected) in cases {
        let mut dbus = RawJsonDbus::new([reply]);
        assert_eq!(upower_enumerate(&mut dbus).expect("enumeration"), expected);
    }
}

#[test]
fn upower_enumerate_rejects_malformed_busctl_arrays() {
    let replies = [
        serde_json::json!({}),
        serde_json::json!({"type": "ao", "data": null}),
        serde_json::json!({"type": "ao", "data": [["/device", false]]}),
        serde_json::json!({"type": "as", "data": [["/device"]]}),
    ];

    for reply in replies {
        let mut dbus = RawJsonDbus::new([reply]);
        let error = upower_enumerate(&mut dbus).expect_err("malformed reply");
        assert!(matches!(error, BoundaryError::DbusCallFailed { .. }));
    }
}

#[test]
fn upower_properties_reject_malformed_inner_variant() {
    let mut dbus = RawJsonDbus::new([serde_json::json!({
        "type": "a{sv}",
        "data": [{"Model": {"type": "s", "data": null}}]
    })]);

    let error = upower_device_props_result(&mut dbus, "/device/one")
        .expect_err("malformed property variant");

    assert!(matches!(error, BoundaryError::DbusCallFailed { .. }));
}

#[test]
fn upower_enumerate_reports_boundary_failure() {
    let mut dbus = FakeDbus::new();

    assert!(upower_enumerate(&mut dbus).is_err());
}

// ── find_battery_sys ─────────────────────────────────────────────────────

#[test]
fn find_battery_sys_filters_and_sorts_battery_paths() {
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        UPOWER_PATH,
        UPOWER_IFACE,
        "EnumerateDevices",
        upath(
            "EnumerateDevices",
            vec![
                "/org/freedesktop/UPower/devices/battery_BAT1".to_owned(),
                "/org/freedesktop/UPower/devices/battery_hidpp_mouse".to_owned(),
                "/org/freedesktop/UPower/devices/battery_BAT0".to_owned(),
            ],
        ),
    );

    let batteries = find_battery_sys(&mut dbus).expect("enumeration");

    assert_eq!(
        batteries,
        [
            "/org/freedesktop/UPower/devices/battery_BAT0".to_owned(),
            "/org/freedesktop/UPower/devices/battery_BAT1".to_owned(),
        ]
    );
}
