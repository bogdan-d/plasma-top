//! Collector composition tests.
//!
//! Deterministic coverage of capability-driven discovery, rescan, and
//! collection: every individual capability, representative combined sets, the
//! empty set, exact ordered call traces, no-duplicate/no-unrequested-call
//! proofs, `skip_slow`, cache hit/expiry, peripheral rescan timing/retention,
//! hardware/service absence, malformed inputs, adapter failures with failure
//! isolation, network/disk device-change rate resets, history coordination,
//! battery (sys/periph/bolt), SMART/hwmon paths, external status files, and
//! NVIDIA NVML success/init-failure/read-failure/fallback selection.
//!
//! Every test builds its own temp proc/sys tree and in-memory fakes; none touch
//! host `/proc`, `/sys`, the system bus, HID, NVML, runtime files, or a desktop.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::field_reassign_with_default
)]

use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config::{Config, Mounts, Section, Surface};
use crate::domain::boundary::{BusKind, ClockSnapshot, CommandOutput, CommandStatus, DbusOutput};
use crate::domain::metric::Capability;
use crate::domain::readings::{DisplaySnapshot, HardwareInventory};
use crate::sensors::gpu_nvidia::{NvidiaMetrics, NvmlError, NvmlFacade, NvmlMetrics};
use crate::sensors::power::{BoltBattery, BoltBatteryFacade};
use crate::sensors::{
    external::ExternalState, gpu_history::GpuHistoryState,
    gpu_nvidia::NvidiaState as TestNvidiaState, network::NetworkState as TestNetworkState,
    power::PowerState,
};
use crate::test_support::{FakeClock, FakeCommandRunner, FakeDbus};

use super::{
    AttemptStatus, NETWORK_COMMAND_TIMEOUT, OwnerRefs, Timings, attempt_cpu, attempt_cpu_cores,
    attempt_disk_io, attempt_disk_temperature, attempt_intel_usage, attempt_memory,
    attempt_network_info, attempt_network_speed, attempt_process, collect,
    collect_with_notifications, cpu, detect_net_device, discover_hardware, disk, gpu_intel, memory,
    needs_periph_rescan, process, rescan_peripherals, sample_due, timed,
};

#[derive(Default)]
struct TestOwners {
    cpu: cpu::CpuState,
    memory: memory::MemoryState,
    network: TestNetworkState,
    disk: disk::DiskState,
    process: process::ProcessState,
    intel_gpu: gpu_intel::IntelGpuState,
    power: PowerState,
    nvidia: TestNvidiaState,
    gpu_history: GpuHistoryState,
    external: ExternalState,
}

impl TestOwners {
    fn refs(&mut self) -> OwnerRefs<'_> {
        OwnerRefs {
            cpu: &mut self.cpu,
            memory: &mut self.memory,
            network: &mut self.network,
            disk: &mut self.disk,
            process: &mut self.process,
            intel_gpu: &mut self.intel_gpu,
            power: &mut self.power,
            nvidia: &mut self.nvidia,
            gpu_history: &mut self.gpu_history,
            external: &mut self.external,
        }
    }
}

// ── Constants mirrored from the lanes (kept local so tests stay self-contained) ┐
const UPOWER_NAME: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UPOWER_IFACE: &str = "org.freedesktop.UPower";
const SYSTEM: BusKind = BusKind::System;
const IP: &str = "ip";

// ── Fixture tree ─────────────────────────────────────────────────────────────

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "plasma-top-collector-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root).expect("temp root");
        Self { root }
    }

    fn proc(&self) -> PathBuf {
        self.root.join("proc")
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

    fn mkdir(&self, relative: &str) {
        fs::create_dir_all(self.root.join(relative)).expect("mkdir");
    }

    /// Creates a symlink whose link parent dirs are created first (mirrors the
    /// gpu_intel/disk lane test helpers).
    fn symlink(&self, original: &str, link_relative: &str) {
        let link = self.root.join(link_relative);
        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent).expect("link parent");
        }
        std::os::unix::fs::symlink(original, &link).expect("symlink");
    }
}

