#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use crate::domain::boundary::{BusKind, DbusFacade, DbusOutput, DbusRequest};
use crate::test_support::FakeDbus;
use serde_json::Value;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Helper to build a `DbusOutput` body tagged with the call signature so
/// the fake can echo it. Production adapters build this from `busctl` JSON
/// replies.
fn dbus_body(
    bus: BusKind,
    service: &str,
    path: &str,
    iface: &str,
    member: &str,
    body: Vec<String>,
) -> DbusOutput {
    DbusOutput {
        bus,
        service: service.to_owned(),
        object_path: path.to_owned(),
        interface: iface.to_owned(),
        member: member.to_owned(),
        body,
    }
}

const SYSTEM: BusKind = BusKind::System;

struct RawJsonDbus {
    replies: VecDeque<Value>,
}

impl RawJsonDbus {
    fn new(replies: impl IntoIterator<Item = Value>) -> Self {
        Self {
            replies: replies.into_iter().collect(),
        }
    }
}

impl DbusFacade for RawJsonDbus {
    fn call(&mut self, request: DbusRequest) -> Result<DbusOutput, BoundaryError> {
        let Some(reply) = self.replies.pop_front() else {
            return Err(BoundaryError::DbusCallNotQueued {
                bus: request.bus,
                service: request.service,
                path: request.object_path,
                interface: request.interface,
                member: request.member,
            });
        };
        let body = crate::adapters::normalize_dbus_body(&request, &reply)?;
        Ok(DbusOutput {
            bus: request.bus,
            service: request.service,
            object_path: request.object_path,
            interface: request.interface,
            member: request.member,
            body,
        })
    }
}

/// monotonic(t) → ClockSnapshot with a zero wall clock (tests only use
/// monotonic time for TTL gates).
fn clock(seconds: u64) -> ClockSnapshot {
    ClockSnapshot {
        monotonic: Duration::from_secs(seconds),
        wall: SystemTime::UNIX_EPOCH,
    }
}

fn upath(member: &str, body: Vec<String>) -> DbusOutput {
    dbus_body(
        SYSTEM,
        UPOWER_NAME,
        "/org/freedesktop/UPower",
        UPOWER_IFACE,
        member,
        body,
    )
}

fn battery_props_reply(path: &str, props: &[(&str, &str)]) -> DbusOutput {
    let body: Vec<String> = props
        .iter()
        .flat_map(|(k, v)| [(*k).to_owned(), (*v).to_owned()])
        .collect();
    dbus_body(
        SYSTEM,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        body,
    )
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
