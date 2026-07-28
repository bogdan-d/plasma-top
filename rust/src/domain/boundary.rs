//! External boundary contracts shared by runtime lanes.

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

/// Typed argument needed by the currently ported D-Bus methods.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DbusArgument {
    /// A D-Bus string (`s`).
    String(String),
    /// An empty string-to-variant dictionary (`a{sv}`).
    EmptyStringVariantDict,
}

/// Exact D-Bus method request passed to production adapters and test fakes.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DbusRequest {
    /// Bus carrying the request.
    pub bus: BusKind,
    /// Remote service name.
    pub service: String,
    /// Remote object path.
    pub object_path: String,
    /// Interface containing the method.
    pub interface: String,
    /// Method member name.
    pub member: String,
    /// Ordered typed method arguments.
    pub arguments: Vec<DbusArgument>,
    /// Per-call timeout; `None` selects the adapter default.
    pub timeout: Option<Duration>,
}

/// Stringly placeholder for a D-Bus response until typed facades land.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbusOutput {
    /// Which bus produced the message.
    pub bus: BusKind,
    /// Remote service name.
    pub service: String,
    /// Object path.
    pub object_path: String,
    /// Interface name.
    pub interface: String,
    /// Method or signal member name.
    pub member: String,
    /// Stringified payload fragments preserved for contract discussion.
    pub body: Vec<String>,
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
mod tests {
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
}
