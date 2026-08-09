#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("plasma-top-hid-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Default)]
struct FakeDevice {
    writes: Vec<Vec<u8>>,
    reads: VecDeque<Result<Vec<u8>, io::ErrorKind>>,
    write_error: Option<io::ErrorKind>,
    timeouts: Vec<u16>,
}

impl FakeDevice {
    fn reply(&mut self, bytes: &[u8]) {
        self.reads.push_back(Ok(bytes.to_vec()));
    }

    fn timeout(&mut self) {
        self.reply(&[]);
    }
}

impl ReportIo for FakeDevice {
    fn write_report(&mut self, report: &[u8]) -> io::Result<usize> {
        if let Some(kind) = self.write_error {
            return Err(io::Error::from(kind));
        }
        self.writes.push(report.to_vec());
        Ok(report.len())
    }

    fn read_report_timeout(&mut self, report: &mut [u8], timeout_ms: u16) -> io::Result<usize> {
        self.timeouts.push(timeout_ms);
        match self.reads.pop_front().unwrap_or(Ok(Vec::new())) {
            Ok(bytes) => {
                report[..bytes.len()].copy_from_slice(&bytes);
                Ok(bytes.len())
            }
            Err(kind) => Err(io::Error::from(kind)),
        }
    }
}

fn response(dev_idx: u8, feature: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = vec![LONG_REPORT_ID, dev_idx, feature, SOFTWARE_ID];
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn discovery_finds_matching_product_and_control_interface() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new();
    let sys = root.0.join("sys");
    let dev = root.0.join("dev");
    let usb = sys.join("devices/pci/usb/1-2");
    let interface = usb.join("1-2:1.2");
    let hid = interface.join("0003:046D:C548.0001/hidraw/hidraw7");
    fs::create_dir_all(&hid).expect("hid hierarchy");
    fs::create_dir_all(sys.join("class/hidraw")).expect("class hierarchy");
    fs::create_dir_all(&dev).expect("dev root");
    fs::write(usb.join("idProduct"), "C548\n").expect("product");
    symlink(&hid, sys.join("class/hidraw/hidraw7")).expect("class link");
    symlink(".", hid.join("device")).expect("device link");

    assert_eq!(find_bolt_hidraw(&sys, &dev), Some(dev.join("hidraw7")));
}

#[test]
fn discovery_rejects_wrong_interface_and_malformed_tree() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new();
    let sys = root.0.join("sys");
    let usb = sys.join("devices/usb/1-2");
    let hid = usb.join("1-2:1.bad/hidraw/hidraw0");
    fs::create_dir_all(&hid).expect("hid hierarchy");
    fs::create_dir_all(sys.join("class/hidraw")).expect("class hierarchy");
    fs::write(usb.join("idProduct"), BOLT_PID).expect("product");
    symlink(&hid, sys.join("class/hidraw/hidraw0")).expect("class link");
    symlink(".", hid.join("device")).expect("device link");

    assert_eq!(find_bolt_hidraw(&sys, &root.0.join("dev")), None);
}

#[test]
fn transfer_skips_short_and_mismatched_reports() {
    let packet = [LONG_REPORT_ID, 2, 7, SOFTWARE_ID, 0];
    let mut device = FakeDevice::default();
    device.reply(&[LONG_REPORT_ID, 2, 7, SOFTWARE_ID]);
    device.reply(&response(3, 7, &[10]));
    device.reply(&response(2, 8, &[20]));
    device.reply(&response(2, 7, &[30]));

    let received = transfer(&mut device, &packet, 7).expect("transfer");

    assert_eq!(received[4], 30);
    assert_eq!(device.writes, vec![packet]);
    assert_eq!(device.timeouts, vec![TIMEOUT_MS; 4]);
}

