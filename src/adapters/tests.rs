use super::*;

#[test]
fn command_runner_returns_timeout_without_waiting_for_child_exit() {
    let mut runner = ProductionCommandRunner;
    let result = runner.run(
        Path::new("/bin/sh"),
        &[OsString::from("-c"), OsString::from("sleep 1")],
        Duration::from_millis(10),
    );

    let error = match result {
        Ok(_) => panic!("sleeping command must time out"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("timed out after 0.010s"));
}

#[test]
fn exact_busctl_get_all_json_normalizes_to_interleaved_pairs() {
    let value = parse_json(
        r#"{"type":"a{sv}","data":[{"Percentage":{"type":"d","data":52.5},"State":{"type":"u","data":2}}]}"#,
    );
    assert_eq!(
        normalized("GetAll", &value),
        vec!["Percentage", "52.5", "State", "2"]
    );
}

#[test]
fn exact_busctl_enumerate_devices_json_preserves_array_cardinality() {
    let cases = [
        (r#"{"type":"ao","data":[[]]}"#, Vec::<&str>::new()),
        (
            r#"{"type":"ao","data":[["/org/freedesktop/UPower/devices/battery_BAT0"]]}"#,
            vec!["/org/freedesktop/UPower/devices/battery_BAT0"],
        ),
        (
            r#"{"type":"ao","data":[["/device/one","/device/two"]]}"#,
            vec!["/device/one", "/device/two"],
        ),
    ];

    for (json, expected) in cases {
        let value = parse_json(json);
        assert_eq!(normalized("EnumerateDevices", &value), expected);
    }
}

#[test]
fn exact_busctl_managed_objects_json_preserves_drive_relation() {
    let value = parse_json(
        r#"{"type":"a{oa{sa{sv}}}","data":[{"/block":{"org.freedesktop.UDisks2.Block":{"Drive":{"type":"o","data":"/drive"}}}}]}"#,
    );
    assert_eq!(
        normalized("GetManagedObjects", &value),
        vec![
            "/block",
            "org.freedesktop.UDisks2.Block",
            "Block.Drive=/drive",
            ""
        ]
    );
}

#[test]
fn exact_busctl_get_json_preserves_warning_array_cardinality() {
    let cases = [
        (r#"{"type":"v","data":[{"type":"as","data":[]}]}"#, "[]"),
        (
            r#"{"type":"v","data":[{"type":"as","data":["spare"]}]}"#,
            r#"["spare"]"#,
        ),
        (
            r#"{"type":"v","data":[{"type":"as","data":["spare","temperature"]}]}"#,
            r#"["spare","temperature"]"#,
        ),
    ];

    for (json, expected) in cases {
        let value = parse_json(json);
        assert_eq!(normalized("Get", &value), vec![expected]);
    }
}

#[test]
fn malformed_typed_dbus_replies_return_contextual_boundary_errors() {
    let cases = [
        (
            "EnumerateDevices",
            serde_json::json!({}),
            "must contain a string `type` signature",
        ),
        (
            "EnumerateDevices",
            serde_json::json!({"type": "ao", "data": null}),
            "must contain a `data` array",
        ),
        (
            "EnumerateDevices",
            serde_json::json!({"type": "ao", "data": [["/device", 2]]}),
            "does not match D-Bus signature `o`",
        ),
        (
            "EnumerateDevices",
            serde_json::json!({"type": "as", "data": [["/device"]]}),
            "unexpected busctl reply signature",
        ),
        (
            "GetManagedObjects",
            serde_json::json!({"type": "a{sv}", "data": [{}]}),
            "unexpected busctl reply signature",
        ),
        (
            "GetManagedObjects",
            serde_json::json!({"type": "a{oa{sa{sv}}}", "data": [{"/block": []}]}),
            "must be an object",
        ),
        (
            "GetManagedObjects",
            serde_json::json!({"type": "a{oa{sa{sv}}}", "data": [{"/block": {"org.freedesktop.UDisks2.Block": {"Drive": {"type": "o", "data": 7}}}}]}),
            "does not match D-Bus signature `o`",
        ),
        (
            "GetAll",
            serde_json::json!({"type": "a{sv}", "data": [[]]}),
            "must be an object",
        ),
        (
            "GetAll",
            serde_json::json!({"type": "a{sv}", "data": [{"Model": {"type": "s", "data": false}}]}),
            "does not match D-Bus signature `s`",
        ),
        (
            "Get",
            serde_json::json!({"type": "v", "data": []}),
            "must contain exactly one value",
        ),
        (
            "Get",
            serde_json::json!({"type": "v", "data": [{"type": "as", "data": null}]}),
            "must be an array for signature `as`",
        ),
    ];

    for (member, value, expected_detail) in cases {
        let Err(error) = normalize_dbus_body(&request(member), &value) else {
            panic!("malformed reply was accepted");
        };
        let BoundaryError::DbusCallFailed {
            service,
            path,
            interface,
            member: failed_member,
            detail,
            ..
        } = error
        else {
            panic!("expected D-Bus boundary error");
        };
        assert_eq!(service, "org.example.Service");
        assert_eq!(path, "/org/example/Object");
        assert_eq!(interface, "org.example.Interface");
        assert_eq!(failed_member, member);
        assert!(detail.contains(expected_detail), "{detail}");
    }
}

fn parse_json(json: &str) -> Value {
    match serde_json::from_str(json) {
        Ok(value) => value,
        Err(error) => panic!("invalid test JSON: {error}"),
    }
}

fn normalized(member: &str, value: &Value) -> Vec<String> {
    match normalize_dbus_body(&request(member), value) {
        Ok(body) => body,
        Err(error) => panic!("valid {member} reply failed: {error}"),
    }
}

fn request(member: &str) -> DbusRequest {
    DbusRequest {
        bus: BusKind::System,
        service: String::from("org.example.Service"),
        object_path: String::from("/org/example/Object"),
        interface: String::from("org.example.Interface"),
        member: member.to_owned(),
        arguments: Vec::new(),
        timeout: None,
    }
}