/// Builds a `/proc/[pid]/stat` line with `utime`/`stime`/`rss` at the correct
/// post-`)` field indices (11/12/21), matching `process::read_proc_stat_times`.
fn proc_stat_line(pid: u32, comm: &str, utime: u64, stime: u64, rss: u64) -> String {
    let mut fields: Vec<String> = (0..22).map(|i| (i + 100).to_string()).collect();
    fields[11] = utime.to_string();
    fields[12] = stime.to_string();
    fields[21] = rss.to_string();
    format!("{pid} ({comm}) {}\n", fields.join(" "))
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// A baseline proc tree: aggregate + 2 cores, meminfo, uptime, loadavg.
fn baseline_proc(tree: &TempTree) {
    tree.write(
        "proc/stat",
        "cpu  10 0 10 80 0 0 0 0 0 0\n\
         cpu0 5 0 5 40 0 0 0 0 0 0\n\
         cpu1 5 0 5 40 0 0 0 0 0 0\n",
    );
    tree.write(
        "proc/meminfo",
        "MemTotal:        2097152 kB\n\
         MemFree:          524288 kB\n\
         MemAvailable:    1572864 kB\n\
         SwapTotal:       1048576 kB\n\
         SwapFree:         262144 kB\n",
    );
    tree.write("proc/uptime", "12345.67 97531.11\n");
    tree.write("proc/loadavg", "0.50 0.40 0.30 1/100 1000\n");
}

fn clock(seconds: u64) -> ClockSnapshot {
    ClockSnapshot {
        monotonic: Duration::from_secs(seconds),
        wall: UNIX_EPOCH + Duration::from_secs(seconds),
    }
}

fn fixed_clock(seconds: u64) -> impl FnMut() -> ClockSnapshot {
    move || clock(seconds)
}

// ── Config builders ──────────────────────────────────────────────────────────

fn panel_section(items: &[&str]) -> Surface {
    Surface {
        sections: vec![Section {
            key: String::from("main"),
            title: String::new(),
            items: items.iter().map(|s| (*s).to_owned()).collect(),
        }],
        glyphs: true,
    }
}

/// Config whose panel lists exactly `items` (tooltip empty) — drives
/// `needed_capabilities` from just those tokens.
fn cfg_panel(items: &[&str]) -> Config {
    let mut cfg = Config::default();
    cfg.panel = panel_section(items);
    cfg
}

// ── In-memory fakes for NVML and Bolt ────────────────────────────────────────

struct FakeNvml {
    replies: VecDeque<Result<NvmlMetrics, NvmlError>>,
    calls: usize,
}

impl FakeNvml {
    fn new(replies: Vec<Result<NvidiaMetrics, NvmlError>>) -> Self {
        Self {
            replies: replies
                .into_iter()
                .map(|reply| reply.map(Into::into))
                .collect(),
            calls: 0,
        }
    }

    fn typed(replies: Vec<Result<NvmlMetrics, NvmlError>>) -> Self {
        Self {
            replies: replies.into(),
            calls: 0,
        }
    }
}

impl NvmlFacade for FakeNvml {
    fn read_device_zero(&mut self) -> Result<NvmlMetrics, NvmlError> {
        self.calls += 1;
        self.replies.pop_front().unwrap_or(Err(NvmlError::Read))
    }
}

struct FakeBolt {
    replies: VecDeque<Result<Option<BoltBattery>, crate::domain::boundary::BoundaryError>>,
    calls: usize,
}

impl FakeBolt {
    fn new(
        replies: Vec<Result<Option<BoltBattery>, crate::domain::boundary::BoundaryError>>,
    ) -> Self {
        Self {
            replies: replies.into_iter().collect(),
            calls: 0,
        }
    }
}

impl BoltBatteryFacade for FakeBolt {
    fn query(
        &mut self,
        _dev_idx: i32,
        _want_name: bool,
    ) -> Result<Option<BoltBattery>, crate::domain::boundary::BoundaryError> {
        self.calls += 1;
        self.replies.pop_front().unwrap_or(Ok(None))
    }
}

// ── D-Bus reply helpers ──────────────────────────────────────────────────────

fn enumerate_reply(paths: &[&str]) -> DbusOutput {
    DbusOutput {
        bus: SYSTEM,
        service: UPOWER_NAME.to_owned(),
        object_path: UPOWER_PATH.to_owned(),
        interface: UPOWER_IFACE.to_owned(),
        member: "EnumerateDevices".to_owned(),
        body: paths.iter().map(|s| (*s).to_owned()).collect(),
    }
}

fn getall_reply(path: &str, props: &[(&str, &str)]) -> DbusOutput {
    let mut body = Vec::new();
    for (k, v) in props {
        body.push((*k).to_owned());
        body.push((*v).to_owned());
    }
    DbusOutput {
        bus: SYSTEM,
        service: UPOWER_NAME.to_owned(),
        object_path: path.to_owned(),
        interface: "org.freedesktop.DBus.Properties".to_owned(),
        member: "GetAll".to_owned(),
        body,
    }
}

fn ok_cmd(program: &str, args: &[&str], stdout: &str) -> CommandOutput {
    CommandOutput {
        program: Path::new(program).to_path_buf(),
        args: args.iter().map(|a| std::ffi::OsString::from(*a)).collect(),
        status: CommandStatus::Exit(0),
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
    }
}

/// Single-collection helper: builds the `CollectCtx`, runs `collect`, and
/// returns the fresh [`DisplaySnapshot`]. All `&mut` fakes share one borrow
/// lifetime so the caller can inspect their traces once the call returns.
#[allow(clippy::too_many_arguments)]
fn run_collect<'a>(
    lanes: &'a mut TestOwners,
    hw: &'a mut HardwareInventory,
    cfg: &'a Config,
    proc_root: &'a Path,
    sys_root: &'a Path,
    commands: &'a mut FakeCommandRunner,
    dbus: &'a mut FakeDbus,
    nvml: Option<&'a mut FakeNvml>,
    bolt: Option<&'a mut FakeBolt>,
    clock: ClockSnapshot,
    skip_slow: bool,
) -> DisplaySnapshot {
    let mut capture_clock = || clock;
    run_collect_with_clock(
        lanes,
        hw,
        cfg,
        proc_root,
        sys_root,
        commands,
        dbus,
        nvml,
        bolt,
        &mut capture_clock,
        skip_slow,
    )
}

