//! Logitech Bolt receiver discovery and HID++ 2.0 battery queries.
//!
//! Bolt uses the receiver's hidraw control interface. The protocol core is
//! isolated behind a tiny report-I/O trait so packet, timeout, and malformed
//! response behavior is deterministic in tests. Production uses ordinary file
//! I/O and `nix`'s safe `poll(2)` wrapper; this crate contains no HID `unsafe`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use crate::domain::boundary::BoundaryError;
use crate::sensors::power::{BoltBattery, BoltBatteryFacade};

const BOLT_PID: &str = "c548";
const BOLT_USB_INTERFACE: u8 = 2;
const SOFTWARE_ID: u8 = 1;
const TIMEOUT_MS: u16 = 1_000;
const MAX_READS: usize = 10;
const REPORT_LEN: usize = 20;
const LONG_REPORT_ID: u8 = 0x11;
const ROOT_FEATURE: u8 = 0x00;
const UNIFIED_BATTERY_FEATURE: u16 = 0x1004;
const DEVICE_NAME_FEATURE: u16 = 0x0005;

/// Error from Bolt receiver discovery or report I/O.
#[derive(Debug)]
pub enum HidError {
    /// No Bolt control-interface hidraw node was found.
    DeviceAbsent,
    /// The configured device index is outside the HID++ byte range.
    InvalidDeviceIndex(i32),
    /// A hidraw node could not be opened.
    Open {
        /// Path that failed to open.
        path: PathBuf,
        /// Underlying filesystem error.
        source: io::Error,
    },
    /// A report could not be written.
    Write(io::Error),
}

impl std::fmt::Display for HidError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceAbsent => formatter.write_str("Bolt receiver control interface not found"),
            Self::InvalidDeviceIndex(index) => {
                write!(formatter, "device index {index} is outside 0..=255")
            }
            Self::Open { path, source } => {
                write!(formatter, "cannot open `{}`: {source}", path.display())
            }
            Self::Write(source) => write!(formatter, "cannot write HID report: {source}"),
        }
    }
}

impl std::error::Error for HidError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } | Self::Write(source) => Some(source),
            Self::DeviceAbsent | Self::InvalidDeviceIndex(_) => None,
        }
    }
}

/// Production Bolt battery facade using `/sys/class/hidraw` and `/dev/hidraw*`.
#[derive(Debug, Clone)]
pub struct BoltHidFacade {
    sys_root: PathBuf,
    dev_root: PathBuf,
}

impl BoltHidFacade {
    /// Creates a facade rooted at the supplied sysfs and device directories.
    ///
    /// Tests pass fixture roots; production uses [`Default`].
    #[must_use]
    pub fn new(sys_root: PathBuf, dev_root: PathBuf) -> Self {
        Self { sys_root, dev_root }
    }

    fn query_inner(&self, dev_idx: i32, want_name: bool) -> Result<Option<BoltBattery>, HidError> {
        let dev_idx = u8::try_from(dev_idx).map_err(|_| HidError::InvalidDeviceIndex(dev_idx))?;
        let path =
            find_bolt_hidraw(&self.sys_root, &self.dev_root).ok_or(HidError::DeviceAbsent)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|source| HidError::Open {
                path: path.clone(),
                source,
            })?;
        let mut device = HidrawDevice(file);
        query_device(&mut device, dev_idx, want_name)
    }
}

impl Default for BoltHidFacade {
    fn default() -> Self {
        Self::new(PathBuf::from("/sys"), PathBuf::from("/dev"))
    }
}

impl BoltBatteryFacade for BoltHidFacade {
    fn query(
        &mut self,
        dev_idx: i32,
        want_name: bool,
    ) -> Result<Option<BoltBattery>, BoundaryError> {
        self.query_inner(dev_idx, want_name).map_err(|error| {
            let path = match &error {
                HidError::Open { path, .. } => Some(path.clone()),
                _ => None,
            };
            BoundaryError::HidFailed {
                path,
                detail: error.to_string(),
            }
        })
    }
}

/// Finds the Bolt receiver's HID++ control interface.
///
/// The USB product must be `c548` and the interface suffix must be `.2`. Entries
/// are sorted so multiple receivers resolve exactly like Python's `glob` walk.
#[must_use]
pub fn find_bolt_hidraw(sys_root: &Path, dev_root: &Path) -> Option<PathBuf> {
    let mut entries: Vec<_> = fs::read_dir(sys_root.join("class/hidraw"))
        .ok()?
        .flatten()
        .collect();
    entries.sort_by_key(fs::DirEntry::file_name);

    for entry in entries {
        let Ok(mut current) = fs::canonicalize(entry.path().join("device")) else {
            continue;
        };
        let mut previous: Option<PathBuf> = None;
        for _ in 0..8 {
            let product_path = current.join("idProduct");
            if product_path.exists() {
                let product_matches = fs::read_to_string(product_path)
                    .ok()
                    .is_some_and(|value| value.trim().eq_ignore_ascii_case(BOLT_PID));
                let interface_matches = previous
                    .as_deref()
                    .and_then(interface_number)
                    .is_some_and(|interface| interface == BOLT_USB_INTERFACE);
                if product_matches && interface_matches {
                    return Some(dev_root.join(entry.file_name()));
                }
                break;
            }
            previous = Some(current.clone());
            let Some(parent) = current.parent() else {
                break;
            };
            current = parent.to_path_buf();
        }
    }
    None
}

