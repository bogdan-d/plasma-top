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
    /// A report could not be read.
    Read(io::Error),
    /// No report arrived before the transport deadline.
    Timeout,
    /// The bounded read loop received no matching response.
    NoMatchingResponse,
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
            Self::Read(source) => write!(formatter, "cannot read HID report: {source}"),
            Self::Timeout => formatter.write_str("timed out waiting for HID response"),
            Self::NoMatchingResponse => {
                write!(
                    formatter,
                    "no matching HID response after {MAX_READS} reads"
                )
            }
        }
    }
}

impl std::error::Error for HidError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Open { source, .. } | Self::Write(source) | Self::Read(source) => Some(source),
            Self::DeviceAbsent
            | Self::InvalidDeviceIndex(_)
            | Self::Timeout
            | Self::NoMatchingResponse => None,
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

    fn query_inner(
        &self,
        dev_idx: i32,
        want_name: bool,
    ) -> Result<Option<BoltBattery>, BoundaryError> {
        let dev_idx = u8::try_from(dev_idx)
            .map_err(|_| boundary_error(HidError::InvalidDeviceIndex(dev_idx), None))?;
        let path = find_bolt_hidraw(&self.sys_root, &self.dev_root)
            .ok_or_else(|| boundary_error(HidError::DeviceAbsent, None))?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|source| {
                boundary_error(
                    HidError::Open {
                        path: path.clone(),
                        source,
                    },
                    Some(&path),
                )
            })?;
        let mut device = HidrawDevice(file);
        self.query_reports(&mut device, dev_idx, want_name, Some(&path))
    }

    fn query_reports(
        &self,
        device: &mut impl ReportIo,
        dev_idx: u8,
        want_name: bool,
        path: Option<&Path>,
    ) -> Result<Option<BoltBattery>, BoundaryError> {
        query_device(device, dev_idx, want_name).map_err(|error| boundary_error(error, path))
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
        self.query_inner(dev_idx, want_name)
    }
}

fn boundary_error(error: HidError, path: Option<&Path>) -> BoundaryError {
    let path = path.map(Path::to_path_buf).or_else(|| match &error {
        HidError::Open { path, .. } => Some(path.clone()),
        _ => None,
    });
    BoundaryError::HidFailed {
        path,
        detail: error.to_string(),
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
) -> Result<Vec<u8>, HidError> {
    device.write_report(packet).map_err(HidError::Write)?;
    let mut buffer = [0_u8; 64];
    for _ in 0..MAX_READS {
        let read = device
            .read_report_timeout(&mut buffer, TIMEOUT_MS)
            .map_err(HidError::Read)?;
        if read >= 5 && buffer[1] == packet[1] && buffer[2] == expected_feature {
            return Ok(buffer[..read].to_vec());
        }
        if read == 0 {
            return Err(HidError::Timeout);
        }
    }
    Err(HidError::NoMatchingResponse)
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
    Ok(transfer(device, &packet, ROOT_FEATURE)?[4])
}

fn battery_level(device: &mut impl ReportIo, dev_idx: u8) -> Result<Option<u8>, HidError> {
    let feature = feature_index(device, dev_idx, UNIFIED_BATTERY_FEATURE)?;
    if feature == 0 {
        return Ok(None);
    }
    let mut packet = [0_u8; REPORT_LEN];
    packet[..4].copy_from_slice(&[LONG_REPORT_ID, dev_idx, feature, (1 << 4) | SOFTWARE_ID]);
    Ok(Some(transfer(device, &packet, feature)?[4]))
}

fn device_name(device: &mut impl ReportIo, dev_idx: u8) -> Result<String, HidError> {
    let feature = feature_index(device, dev_idx, DEVICE_NAME_FEATURE)?;
    if feature == 0 {
        return Ok(String::new());
    }
    let mut packet = [0_u8; REPORT_LEN];
    packet[..5].copy_from_slice(&[LONG_REPORT_ID, dev_idx, feature, (1 << 4) | SOFTWARE_ID, 0]);
    let response = transfer(device, &packet, feature)?;
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
mod tests;