#[test]
fn transfer_timeout_and_read_error_are_typed_failures() {
    let packet = [LONG_REPORT_ID, 1, 0, SOFTWARE_ID, 0];
    let mut timeout = FakeDevice::default();
    timeout.timeout();
    assert!(matches!(
        transfer(&mut timeout, &packet, 0),
        Err(HidError::Timeout)
    ));

    let mut failed = FakeDevice::default();
    failed.reads.push_back(Err(io::ErrorKind::PermissionDenied));
    assert!(matches!(
        transfer(&mut failed, &packet, 0),
        Err(HidError::Read(_))
    ));
}

#[test]
fn transfer_write_failure_is_an_error() {
    let mut device = FakeDevice {
        write_error: Some(io::ErrorKind::PermissionDenied),
        ..FakeDevice::default()
    };
    assert!(matches!(
        transfer(&mut device, &[LONG_REPORT_ID, 1, 0, SOFTWARE_ID], 0),
        Err(HidError::Write(_))
    ));
}

#[test]
fn feature_query_emits_exact_root_packet() {
    let mut device = FakeDevice::default();
    device.reply(&response(3, ROOT_FEATURE, &[9]));

    assert_eq!(feature_index(&mut device, 3, 0x1004).expect("feature"), 9);
    assert_eq!(
        device.writes[0],
        [LONG_REPORT_ID, 3, ROOT_FEATURE, SOFTWARE_ID, 0x10, 0x04]
            .into_iter()
            .chain([0; 14])
            .collect::<Vec<_>>()
    );
}

#[test]
fn transfer_stops_after_ten_mismatched_reports() {
    let packet = [LONG_REPORT_ID, 1, 7, SOFTWARE_ID, 0];
    let mut device = FakeDevice::default();
    for _ in 0..MAX_READS + 1 {
        device.reply(&response(2, 7, &[10]));
    }

    assert!(matches!(
        transfer(&mut device, &packet, 7),
        Err(HidError::NoMatchingResponse)
    ));
    assert_eq!(device.timeouts.len(), MAX_READS);
    assert_eq!(device.reads.len(), 1);
}

#[test]
fn absent_battery_feature_stops_after_root_query() {
    let mut device = FakeDevice::default();
    device.reply(&response(1, ROOT_FEATURE, &[0]));

    assert_eq!(battery_level(&mut device, 1).expect("unsupported"), None);
    assert_eq!(device.writes.len(), 1);
}

#[test]
fn absent_name_feature_returns_empty_name() {
    let mut device = FakeDevice::default();
    device.reply(&response(1, ROOT_FEATURE, &[0]));

    assert_eq!(device_name(&mut device, 1).expect("unsupported"), "");
    assert_eq!(device.writes.len(), 1);
}

#[test]
fn battery_query_emits_exact_function_packet_and_converts_level() {
    let mut device = FakeDevice::default();
    device.reply(&response(4, ROOT_FEATURE, &[7]));
    device.reply(&response(4, 7, &[83]));

    assert_eq!(battery_level(&mut device, 4).expect("battery"), Some(83));
    assert_eq!(
        device.writes[1],
        [LONG_REPORT_ID, 4, 7, 0x11]
            .into_iter()
            .chain([0; 16])
            .collect::<Vec<_>>()
    );
}

#[test]
fn battery_report_timeout_is_a_failure() {
    let mut device = FakeDevice::default();
    device.reply(&response(4, ROOT_FEATURE, &[7]));
    device.timeout();

    assert!(matches!(
        battery_level(&mut device, 4),
        Err(HidError::Timeout)
    ));
    assert_eq!(device.writes.len(), 2);
}

#[test]
fn name_query_decodes_ascii_replaces_invalid_bytes_and_trims() {
    let mut device = FakeDevice::default();
    device.reply(&response(1, ROOT_FEATURE, &[5]));
    device.reply(&response(1, 5, b" MX\xff Keys \0ignored"));

    assert_eq!(
        device_name(&mut device, 1).expect("name"),
        "MX\u{fffd} Keys"
    );
    assert_eq!(
        device.writes[1],
        [LONG_REPORT_ID, 1, 5, 0x11, 0]
            .into_iter()
            .chain([0; 15])
            .collect::<Vec<_>>()
    );
}

