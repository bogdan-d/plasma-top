use super::*;

fn object(path: &str, interfaces: &[&str], drive: Option<&str>) -> UdisksManagedObject {
    UdisksManagedObject {
        path: path.to_owned(),
        interfaces: interfaces.iter().map(|value| (*value).to_owned()).collect(),
        drive: drive.map(str::to_owned),
    }
}

fn managed_objects_reply(objects: Vec<UdisksManagedObject>) -> DbusOutput {
    DbusOutput::UdisksManagedObjects(objects)
}

fn write_rotational(sys: &TempTree, label: &str, rotational: bool) {
    sys.write(
        &format!("sys/block/{label}/queue/rotational"),
        if rotational { "1" } else { "0" },
    );
}

#[test]
fn detect_smart_disks_finds_typed_nvme_and_ata_drives() {
    let tmp = TempTree::new();
    write_rotational(&tmp, "nvme0n1", false);
    write_rotational(&tmp, "sda", true);
    let nvme_drive = "/org/freedesktop/UDisks2/drives/NVMe_1234";
    let ata_drive = "/org/freedesktop/UDisks2/drives/SATA_1";
    let mut dbus = FakeDbus::new();
    dbus.enqueue_request(
        DbusRequest::UdisksManagedObjects,
        managed_objects_reply(vec![
            object(
                "/org/freedesktop/UDisks2/block_devices/nvme0n1",
                &[UDISKS_BLOCK],
                Some(nvme_drive),
            ),
            object(nvme_drive, &[UDISKS_NVME], None),
            object(
                "/org/freedesktop/UDisks2/block_devices/sda",
                &[UDISKS_BLOCK],
                Some(ata_drive),
            ),
            object(ata_drive, &[UDISKS_ATA], None),
        ]),
    );

    let disks = detect_smart_disks(&mut dbus, &tmp.sys()).expect("enumeration");

    assert_eq!(disks["nvme0n1"].interface, DiskSmartInterface::Nvme);
    assert!(!disks["nvme0n1"].rotational);
    assert_eq!(disks["sda"].interface, DiskSmartInterface::Ata);
    assert!(disks["sda"].rotational);
}

#[test]
fn detect_smart_disks_skips_partitions_optical_and_missing_drives() {
    let tmp = TempTree::new();
    for label in ["nvme0n1p1", "sr0", "sdc"] {
        write_rotational(&tmp, label, false);
    }
    let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";
    let mut dbus = FakeDbus::new();
    dbus.enqueue_request(
        DbusRequest::UdisksManagedObjects,
        managed_objects_reply(vec![
            object(
                "/org/freedesktop/UDisks2/block_devices/nvme0n1p1",
                &[UDISKS_BLOCK, UDISKS_PARTITION],
                Some(drive),
            ),
            object(drive, &[UDISKS_NVME], None),
            object(
                "/org/freedesktop/UDisks2/block_devices/sr0",
                &[UDISKS_BLOCK],
                Some(drive),
            ),
            object(
                "/org/freedesktop/UDisks2/block_devices/sdc",
                &[UDISKS_BLOCK],
                Some("/missing"),
            ),
        ]),
    );

    assert!(
        detect_smart_disks(&mut dbus, &tmp.sys())
            .expect("enumeration")
            .is_empty()
    );
}

#[test]
fn detect_smart_disks_reports_boundary_failure() {
    let tmp = TempTree::new();
    assert!(detect_smart_disks(&mut FakeDbus::new(), &tmp.sys()).is_err());
}

#[test]
fn read_disk_smart_uses_typed_update_and_nvme_property() {
    let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";
    let update = DbusRequest::UdisksSmartUpdate {
        object_path: drive.to_owned(),
        kind: UdisksSmartKind::Nvme,
        timeout: SMART_UPDATE_TIMEOUT,
    };
    let property = DbusRequest::UdisksSmartProperty {
        object_path: drive.to_owned(),
        kind: UdisksSmartKind::Nvme,
    };
    for (warnings, healthy) in [(Vec::new(), true), (vec![String::from("spare")], false)] {
        let mut dbus = FakeDbus::new();
        dbus.enqueue_request(update.clone(), DbusOutput::UdisksSmartUpdated)
            .enqueue_request(
                property.clone(),
                DbusOutput::UdisksNvmeCriticalWarnings(warnings),
            );
        assert_eq!(
            read_disk_smart(&mut dbus, drive, DiskSmartInterface::Nvme),
            Some(healthy)
        );
        assert_eq!(dbus.call_trace(), &[update.clone(), property.clone()]);
    }
}

#[test]
fn read_disk_smart_uses_typed_ata_failing_property() {
    let drive = "/org/freedesktop/UDisks2/drives/SATA_1";
    for (failing, healthy) in [(false, true), (true, false)] {
        let mut dbus = FakeDbus::new();
        dbus.enqueue_request(
            DbusRequest::UdisksSmartUpdate {
                object_path: drive.to_owned(),
                kind: UdisksSmartKind::Ata,
                timeout: SMART_UPDATE_TIMEOUT,
            },
            DbusOutput::UdisksSmartUpdated,
        )
        .enqueue_request(
            DbusRequest::UdisksSmartProperty {
                object_path: drive.to_owned(),
                kind: UdisksSmartKind::Ata,
            },
            DbusOutput::UdisksAtaFailing(failing),
        );
        assert_eq!(
            read_disk_smart(&mut dbus, drive, DiskSmartInterface::Ata),
            Some(healthy)
        );
    }
}

#[test]
fn read_disk_smart_stops_after_update_failure_or_wrong_typed_reply() {
    let drive = "/org/freedesktop/UDisks2/drives/NVMe_1";
    assert_eq!(
        read_disk_smart(&mut FakeDbus::new(), drive, DiskSmartInterface::Nvme),
        None
    );
    let mut wrong = FakeDbus::new();
    wrong.enqueue_request(
        DbusRequest::UdisksSmartUpdate {
            object_path: drive.to_owned(),
            kind: UdisksSmartKind::Nvme,
            timeout: SMART_UPDATE_TIMEOUT,
        },
        DbusOutput::UpowerDevices(Vec::new()),
    );
    assert_eq!(
        read_disk_smart(&mut wrong, drive, DiskSmartInterface::Nvme),
        None
    );
}