#[allow(clippy::too_many_arguments)]
fn run_collect_with_clock<'a>(
    lanes: &'a mut TestOwners,
    hw: &'a mut HardwareInventory,
    cfg: &'a Config,
    proc_root: &'a Path,
    sys_root: &'a Path,
    commands: &'a mut FakeCommandRunner,
    dbus: &'a mut FakeDbus,
    nvml: Option<&'a mut FakeNvml>,
    bolt: Option<&'a mut FakeBolt>,
    capture_clock: &'a mut dyn FnMut() -> ClockSnapshot,
    skip_slow: bool,
) -> DisplaySnapshot {
    let commands_dyn: &mut dyn crate::domain::boundary::CommandRunner = commands;
    let dbus_dyn: &mut dyn crate::domain::boundary::DbusFacade = dbus;
    let nvml_dyn: Option<&mut dyn NvmlFacade> = nvml.map(|n| n as &mut dyn NvmlFacade);
    let bolt_dyn: Option<&mut dyn BoltBatteryFacade> =
        bolt.map(|b| b as &mut dyn BoltBatteryFacade);
    let mut ctx = super::CollectCtx {
        proc_root,
        sys_root,
        commands: commands_dyn,
        dbus: dbus_dyn,
        nvml: nvml_dyn,
        bolt: bolt_dyn,
        clock: capture_clock,
        skip_slow,
    };
    collect(lanes.refs(), hw, cfg, &mut ctx, None)
}

#[allow(clippy::too_many_arguments)]
fn run_collect_output<'a>(
    lanes: &'a mut TestOwners,
    hw: &'a mut HardwareInventory,
    cfg: &'a Config,
    proc_root: &'a Path,
    sys_root: &'a Path,
    commands: &'a mut FakeCommandRunner,
    dbus: &'a mut FakeDbus,
    clock: ClockSnapshot,
) -> super::CollectionOutput {
    let mut capture_clock = || clock;
    let mut ctx = super::CollectCtx {
        proc_root,
        sys_root,
        commands,
        dbus,
        nvml: None,
        bolt: None,
        clock: &mut capture_clock,
        skip_slow: false,
    };
    collect_with_notifications(lanes.refs(), hw, cfg, &mut ctx, None)
}

// ── timed_section ────────────────────────────────────────────────────────────

mod accelerators_power;
mod capability_calls;
mod coordination;
mod core_collection;
mod discovery;
mod disk_collection;
mod external_collection;
mod histories;
mod intel_collection;
mod network_collection;
mod nvidia_collection;
mod sampling;
mod scheduled_execution;
mod slow_source_coordination;
