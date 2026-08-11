//! External boundary contracts shared by runtime lanes.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Process execution status captured by the future command-runner boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandStatus {
    /// The process exited with a code.
    Exit(i32),
    /// The process terminated because of a signal.
    Signal(i32),
}

/// Captured command output contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Resolved program path or executable token.
    pub program: PathBuf,
    /// Exact argv values passed to the child process.
    pub args: Vec<OsString>,
    /// Child exit status.
    pub status: CommandStatus,
    /// Raw stdout bytes.
    pub stdout: Vec<u8>,
    /// Raw stderr bytes.
    pub stderr: Vec<u8>,
    /// Bytes discarded after the combined retained-output limit was reached.
    pub truncation: CommandTruncation,
}

/// Per-stream output bytes discarded by the command service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CommandTruncation {
    /// Discarded stdout bytes.
    pub stdout_bytes: u64,
    /// Discarded stderr bytes.
    pub stderr_bytes: u64,
}

impl CommandTruncation {
    /// Returns whether any output was discarded.
    #[must_use]
    pub const fn is_truncated(self) -> bool {
        self.stdout_bytes != 0 || self.stderr_bytes != 0
    }
}

/// D-Bus bus selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BusKind {
    /// The desktop session bus.
    Session,
    /// The system bus.
    System,
}

/// Desktop-notification urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationUrgency {
    /// Critical urgency used by every current PlasmaTop alert.
    Critical,
}

/// Desktop-notification expiry policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationTimeout {
    /// Keep the notification until the desktop or user dismisses it.
    Never,
}

/// Complete desktop-notification payload passed to the production adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPayload {
    /// Notification title.
    pub title: String,
    /// Notification body.
    pub body: String,
    /// Freedesktop icon name.
    pub icon: String,
    /// Desktop urgency hint.
    pub urgency: NotificationUrgency,
    /// Desktop expiry policy.
    pub timeout: NotificationTimeout,
}

/// Failure returned by a desktop-notification adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationError {
    /// Human-readable adapter failure detail.
    pub detail: String,
}

impl Display for NotificationError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "desktop notification failed: {}", self.detail)
    }
}

impl std::error::Error for NotificationError {}

/// Desktop-notification boundary shared by production and deterministic fakes.
pub trait NotificationFacade {
    /// Attempts to display one exact notification payload.
    ///
    /// # Errors
    ///
    /// Returns [`NotificationError`] when the desktop service is unavailable or
    /// rejects the notification. Notification state-machine callers must retain
    /// their state transition and report the failure instead of panicking.
    fn send(&mut self, payload: &NotificationPayload) -> Result<(), NotificationError>;
}

/// Typed UPower properties used by system and peripheral battery paths.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct UpowerDeviceProperties {
    /// Charge percentage.
    pub percentage: Option<f64>,
    /// UPower state enum value.
    pub state: Option<u32>,
    /// Current energy rate in watts.
    pub energy_rate: Option<f64>,
    /// Device model.
    pub model: Option<String>,
    /// UPower device type enum value.
    pub kind: Option<u32>,
}

/// Typed subset of one UDisks managed object used by SMART discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdisksManagedObject {
    /// Object path.
    pub path: String,
    /// Interfaces exposed by the object.
    pub interfaces: BTreeSet<String>,
    /// `org.freedesktop.UDisks2.Block.Drive` object path when present.
    pub drive: Option<String>,
}

/// UDisks SMART interface selected for a drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UdisksSmartKind {
    /// `org.freedesktop.UDisks2.NVMe.Controller`.
    Nvme,
    /// `org.freedesktop.UDisks2.Drive.Ata`.
    Ata,
}

impl UdisksSmartKind {
    /// Returns the D-Bus interface name.
    #[must_use]
    pub const fn interface(self) -> &'static str {
        match self {
            Self::Nvme => "org.freedesktop.UDisks2.NVMe.Controller",
            Self::Ata => "org.freedesktop.UDisks2.Drive.Ata",
        }
    }
}

