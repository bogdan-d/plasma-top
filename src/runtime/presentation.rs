//! Per-applet tooltip presentation leases.

use std::fmt::{self, Display, Formatter};
use std::fs::{File, OpenOptions};
use std::num::NonZeroU64;
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, SystemTime};

use nix::fcntl::{Flock, FlockArg};

use super::atomic::write_atomic;

/// Maximum age of a presentation heartbeat.
pub const LEASE_LIFETIME: Duration = Duration::from_secs(90);

/// Validated positive numeric Plasma applet instance identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceId(NonZeroU64);

impl Display for InstanceId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl FromStr for InstanceId {
    type Err = InvalidInstanceId;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(InvalidInstanceId);
        }
        value
            .parse::<NonZeroU64>()
            .map(Self)
            .map_err(|_| InvalidInstanceId)
    }
}

/// Failure to parse a strict positive numeric instance identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidInstanceId;

impl Display for InvalidInstanceId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("instance id must be a positive decimal integer")
    }
}

impl std::error::Error for InvalidInstanceId {}

/// Current aggregate lease state and bounded time until its next expiry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseState {
    /// Whether at least one live applet lease exists.
    pub presented: bool,
    /// Remaining duration until the earliest live lease expires.
    pub next_expiry_in: Option<Duration>,
}

/// Creates or atomically refreshes one instance lease.
pub fn present(instance: InstanceId) -> std::io::Result<()> {
    let directory = super::presented_dir();
    present_at(&directory, instance)
}

fn present_at(directory: &Path, instance: InstanceId) -> std::io::Result<()> {
    with_lock(directory, || {
        write_atomic(&directory.join(instance.to_string()), b"")
    })
}

/// Removes one instance lease. Missing leases are already dismissed.
pub fn dismiss(instance: InstanceId) -> std::io::Result<()> {
    dismiss_at(&super::presented_dir(), instance)
}

fn dismiss_at(directory: &Path, instance: InstanceId) -> std::io::Result<()> {
    with_lock(directory, || {
        remove_if_present(&directory.join(instance.to_string()))
    })
}

/// Aggregates live leases, removing entries at least 90 seconds old.
pub fn scan(directory: &Path, now: SystemTime) -> std::io::Result<LeaseState> {
    with_lock(directory, || scan_locked(directory, now))
}

fn scan_locked(directory: &Path, now: SystemTime) -> std::io::Result<LeaseState> {
    let mut presented = false;
    let mut next_expiry_in = None;
    for entry in std::fs::read_dir(directory)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if !file_type.is_file()
            || entry
                .file_name()
                .to_str()
                .and_then(|s| s.parse::<InstanceId>().ok())
                .is_none()
        {
            continue;
        }
        let metadata = match entry.metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let modified = metadata.modified()?.min(now);
        let age = now.duration_since(modified).unwrap_or(Duration::ZERO);
        if age >= LEASE_LIFETIME {
            remove_if_present(&entry.path())?;
        } else {
            presented = true;
            let remaining = LEASE_LIFETIME.saturating_sub(age);
            next_expiry_in =
                Some(next_expiry_in.map_or(remaining, |current: Duration| current.min(remaining)));
        }
    }
    Ok(LeaseState {
        presented,
        next_expiry_in,
    })
}

fn with_lock<T>(
    directory: &Path,
    operation: impl FnOnce() -> std::io::Result<T>,
) -> std::io::Result<T> {
    std::fs::create_dir_all(directory)?;
    let file = open_lock(directory)?;
    let _guard = Flock::lock(file, FlockArg::LockExclusive)
        .map_err(|(_file, errno)| std::io::Error::from_raw_os_error(errno as i32))?;
    operation()
}

fn open_lock(directory: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.with_extension("lock"))
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

/// Resolves one instance's lease path under an explicit test/runtime root.
#[cfg(test)]
pub(crate) fn lease_path(directory: &Path, instance: InstanceId) -> std::path::PathBuf {
    directory.join(instance.to_string())
}

#[cfg(test)]
mod tests;
