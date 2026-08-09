use super::*;

// ── detect_smart_disks ───────────────────────────────────────────────────

fn managed_objects_reply(objects: &[Vec<&str>]) -> DbusOutput {
    let mut body = Vec::new();
    for (idx, obj) in objects.iter().enumerate() {
        if idx > 0 {
            body.push(String::new());
        }
        body.extend(obj.iter().map(|s| (*s).to_owned()));
    }
    dbus_body(
        SYSTEM,
        UDISKS_NAME,
        UDISKS_PATH,
        OBJ_MANAGER_IFACE,
        "GetManagedObjects",
        body,
    )
}

fn write_rotational(sys: &TempTree, label: &str, rotational: bool) {
    sys.write(
        &format!("sys/block/{label}/queue/rotational"),
        if rotational { "1" } else { "0" },
    );
}

#[test]
fn detect_smart_disks_finds_nvme_and_ata_drives() {
    let tmp = TempTree::new();
    write_rotational(&tmp, "nvme0n1", false);
    write_rotational(&tmp, "sda", true);
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        UDISKS_PATH,
        OBJ_MANAGER_IFACE,
        "GetManagedObjects",
        managed_objects_reply(&[
            vec![
                "/org/freedesktop/UDisks2/block_devices/nvme0n1",
                UDISKS_BLOCK,
                &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/NVMe_1234"),
            ],
            vec!["/org/freedesktop/UDisks2/drives/NVMe_1234", UDISKS_NVME],
            vec![
                "/org/freedesktop/UDisks2/block_devices/sda",
                UDISKS_BLOCK,
                &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/SATA_1"),
            ],
            vec!["/org/freedesktop/UDisks2/drives/SATA_1", UDISKS_ATA],
        ]),
    );

    let disks = detect_smart_disks(&mut dbus, &tmp.sys()).expect("enumeration");

    let nvme = disks.get("nvme0n1").expect("nvme present");
    assert_eq!(
        nvme.object_path,
        "/org/freedesktop/UDisks2/drives/NVMe_1234"
    );
    assert_eq!(nvme.interface, DiskSmartInterface::Nvme);
    assert!(!nvme.rotational);
    let sata = disks.get("sda").expect("sata present");
    assert_eq!(sata.interface, DiskSmartInterface::Ata);
    assert!(sata.rotational);
}

#[test]
fn detect_smart_disks_skips_partitions() {
    let tmp = TempTree::new();
    write_rotational(&tmp, "nvme0n1", false);
    write_rotational(&tmp, "nvme0n1p1", false);
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        UDISKS_PATH,
        OBJ_MANAGER_IFACE,
        "GetManagedObjects",
        managed_objects_reply(&[
            vec![
                "/org/freedesktop/UDisks2/block_devices/nvme0n1",
                UDISKS_BLOCK,
                &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/NVMe_1"),
            ],
            vec![
                "/org/freedesktop/UDisks2/block_devices/nvme0n1p1",
                UDISKS_BLOCK,
                UDISKS_PARTITION,
                &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/NVMe_1"),
            ],
            vec!["/org/freedesktop/UDisks2/drives/NVMe_1", UDISKS_NVME],
        ]),
    );

    let disks = detect_smart_disks(&mut dbus, &tmp.sys()).expect("enumeration");

    assert_eq!(disks.len(), 1);
    assert!(disks.contains_key("nvme0n1"));
}

#[test]
fn detect_smart_disks_skips_optical_and_missing_drive_and_unsupported() {
    let tmp = TempTree::new();
    write_rotational(&tmp, "sr0", false);
    write_rotational(&tmp, "sdb", false);
    write_rotational(&tmp, "sdc", false);
    let mut dbus = FakeDbus::new();
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        UDISKS_PATH,
        OBJ_MANAGER_IFACE,
        "GetManagedObjects",
        managed_objects_reply(&[
            // optical drive — skipped by sr* prefix
            vec![
                "/org/freedesktop/UDisks2/block_devices/sr0",
                UDISKS_BLOCK,
                &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/Odd"),
            ],
            vec!["/org/freedesktop/UDisks2/drives/Odd", UDISKS_ATA],
            // block with empty drive ref — skipped
            vec![
                "/org/freedesktop/UDisks2/block_devices/sdb",
                UDISKS_BLOCK,
                &format!("{BLOCK_DRIVE_PREFIX}/"),
            ],
            // block whose drive is absent from the reply — skipped
            vec![
                "/org/freedesktop/UDisks2/block_devices/sdc",
                UDISKS_BLOCK,
                &format!("{BLOCK_DRIVE_PREFIX}/org/freedesktop/UDisks2/drives/Ghost"),
            ],
        ]),
    );

    let disks = detect_smart_disks(&mut dbus, &tmp.sys()).expect("enumeration");

    assert!(disks.is_empty(), "no drive qualifies: {disks:?}");
}

#[test]
fn detect_smart_disks_reports_boundary_failure() {
    let tmp = TempTree::new();
    let mut dbus = FakeDbus::new();

    assert!(detect_smart_disks(&mut dbus, &tmp.sys()).is_err());
}