/// Exact typed system-bus request passed to production and fake services.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DbusRequest {
    /// Enumerate UPower device object paths.
    UpowerEnumerate,
    /// Read the UPower properties needed for one device.
    UpowerDeviceProperties {
        /// UPower device object path.
        object_path: String,
    },
    /// Read the UDisks managed-object inventory.
    UdisksManagedObjects,
    /// Refresh SMART state for one drive.
    UdisksSmartUpdate {
        /// UDisks drive object path.
        object_path: String,
        /// Drive SMART interface.
        kind: UdisksSmartKind,
        /// Per-call timeout for the drive refresh ioctl.
        timeout: Duration,
    },
    /// Read the typed SMART health property for one drive.
    UdisksSmartProperty {
        /// UDisks drive object path.
        object_path: String,
        /// Drive SMART interface and property family.
        kind: UdisksSmartKind,
    },
}

impl DbusRequest {
    /// Returns stable call metadata for diagnostics and fake mismatch errors.
    #[must_use]
    pub fn metadata(&self) -> (BusKind, &'static str, &str, &'static str, &'static str) {
        match self {
            Self::UpowerEnumerate => (
                BusKind::System,
                "org.freedesktop.UPower",
                "/org/freedesktop/UPower",
                "org.freedesktop.UPower",
                "EnumerateDevices",
            ),
            Self::UpowerDeviceProperties { object_path } => (
                BusKind::System,
                "org.freedesktop.UPower",
                object_path,
                "org.freedesktop.DBus.Properties",
                "GetAll",
            ),
            Self::UdisksManagedObjects => (
                BusKind::System,
                "org.freedesktop.UDisks2",
                "/org/freedesktop/UDisks2",
                "org.freedesktop.DBus.ObjectManager",
                "GetManagedObjects",
            ),
            Self::UdisksSmartUpdate {
                object_path, kind, ..
            } => (
                BusKind::System,
                "org.freedesktop.UDisks2",
                object_path,
                kind.interface(),
                "SmartUpdate",
            ),
            Self::UdisksSmartProperty { object_path, kind } => (
                BusKind::System,
                "org.freedesktop.UDisks2",
                object_path,
                "org.freedesktop.DBus.Properties",
                match kind {
                    UdisksSmartKind::Nvme => "Get(SmartCriticalWarning)",
                    UdisksSmartKind::Ata => "Get(SmartFailing)",
                },
            ),
        }
    }
}

/// Typed system-bus reply. Variants are never flattened into strings.
#[derive(Debug, Clone, PartialEq)]
pub enum DbusOutput {
    /// UPower device object paths.
    UpowerDevices(Vec<String>),
    /// Selected UPower device properties.
    UpowerDeviceProperties(UpowerDeviceProperties),
    /// Selected UDisks managed-object data.
    UdisksManagedObjects(Vec<UdisksManagedObject>),
    /// Successful SMART refresh.
    UdisksSmartUpdated,
    /// NVMe SMART critical-warning names. An empty list is healthy.
    UdisksNvmeCriticalWarnings(Vec<String>),
    /// ATA failing flag.
    UdisksAtaFailing(bool),
}

/// Coalesced async I/O event consumed by the daemon scheduler shell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IoEvent {
    /// UPower device inventory or properties changed.
    UpowerChanged,
    /// logind announced the start or end of sleep preparation.
    PrepareForSleep(bool),
}

/// Shared boundary error contract used by command and D-Bus adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoundaryError {
    /// A fake command runner was asked to execute a command for which no reply
    /// was enqueued.
    CommandNotQueued {
        /// Program path the fake was asked to run.
        program: PathBuf,
        /// Argv values the fake was asked to run with.
        args: Vec<OsString>,
    },
    /// A production command adapter failed before it could return output.
    CommandFailed {
        /// Program path or executable token.
        program: PathBuf,
        /// Exact argv values passed to the child process.
        args: Vec<OsString>,
        /// Human-readable failure detail.
        detail: String,
    },
    /// A fake D-Bus facade was asked to dispatch a call for which no reply was
    /// enqueued.
    DbusCallNotQueued {
        /// Which bus the fake was asked to call on.
        bus: BusKind,
        /// Remote service name.
        service: String,
        /// Object path.
        path: String,
        /// Interface name.
        interface: String,
        /// Method or signal member name.
        member: String,
    },
    /// A production D-Bus facade failed before it could return a decoded reply.
    DbusCallFailed {
        /// Which bus the call targeted.
        bus: BusKind,
        /// Remote service name.
        service: String,
        /// Object path.
        path: String,
        /// Interface name.
        interface: String,
        /// Method or signal member name.
        member: String,
        /// Human-readable failure detail.
        detail: String,
    },
    /// A production HID adapter could not discover, open, or communicate with
    /// a device.
    HidFailed {
        /// Device path, when discovery reached a concrete hidraw node.
        path: Option<PathBuf>,
        /// Human-readable failure detail.
        detail: String,
    },
}

