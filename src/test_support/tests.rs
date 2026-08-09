use super::*;

#[test]
fn boundary_error_command_not_queued_displays_program_and_arg_count() {
    let err = BoundaryError::CommandNotQueued {
        program: std::path::PathBuf::from("/bin/false"),
        args: vec![
            std::ffi::OsString::from("--flag"),
            std::ffi::OsString::from("value"),
        ],
    };

    let msg = format!("{err}");
    assert!(msg.contains("/bin/false"), "message includes program path");
    assert!(msg.contains("2 arg"), "message includes arg count");
}

#[test]
fn boundary_error_dbus_not_queued_displays_full_signature() {
    let err = BoundaryError::DbusCallNotQueued {
        bus: BusKind::System,
        service: "org.freedesktop.UPower".to_owned(),
        path: "/org/freedesktop/UPower".to_owned(),
        interface: "org.freedesktop.UPower".to_owned(),
        member: "EnumerateDevices".to_owned(),
    };

    let msg = format!("{err}");
    assert!(msg.contains("system"), "message includes bus label");
    assert!(msg.contains("org.freedesktop.UPower"));
    assert!(msg.contains("/org/freedesktop/UPower"));
    assert!(msg.contains("EnumerateDevices"));
}

#[test]
fn re_exports_are_visible_at_module_root() {
    // Compile-time check that every documented re-export is reachable
    // from the crate root as `plasma_top::test_support::*`. The actual
    // behavior is exercised by the submodule test suites; this just
    // guards against accidental visibility regressions.
    fn _check(
        _clock: FakeClock,
        _runner: FakeCommandRunner,
        _dbus: FakeDbus,
        _notifications: FakeNotificationFacade,
        _loader: FixtureLoader,
        _root: FixtureRoot,
    ) {
    }

    _check(
        FakeClock::default(),
        FakeCommandRunner::new(),
        FakeDbus::new(),
        FakeNotificationFacade::new(),
        FixtureLoader::default(),
        FixtureRoot::default(),
    );
}