#[test]
fn detect_smart_disks_rejects_malformed_inner_drive_variant() {
    let tmp = TempTree::new();
    let mut dbus = RawJsonDbus::new([serde_json::json!({
        "type": "a{oa{sa{sv}}}",
        "data": [{
            "/org/freedesktop/UDisks2/block_devices/nvme0n1": {
                "org.freedesktop.UDisks2.Block": {
                    "Drive": {"type": "o", "data": false}
                }
            }
        }]
    })]);

    let error =
        detect_smart_disks(&mut dbus, &tmp.sys()).expect_err("malformed managed property variant");

    assert!(matches!(error, BoundaryError::DbusCallFailed { .. }));
}

// ── read_disk_smart ──────────────────────────────────────────────────────

#[test]
fn read_disk_smart_nvme_healthy_when_warning_empty() {
    let mut dbus = FakeDbus::new();
    let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        drive,
        UDISKS_NVME,
        "SmartUpdate",
        dbus_body(
            SYSTEM,
            UDISKS_NAME,
            drive,
            UDISKS_NVME,
            "SmartUpdate",
            Vec::new(),
        ),
    );
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        drive,
        "org.freedesktop.DBus.Properties",
        "Get",
        dbus_body(
            SYSTEM,
            UDISKS_NAME,
            drive,
            "org.freedesktop.DBus.Properties",
            "Get",
            vec![String::from("[]")],
        ),
    );

    let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Nvme);

    assert_eq!(health, Some(true));
    let trace = dbus.call_trace();
    assert_eq!(trace[0].arguments, [DbusArgument::EmptyStringVariantDict]);
    assert_eq!(trace[0].timeout, Some(SMART_UPDATE_TIMEOUT));
    assert_eq!(trace[1].interface, "org.freedesktop.DBus.Properties");
    assert_eq!(trace[1].member, "Get");
    assert_eq!(
        trace[1].arguments,
        [
            DbusArgument::String(UDISKS_NVME.to_owned()),
            DbusArgument::String("SmartCriticalWarning".to_owned()),
        ]
    );
}

#[test]
fn read_disk_smart_nvme_failing_when_multiple_warnings_present() {
    let mut dbus = FakeDbus::new();
    let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        drive,
        UDISKS_NVME,
        "SmartUpdate",
        dbus_body(
            SYSTEM,
            UDISKS_NAME,
            drive,
            UDISKS_NVME,
            "SmartUpdate",
            Vec::new(),
        ),
    );
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        drive,
        "org.freedesktop.DBus.Properties",
        "Get",
        dbus_body(
            SYSTEM,
            UDISKS_NAME,
            drive,
            "org.freedesktop.DBus.Properties",
            "Get",
            vec![String::from(r#"["spare","temperature"]"#)],
        ),
    );

    let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Nvme);

    assert_eq!(health, Some(false));
}

#[test]
fn read_disk_smart_does_not_treat_null_warning_array_as_healthy() {
    let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";
    let mut dbus = RawJsonDbus::new([
        serde_json::json!({"type": "", "data": []}),
        serde_json::json!({
            "type": "v",
            "data": [{"type": "as", "data": null}]
        }),
    ]);

    let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Nvme);

    assert_eq!(health, None);
}

#[test]
fn read_disk_smart_ata_healthy_when_not_failing() {
    let mut dbus = FakeDbus::new();
    let drive = "/org/freedesktop/UDisks2/drives/SATA_1";
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        drive,
        UDISKS_ATA,
        "SmartUpdate",
        dbus_body(
            SYSTEM,
            UDISKS_NAME,
            drive,
            UDISKS_ATA,
            "SmartUpdate",
            Vec::new(),
        ),
    );
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        drive,
        "org.freedesktop.DBus.Properties",
        "Get",
        dbus_body(
            SYSTEM,
            UDISKS_NAME,
            drive,
            "org.freedesktop.DBus.Properties",
            "Get",
            vec!["false".to_owned()],
        ),
    );

    let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Ata);

    assert_eq!(health, Some(true));
}

#[test]
fn read_disk_smart_ata_failing_when_smart_failing_true() {
    let mut dbus = FakeDbus::new();
    let drive = "/org/freedesktop/UDisks2/drives/SATA_1";
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        drive,
        UDISKS_ATA,
        "SmartUpdate",
        dbus_body(
            SYSTEM,
            UDISKS_NAME,
            drive,
            UDISKS_ATA,
            "SmartUpdate",
            Vec::new(),
        ),
    );
    dbus.enqueue(
        SYSTEM,
        UDISKS_NAME,
        drive,
        "org.freedesktop.DBus.Properties",
        "Get",
        dbus_body(
            SYSTEM,
            UDISKS_NAME,
            drive,
            "org.freedesktop.DBus.Properties",
            "Get",
            vec!["true".to_owned()],
        ),
    );

    let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Ata);

    assert_eq!(health, Some(false));
}

#[test]
fn read_disk_smart_returns_none_when_smart_update_unreachable() {
    let mut dbus = FakeDbus::new();
    let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";

    // SmartUpdate fails → no property read attempted → None.
    let health = read_disk_smart(&mut dbus, drive, DiskSmartInterface::Nvme);

    assert_eq!(health, None);
}