impl Display for BoundaryError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandNotQueued { program, args } => {
                write!(
                    formatter,
                    "no fixture reply queued for command `{}` with {} arg(s)",
                    program.display(),
                    args.len(),
                )
            }
            Self::CommandFailed {
                program,
                args,
                detail,
            } => {
                write!(
                    formatter,
                    "command `{}` with {} arg(s) failed: {detail}",
                    program.display(),
                    args.len(),
                )
            }
            Self::DbusCallNotQueued {
                bus,
                service,
                path,
                interface,
                member,
            } => {
                let bus_label = match bus {
                    BusKind::Session => "session",
                    BusKind::System => "system",
                };
                write!(
                    formatter,
                    "no fixture reply queued for D-Bus {bus_label} call \
                     `{service}` `{path}` `{interface}` `{member}`",
                )
            }
            Self::DbusCallFailed {
                bus,
                service,
                path,
                interface,
                member,
                detail,
            } => {
                let bus_label = match bus {
                    BusKind::Session => "session",
                    BusKind::System => "system",
                };
                write!(
                    formatter,
                    "D-Bus {bus_label} call `{service}` `{path}` `{interface}` `{member}` failed: {detail}",
                )
            }
            Self::HidFailed { path, detail } => {
                if let Some(path) = path {
                    write!(
                        formatter,
                        "HID device `{}` failed: {detail}",
                        path.display()
                    )
                } else {
                    write!(formatter, "HID device failed: {detail}")
                }
            }
        }
    }
}

impl std::error::Error for BoundaryError {}

/// Command-runner boundary implemented by production adapters and test fakes.
pub trait CommandRunner {
    /// Runs `program` with `args` under the requested `timeout` and returns
    /// the captured output.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] when the adapter cannot dispatch the command.
    fn run(
        &mut self,
        program: &std::path::Path,
        args: &[OsString],
        timeout: Duration,
    ) -> Result<CommandOutput, BoundaryError>;
}

/// Generic D-Bus call facade implemented by production adapters and test fakes.
pub trait DbusFacade {
    /// Invokes an exact method request and returns the decoded reply.
    ///
    /// # Errors
    ///
    /// Returns [`BoundaryError`] when the adapter cannot dispatch the call.
    fn call(&mut self, request: DbusRequest) -> Result<DbusOutput, BoundaryError>;
}

/// Clock snapshot stub used by daemon and collection boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClockSnapshot {
    /// Monotonic elapsed time.
    pub monotonic: Duration,
    /// Wall-clock timestamp.
    pub wall: SystemTime,
}

impl Default for ClockSnapshot {
    fn default() -> Self {
        Self {
            monotonic: Duration::ZERO,
            wall: SystemTime::UNIX_EPOCH,
        }
    }
}

/// Filesystem roots shared by runtime, config, sensors, and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilesystemRoots {
    /// Runtime publication root.
    pub runtime_root: Option<PathBuf>,
    /// Cache root.
    pub cache_root: Option<PathBuf>,
    /// Config root.
    pub config_root: Option<PathBuf>,
    /// Procfs root.
    pub proc_root: PathBuf,
    /// Sysfs root.
    pub sys_root: PathBuf,
}

impl Default for FilesystemRoots {
    fn default() -> Self {
        Self {
            runtime_root: None,
            cache_root: None,
            config_root: None,
            proc_root: PathBuf::from("/proc"),
            sys_root: PathBuf::from("/sys"),
        }
    }
}

impl FilesystemRoots {
    /// Returns the future runtime `state/` directory when the runtime root is known.
    #[must_use]
    pub fn state_root(&self) -> Option<PathBuf> {
        self.runtime_root.as_ref().map(|root| root.join("state"))
    }
}

#[cfg(test)]
mod tests;
