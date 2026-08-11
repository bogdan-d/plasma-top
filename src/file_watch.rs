//! Linux inotify integration for daemon logical sources.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::os::fd::{AsFd, AsRawFd, RawFd};
use std::path::{Path, PathBuf};
use std::time::Duration;

use nix::errno::Errno;
use nix::sys::inotify::{AddWatchFlags, InitFlags, Inotify, WatchDescriptor};
use tokio::io::unix::AsyncFd;

const DEBOUNCE: Duration = Duration::from_millis(50);

const WATCH_MASK: AddWatchFlags = AddWatchFlags::IN_CLOSE_WRITE
    .union(AddWatchFlags::IN_CREATE)
    .union(AddWatchFlags::IN_DELETE)
    .union(AddWatchFlags::IN_MOVED_FROM)
    .union(AddWatchFlags::IN_MOVED_TO)
    .union(AddWatchFlags::IN_ATTRIB)
    .union(AddWatchFlags::IN_DELETE_SELF)
    .union(AddWatchFlags::IN_MOVE_SELF)
    .union(AddWatchFlags::IN_UNMOUNT);

/// Typed daemon input changed by a watched path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum WatchSource {
    Config,
    MachineConfig,
    PlasmaConfig,
    Geometry,
    Theme,
    Style,
    Page,
    Updates,
    Server,
    Presentation,
}

/// One exact path assigned to a logical source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WatchTarget {
    pub(crate) source: WatchSource,
    pub(crate) path: PathBuf,
}

impl WatchTarget {
    pub(crate) fn new(source: WatchSource, path: impl Into<PathBuf>) -> Self {
        Self {
            source,
            path: path.into(),
        }
    }
}

/// Nonblocking inotify descriptor registered with Tokio's reactor.
pub(crate) struct FileWatcher {
    fd: AsyncFd<InotifyFd>,
    targets: Vec<WatchTarget>,
    directories: BTreeMap<WatchDescriptor, PathBuf>,
    pending: BTreeSet<WatchSource>,
    debounce_until: Option<tokio::time::Instant>,
    recover: bool,
}

impl FileWatcher {
    /// Installs all required watches, reporting the exact logical path on failure.
    pub(crate) fn new(targets: Vec<WatchTarget>) -> io::Result<Self> {
        let inotify =
            Inotify::init(InitFlags::IN_CLOEXEC | InitFlags::IN_NONBLOCK).map_err(nix_error)?;
        let fd = AsyncFd::new(InotifyFd(inotify))?;
        let mut watcher = Self {
            fd,
            targets: normalize_targets(targets)?,
            directories: BTreeMap::new(),
            pending: BTreeSet::new(),
            debounce_until: None,
            recover: false,
        };
        watcher.install_watches()?;
        Ok(watcher)
    }

    /// Replaces the logical targets after successful source resolution changes.
    pub(crate) fn set_targets(
        &mut self,
        targets: Vec<WatchTarget>,
    ) -> io::Result<Option<BTreeSet<WatchSource>>> {
        let targets = normalize_targets(targets)?;
        if targets != self.targets {
            *self = Self::new(targets)?;
            return Ok(Some(self.rescan_sources()));
        }
        Ok(None)
    }

    /// Returns every installed logical source for a post-arm state rescan.
    pub(crate) fn rescan_sources(&self) -> BTreeSet<WatchSource> {
        self.targets.iter().map(|target| target.source).collect()
    }

    /// Waits for one debounced set of logical changes.
    pub(crate) async fn changed(&mut self) -> io::Result<BTreeSet<WatchSource>> {
        if self.pending.is_empty() && !self.recover {
            loop {
                let mut ready = self.fd.readable().await?;
                match ready.try_io(|fd| fd.get_ref().0.read_events().map_err(nix_error)) {
                    Ok(events) => {
                        let mut changed = BTreeSet::new();
                        let mut recover = false;
                        self.classify(events?, &mut changed, &mut recover);
                        self.pending.extend(changed);
                        self.recover |= recover;
                    }
                    Err(_) => continue,
                }
                if !self.pending.is_empty() || self.recover {
                    self.debounce_until = Some(tokio::time::Instant::now() + DEBOUNCE);
                    break;
                }
            }
        }
        if let Some(deadline) = self.debounce_until {
            tokio::time::sleep_until(deadline).await;
        }
        loop {
            match self.fd.get_ref().0.read_events() {
                Ok(events) => {
                    let mut changed = BTreeSet::new();
                    let mut recover = false;
                    self.classify(events, &mut changed, &mut recover);
                    self.pending.extend(changed);
                    self.recover |= recover;
                }
                Err(Errno::EAGAIN) => break,
                Err(error) => return Err(nix_error(error)),
            }
        }
        let recover = self.recover;
        let changed = std::mem::take(&mut self.pending);
        self.recover = false;
        self.debounce_until = None;
        let mut changed = changed;
        if recover {
            changed.extend(self.full_rescan_and_rearm()?);
        } else if self.needs_rearm()? {
            *self = Self::new(self.targets.clone())?;
        }
        Ok(changed)
    }