fn interface_number(path: &Path) -> Option<u8> {
    path.file_name()?.to_str()?.rsplit_once('.')?.1.parse().ok()
}

trait ReportIo {
    fn write_report(&mut self, report: &[u8]) -> io::Result<usize>;
    fn read_report_timeout(&mut self, report: &mut [u8], timeout_ms: u16) -> io::Result<usize>;
}

struct HidrawDevice(File);

impl ReportIo for HidrawDevice {
    fn write_report(&mut self, report: &[u8]) -> io::Result<usize> {
        self.0.write(report)
    }

    fn read_report_timeout(&mut self, report: &mut [u8], timeout_ms: u16) -> io::Result<usize> {
        let mut descriptors = [PollFd::new(self.0.as_fd(), PollFlags::POLLIN)];
        if poll(&mut descriptors, PollTimeout::from(timeout_ms))? == 0 {
            return Ok(0);
        }
        self.0.read(report)
    }
}

fn transfer(
    device: &mut impl ReportIo,
    packet: &[u8],
    expected_feature: u8,
) -> Result<Option<Vec<u8>>, HidError> {
    device.write_report(packet).map_err(HidError::Write)?;
    let mut buffer = [0_u8; 64];
    for _ in 0..MAX_READS {
        let read = match device.read_report_timeout(&mut buffer, TIMEOUT_MS) {
            Ok(read) => read,
            Err(_) => return Ok(None),
        };
        if read >= 5 && buffer[1] == packet[1] && buffer[2] == expected_feature {
            return Ok(Some(buffer[..read].to_vec()));
        }
        if read == 0 {
            break;
        }
    }
    Ok(None)
}

fn feature_index(device: &mut impl ReportIo, dev_idx: u8, feature_id: u16) -> Result<u8, HidError> {
    let [high, low] = feature_id.to_be_bytes();
    let mut packet = [0_u8; REPORT_LEN];
    packet[..6].copy_from_slice(&[
        LONG_REPORT_ID,
        dev_idx,
        ROOT_FEATURE,
        SOFTWARE_ID,
        high,
        low,
    ]);
    Ok(transfer(device, &packet, ROOT_FEATURE)?.map_or(0, |response| response[4]))
}

fn battery_level(device: &mut impl ReportIo, dev_idx: u8) -> Result<Option<u8>, HidError> {
    let feature = feature_index(device, dev_idx, UNIFIED_BATTERY_FEATURE)?;
    if feature == 0 {
        return Ok(None);
    }
    let mut packet = [0_u8; REPORT_LEN];
    packet[..4].copy_from_slice(&[LONG_REPORT_ID, dev_idx, feature, (1 << 4) | SOFTWARE_ID]);
    Ok(transfer(device, &packet, feature)?.map(|response| response[4]))
}

fn device_name(device: &mut impl ReportIo, dev_idx: u8) -> Result<String, HidError> {
    let feature = feature_index(device, dev_idx, DEVICE_NAME_FEATURE)?;
    if feature == 0 {
        return Ok(String::new());
    }
    let mut packet = [0_u8; REPORT_LEN];
    packet[..5].copy_from_slice(&[LONG_REPORT_ID, dev_idx, feature, (1 << 4) | SOFTWARE_ID, 0]);
    let Some(response) = transfer(device, &packet, feature)? else {
        return Ok(String::new());
    };
    let payload = &response[4..];
    let end = payload
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(payload.len());
    let name: String = payload[..end]
        .iter()
        .map(|&byte| {
            if byte.is_ascii() {
                char::from(byte)
            } else {
                char::REPLACEMENT_CHARACTER
            }
        })
        .collect();
    Ok(name.trim().to_owned())
}

fn query_device(
    device: &mut impl ReportIo,
    dev_idx: u8,
    want_name: bool,
) -> Result<Option<BoltBattery>, HidError> {
    let name = if want_name {
        device_name(device, dev_idx)?
    } else {
        String::new()
    };
    Ok(battery_level(device, dev_idx)?.map(|level| BoltBattery { name, level }))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("pirostats-hid-{}-{id}", std::process::id()));
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

        assert_eq!(received.expect("matching response")[4], 30);
        assert_eq!(device.writes, vec![packet]);
        assert_eq!(device.timeouts, vec![TIMEOUT_MS; 4]);
    }

    #[test]
    fn transfer_timeout_and_read_error_return_no_response() {
        let packet = [LONG_REPORT_ID, 1, 0, SOFTWARE_ID, 0];
        let mut timeout = FakeDevice::default();
        timeout.timeout();
        assert_eq!(transfer(&mut timeout, &packet, 0).expect("timeout"), None);

        let mut failed = FakeDevice::default();
        failed.reads.push_back(Err(io::ErrorKind::PermissionDenied));
        assert_eq!(transfer(&mut failed, &packet, 0).expect("read error"), None);
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

        assert_eq!(transfer(&mut device, &packet, 7).expect("transfer"), None);
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
    fn battery_report_timeout_returns_no_level() {
        let mut device = FakeDevice::default();
        device.reply(&response(4, ROOT_FEATURE, &[7]));
        device.timeout();

        assert_eq!(battery_level(&mut device, 4).expect("timeout"), None);
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
    fn battery_timeout_returns_success_without_level() {
        let mut device = FakeDevice::default();
        device.timeout();

        assert_eq!(query_device(&mut device, 1, false).expect("query"), None);
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
}
