//! Event-driven configuration, style, and status file sources.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::adapters::ProductionClock;
use crate::domain::boundary::FilesystemRoots;
use crate::error::Result;
use crate::file_watch::{FileWatcher, WatchSource, WatchTarget};
use crate::scheduler::{JobKind, OwnerId, Scheduler, SchedulerAction, SchedulerEvent};

use super::state::RuntimeState;
use super::worker::OwnerMessage;
use super::{DaemonPaths, enqueue, now, reload, request_selected_graph};

const WATCH_STABILIZATION_LIMIT: usize = 8;

pub(super) fn watch_targets(
    config: &Path,
    machines: &[PathBuf],
    paths: &DaemonPaths,
    state: &RuntimeState,
) -> Vec<WatchTarget> {
    let mut targets = vec![
        WatchTarget::new(WatchSource::Config, config),
        WatchTarget::new(
            WatchSource::Config,
            crate::config::assets::xdg_dir().join("config.toml"),
        ),
        WatchTarget::new(WatchSource::PlasmaConfig, &paths.plasma_config),
        WatchTarget::new(WatchSource::Theme, &paths.kdeglobals),
        WatchTarget::new(WatchSource::Style, &state.css_path),
    ];
    if let Some(name) = state.css_path.file_name() {
        targets.push(WatchTarget::new(
            WatchSource::Style,
            crate::config::assets::xdg_dir().join("style").join(name),
        ));
    }
    targets.extend(
        machines
            .iter()
            .map(|path| WatchTarget::new(WatchSource::MachineConfig, path)),
    );
    if let Some(path) = &state.overlay_path {
        targets.push(WatchTarget::new(WatchSource::Style, path));
        if let Some(name) = path.file_name() {
            targets.push(WatchTarget::new(
                WatchSource::Style,
                crate::config::assets::xdg_dir().join("style").join(name),
            ));
        }
    }
    if !state.cfg.system_updates.file.is_empty() {
        targets.push(WatchTarget::new(
            WatchSource::Updates,
            &state.cfg.system_updates.file,
        ));
    }
    if !state.cfg.server_check.file.is_empty() {
        targets.push(WatchTarget::new(
            WatchSource::Server,
            &state.cfg.server_check.file,
        ));
    }
    targets
}

#[allow(clippy::too_many_arguments)]
pub(super) fn process_file_changes(
    changed: &BTreeSet<WatchSource>,
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    cpu_count: usize,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
    owner_messages: &mut VecDeque<(OwnerId, OwnerMessage)>,
) -> Result<()> {
    if changed.iter().any(|source| {
        matches!(
            source,
            WatchSource::Config | WatchSource::MachineConfig | WatchSource::PlasmaConfig
        )
    }) {
        reload::apply_config_change(
            config_path,
            roots,
            paths,
            cpu_count,
            scheduler,
            state,
            clock,
            actions,
            owner_messages,
        )?;
    }
    if changed.contains(&WatchSource::Updates) {
        reload::trigger_external_change(JobKind::UpdatesFile, scheduler, state, clock, actions);
    }
    if changed.contains(&WatchSource::Server) {
        reload::trigger_external_change(JobKind::ServerFile, scheduler, state, clock, actions);
    }
    if changed
        .iter()
        .any(|source| matches!(source, WatchSource::Theme | WatchSource::Style))
        && state.update_style(paths, true)
        && state.first_paint_published
    {
        enqueue(
            actions,
            scheduler.handle(SchedulerEvent::DisplayRefreshRequested { at: now(clock) }),
        );
        request_selected_graph(scheduler, state, clock, actions);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn process_and_stabilize(
    mut changed: BTreeSet<WatchSource>,
    watcher: &mut FileWatcher,
    watch_path: &Path,
    machine_paths: &[PathBuf],
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    cpu_count: usize,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
    owner_messages: &mut VecDeque<(OwnerId, OwnerMessage)>,
) -> Result<()> {
    for attempt in 0..=WATCH_STABILIZATION_LIMIT {
        process_file_changes(
            &changed,
            config_path,
            roots,
            paths,
            cpu_count,
            scheduler,
            state,
            clock,
            actions,
            owner_messages,
        )?;
        let Some(post_arm_sources) = watcher
            .set_targets(watch_targets(watch_path, machine_paths, paths, state))
            .map_err(|error| {
                crate::error::Error::Runtime(format!("file watch reconfiguration failed: {error}"))
            })?
        else {
            return Ok(());
        };
        if attempt == WATCH_STABILIZATION_LIMIT {
            return Err(crate::error::Error::Runtime(format!(
                "file watch targets did not stabilize after {WATCH_STABILIZATION_LIMIT} reconfigurations"
            )));
        }
        changed = post_arm_sources;
    }
    unreachable!("bounded watch stabilization loop always returns")
}
