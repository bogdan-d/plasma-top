use super::*;

#[test]
fn filesystem_roots_default_to_host_stubs() {
    let roots = FilesystemRoots::default();

    assert_eq!(roots.runtime_root, None);
    assert_eq!(roots.cache_root, None);
    assert_eq!(roots.config_root, None);
    assert_eq!(roots.proc_root, PathBuf::from("/proc"));
    assert_eq!(roots.sys_root, PathBuf::from("/sys"));
    assert_eq!(roots.state_root(), None);
}

#[test]
fn boundary_error_messages_include_context() {
    let command = BoundaryError::CommandNotQueued {
        program: PathBuf::from("/bin/false"),
        args: vec![OsString::from("--flag")],
    };
    let dbus = BoundaryError::DbusCallFailed {
        bus: BusKind::System,
        service: "org.freedesktop.UPower".to_owned(),
        path: "/org/freedesktop/UPower".to_owned(),
        interface: "org.freedesktop.UPower".to_owned(),
        member: "EnumerateDevices".to_owned(),
        detail: "connection lost".to_owned(),
    };

    assert!(format!("{command}").contains("/bin/false"));
    assert!(format!("{dbus}").contains("connection lost"));
}