    fn classify(
        &self,
        events: Vec<nix::sys::inotify::InotifyEvent>,
        changed: &mut BTreeSet<WatchSource>,
        recover: &mut bool,
    ) {
        for event in events {
            if is_recovery_mask(event.mask) {
                *recover = true;
            }
            let Some(directory) = self.directories.get(&event.wd) else {
                continue;
            };
            let affected = event
                .name
                .as_ref()
                .map_or_else(|| directory.clone(), |name| directory.join(name));
            changed.extend(self.targets.iter().filter_map(|target| {
                paths_related(&affected, &target.path).then_some(target.source)
            }));
        }
    }

    fn install_watches(&mut self) -> io::Result<()> {
        for target in &self.targets {
            for directory in watch_directories(&target.path).map_err(|error| {
                io::Error::new(
                    error.kind(),
                    format!("cannot watch {}: {error}", target.path.display()),
                )
            })? {
                let descriptor = self
                    .fd
                    .get_ref()
                    .0
                    .add_watch(&directory, WATCH_MASK)
                    .map_err(|error| {
                        io::Error::other(format!(
                            "cannot watch {} via {}: {error}",
                            target.path.display(),
                            directory.display()
                        ))
                    })?;
                self.directories.insert(descriptor, directory);
            }
        }
        Ok(())
    }

    fn needs_rearm(&self) -> io::Result<bool> {
        let current = self.directories.values().collect::<BTreeSet<_>>();
        let wanted = self
            .targets
            .iter()
            .map(|target| watch_directories(&target.path))
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<BTreeSet<_>>();
        Ok(current.len() != wanted.len() || wanted.iter().any(|path| !current.contains(path)))
    }

    fn full_rescan_and_rearm(&mut self) -> io::Result<BTreeSet<WatchSource>> {
        let sources = self.rescan_sources();
        *self = Self::new(self.targets.clone())?;
        Ok(sources)
    }

    #[cfg(test)]
    fn simulate_recovery(&mut self, mask: AddWatchFlags) -> io::Result<BTreeSet<WatchSource>> {
        if is_recovery_mask(mask) {
            self.full_rescan_and_rearm()
        } else {
            Ok(BTreeSet::new())
        }
    }
}

struct InotifyFd(Inotify);

impl AsRawFd for InotifyFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_fd().as_raw_fd()
    }
}

fn normalize_targets(targets: Vec<WatchTarget>) -> io::Result<Vec<WatchTarget>> {
    let current = std::env::current_dir()?;
    Ok(targets
        .into_iter()
        .map(|mut target| {
            if target.path.is_relative() {
                target.path = current.join(&target.path);
            }
            target
        })
        .collect())
}

fn watch_directories(path: &Path) -> io::Result<BTreeSet<PathBuf>> {
    let mut directories = BTreeSet::new();
    let parent = nearest_existing_directory(path.parent().unwrap_or(path))?;
    directories.insert(parent);
    if path.is_dir() {
        directories.insert(path.to_path_buf());
    }
    Ok(directories)
}

fn nearest_existing_directory(path: &Path) -> io::Result<PathBuf> {
    let mut candidate = path;
    loop {
        match candidate.metadata() {
            Ok(metadata) if metadata.is_dir() => return Ok(candidate.to_path_buf()),
            Ok(_) => {
                return Err(io::Error::other(format!(
                    "{} is not a directory",
                    candidate.display()
                )));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                candidate = candidate.parent().ok_or(error)?;
            }
            Err(error) => return Err(error),
        }
    }
}

fn is_recovery_mask(mask: AddWatchFlags) -> bool {
    mask.intersects(
        AddWatchFlags::IN_Q_OVERFLOW
            | AddWatchFlags::IN_IGNORED
            | AddWatchFlags::IN_DELETE_SELF
            | AddWatchFlags::IN_MOVE_SELF
            | AddWatchFlags::IN_UNMOUNT,
    )
}

fn paths_related(affected: &Path, target: &Path) -> bool {
    affected == target || affected.starts_with(target) || target.starts_with(affected)
}

fn nix_error(error: Errno) -> io::Error {
    io::Error::from_raw_os_error(error as i32)
}

#[cfg(test)]
mod tests;
