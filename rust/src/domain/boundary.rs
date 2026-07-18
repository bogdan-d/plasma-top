//! External boundary stubs for later runtime lanes.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use crate::domain::metric::{Capability, Metric};

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

/// Filesystem roots resolved by later runtime/config lanes.
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

/// Hardware discovery snapshot stub.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HardwareSnapshot {
    /// Capabilities discovered on the current host.
    pub capabilities: BTreeSet<Capability>,
    /// Metrics that a later collection lane can attempt to populate.
    pub metrics: BTreeSet<Metric>,
}

/// Reading snapshot stub.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadingsSnapshot {
    /// Collection timestamp.
    pub collected_at: ClockSnapshot,
    /// Metrics populated in this sample.
    pub metrics: BTreeSet<Metric>,
}

/// Mutable daemon state stub shared by later runtime lanes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonStateSnapshot {
    /// Active tooltip page index.
    pub active_page: usize,
    /// Number of published pages.
    pub page_count: usize,
    /// Timestamp of the most recent successful poll.
    pub last_poll: Option<ClockSnapshot>,
}

impl Default for DaemonStateSnapshot {
    fn default() -> Self {
        Self {
            active_page: 0,
            page_count: 1,
            last_poll: None,
        }
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
    fn daemon_state_defaults_to_single_full_page() {
        let state = DaemonStateSnapshot::default();

        assert_eq!(state.active_page, 0);
        assert_eq!(state.page_count, 1);
        assert_eq!(state.last_poll, None);
    }
}