#[test]
fn combined_query_fetches_name_then_battery() {
    let mut device = FakeDevice::default();
    device.reply(&response(2, ROOT_FEATURE, &[5]));
    device.reply(&response(2, 5, b"Mouse\0"));
    device.reply(&response(2, ROOT_FEATURE, &[7]));
    device.reply(&response(2, 7, &[64]));

    assert_eq!(
        query_device(&mut device, 2, true).expect("query"),
        Some(BoltBattery {
            name: String::from("Mouse"),
            level: 64,
        })
    );
    assert_eq!(device.writes.len(), 4);
}

#[test]
fn facade_maps_timeout_read_error_and_exhausted_mismatches_to_failures() {
    let root = TempDir::new();
    let facade = BoltHidFacade::new(root.0.join("sys"), root.0.join("dev"));

    let mut device = FakeDevice::default();
    device.timeout();
    let timeout = facade
        .query_reports(&mut device, 1, false, Some(Path::new("/dev/hidraw0")))
        .expect_err("timeout");
    assert!(matches!(timeout, BoundaryError::HidFailed { .. }));
    assert!(timeout.to_string().contains("timed out"));

    let mut read_error = FakeDevice::default();
    read_error
        .reads
        .push_back(Err(io::ErrorKind::PermissionDenied));
    let read_error = facade
        .query_reports(&mut read_error, 1, false, Some(Path::new("/dev/hidraw0")))
        .expect_err("read failure");
    assert!(read_error.to_string().contains("cannot read HID report"));

    let mut mismatches = FakeDevice::default();
    for _ in 0..MAX_READS {
        mismatches.reply(&response(2, ROOT_FEATURE, &[7]));
    }
    let mismatch = facade
        .query_reports(&mut mismatches, 1, false, Some(Path::new("/dev/hidraw0")))
        .expect_err("mismatches");
    assert!(mismatch.to_string().contains("no matching HID response"));
}

#[test]
fn facade_reserves_absence_for_confirmed_unsupported_battery_feature() {
    let root = TempDir::new();
    let facade = BoltHidFacade::new(root.0.join("sys"), root.0.join("dev"));
    let mut device = FakeDevice::default();
    device.reply(&response(1, ROOT_FEATURE, &[0]));

    assert_eq!(
        facade
            .query_reports(&mut device, 1, false, Some(Path::new("/dev/hidraw0")))
            .expect("confirmed unsupported feature"),
        None
    );
}

#[test]
fn facade_reports_absent_device_and_invalid_index() {
    let root = TempDir::new();
    let mut facade = BoltHidFacade::new(root.0.join("sys"), root.0.join("dev"));

    assert!(matches!(
        facade.query(1, false),
        Err(BoundaryError::HidFailed { path: None, .. })
    ));
    assert!(matches!(
        facade.query(256, false),
        Err(BoundaryError::HidFailed { path: None, .. })
    ));
}

#[test]
fn facade_reports_open_failure_with_device_path() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new();
    let sys = root.0.join("sys");
    let dev = root.0.join("dev");
    let usb = sys.join("devices/usb/1-2");
    let interface = usb.join("1-2:1.2");
    let hid = interface.join("hid/hidraw/hidraw4");
    fs::create_dir_all(&hid).expect("hid hierarchy");
    fs::create_dir_all(sys.join("class/hidraw")).expect("class hierarchy");
    fs::create_dir_all(&dev).expect("dev root");
    fs::write(usb.join("idProduct"), BOLT_PID).expect("product");
    symlink(&hid, sys.join("class/hidraw/hidraw4")).expect("class link");
    symlink(".", hid.join("device")).expect("device link");
    let mut facade = BoltHidFacade::new(sys, dev.clone());

    let error = facade.query(1, false).expect_err("missing dev node");

    assert!(matches!(
        error,
        BoundaryError::HidFailed {
            path: Some(ref path),
            ..
        } if path == &dev.join("hidraw4")
    ));
    assert!(error.to_string().contains("hidraw4"));
}
