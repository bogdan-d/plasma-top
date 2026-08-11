#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::domain::boundary::{
    BusKind, DbusOutput, DbusRequest, UdisksManagedObject, UdisksSmartKind, UpowerDeviceProperties,
};
use crate::test_support::FakeDbus;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const SYSTEM: BusKind = BusKind::System;
const UPOWER_NAME: &str = "org.freedesktop.UPower";

/// monotonic(t) → ClockSnapshot with a zero wall clock (tests only use
/// monotonic time for TTL gates).
fn clock(seconds: u64) -> ClockSnapshot {
    ClockSnapshot {
        monotonic: Duration::from_secs(seconds),
        wall: SystemTime::UNIX_EPOCH,
    }
}

fn battery_props_reply(path: &str, props: &[(&str, &str)]) -> DbusOutput {
    let mut output = UpowerDeviceProperties::default();
    for (name, value) in props {
        match *name {
            "Percentage" => output.percentage = value.parse().ok(),
            "State" => output.state = value.parse().ok(),
            "EnergyRate" => output.energy_rate = value.parse().ok(),
            "Model" => output.model = Some((*value).to_owned()),
            "Type" => output.kind = value.parse().ok(),
            _ => {}
        }
    }
    let _ = path;
    DbusOutput::UpowerDeviceProperties(output)
}

/// In-memory fake Bolt facade with FIFO replies keyed by `(dev_idx,
/// want_name)`. Each call pops the next queued reply.
#[derive(Default)]
struct FakeBolt {
    ok_replies: Vec<(i32, bool, Option<BoltBattery>)>,
    err_replies: Vec<(i32, bool)>,
    calls: Vec<(i32, bool)>,
}

impl FakeBolt {
    fn push_ok(
        &mut self,
        dev_idx: i32,
        want_name: bool,
        battery: Option<BoltBattery>,
    ) -> &mut Self {
        self.ok_replies.push((dev_idx, want_name, battery));
        self
    }

    fn push_err(&mut self, dev_idx: i32, want_name: bool) -> &mut Self {
        self.err_replies.push((dev_idx, want_name));
        self
    }

    fn calls(&self) -> &[(i32, bool)] {
        &self.calls
    }
}

impl BoltBatteryFacade for FakeBolt {
    fn query(
        &mut self,
        dev_idx: i32,
        want_name: bool,
    ) -> Result<Option<BoltBattery>, BoundaryError> {
        self.calls.push((dev_idx, want_name));
        if let Some(idx) = self
            .err_replies
            .iter()
            .position(|(d, w)| *d == dev_idx && *w == want_name)
        {
            self.err_replies.swap_remove(idx);
            return Err(BoundaryError::DbusCallFailed {
                bus: BusKind::Session,
                service: "bolt".to_owned(),
                path: "/dev/hidraw0".to_owned(),
                interface: "HIDPP".to_owned(),
                member: "query".to_owned(),
                detail: "hid read timeout".to_owned(),
            });
        }
        if let Some(idx) = self
            .ok_replies
            .iter()
            .position(|(d, w, _)| *d == dev_idx && *w == want_name)
        {
            let (_, _, battery) = self.ok_replies.swap_remove(idx);
            Ok(battery)
        } else {
            Ok(None)
        }
    }
}

/// Minimal temp-directory helper for sysfs fixture trees (mirrors
/// `sensors::disk::tests::TempTree`).
struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root =
            std::env::temp_dir().join(format!("plasma-top-power-{}-{unique}", std::process::id(),));
        fs::create_dir_all(&root).expect("temp root");
        Self { root }
    }

    fn sys(&self) -> PathBuf {
        self.root.join("sys")
    }

    fn write(&self, relative: &str, content: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent");
        }
        fs::write(&path, content).expect("write");
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

mod numeric;
mod peripheral_battery;
mod smart;
mod system_battery;
mod upower;
