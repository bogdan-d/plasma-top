use std::path::PathBuf;

use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    cpu_count: usize,
    watch_path: &Path,
    machine_paths: &[PathBuf],
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
    owner_messages: &mut VecDeque<(OwnerId, OwnerMessage)>,
) -> Result<()> {
    let config_stamp = mtime(watch_path);
    let machine_stamps = machine_paths
        .iter()
        .map(|path| mtime(path))
        .collect::<Vec<_>>();
    let plasma_stamp = mtime(&paths.plasma_config);
    let geom_stamp = mtime(&paths.geom);
    let changed_config = config_stamp != state.config_stamp
        || machine_stamps != state.machine_stamps
        || plasma_stamp != state.plasma_stamp
        || geom_stamp != state.geom_stamp;
    state.config_stamp = config_stamp;
    state.machine_stamps = machine_stamps;
    state.plasma_stamp = plasma_stamp;
    state.geom_stamp = geom_stamp;
    if !changed_config {
        return Ok(());
    }
    cache_live_geom();
    let new_cfg = match state::load_replacement(config_path) {
        Ok(cfg) => cfg,
        Err(error) => {
            eprintln!("[reload] config reload failed, keeping previous: {error}");
            return Ok(());
        }
    };
    let discovery =
        discover_local_hardware_attempt(&roots.sys_root, &roots.proc_root, &new_cfg, cpu_count);
    let mut hw = state.hw.clone();
    discovery.merge_into(&mut hw);
    let hardware_changed = hw != state.hw;
    let pages = state::build_replacement_pages(&new_cfg);
    write_atomic(&paths.npages, &pages.len().to_string())?;
    state.active = pages.clone();
    state.command_cache.retain_pages(&pages);
    for owner in [OwnerId::Page, OwnerId::Process] {
        if !queue_owner_message(
            owner_messages,
            (owner, OwnerMessage::PagesChanged(pages.clone())),
        ) {
            return Err(Error::Runtime(
                "bounded owner pending queue is full".to_owned(),
            ));
        }
    }
    state.cfg = new_cfg;
    state.hw = hw;
    state.config_generation = state.config_generation.next();
    state.rendered_graph = None;
    state.render_generation = state.render_generation.saturating_add(1);
    enqueue(
        actions,
        scheduler.handle(SchedulerEvent::ConfigChanged {
            at: now(clock),
            config: state.scheduler_config(),
        }),
    );
    if hardware_changed {
        state.inventory_generation = state.inventory_generation.next();
        let config = state.scheduler_config();
        enqueue(
            actions,
            scheduler.handle(SchedulerEvent::InventoryChanged {
                at: now(clock),
                update: InventoryUpdate {
                    generation: state.inventory_generation,
                    jobs: config.jobs,
                    demand: config.demand,
                    resume_acknowledgement: None,
                },
            }),
        );
    }
    state.refresh_deferred_first_paint_jobs();
    if matches!(state.cfg.disks.mounts, crate::config::Mounts::Auto)
        && let Some(job) = state
            .scheduler_config()
            .jobs
            .into_iter()
            .find(|spec| spec.id.kind == JobKind::MountInventory)
            .map(|spec| spec.id)
    {
        enqueue(
            actions,
            scheduler.handle(SchedulerEvent::RefreshTriggered {
                at: now(clock),
                job,
                trigger: RefreshTrigger::Signal,
            }),
        );
    }
    request_selected_graph(scheduler, state, paths, clock, actions);
    Ok(())
}

pub(super) fn trigger_external_changes(
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
) {
    let updates = state::nonempty_mtime(&state.cfg.system_updates.file);
    let server = state::nonempty_mtime(&state.cfg.server_check.file);
    let changed = [
        (updates != state.updates_stamp).then_some(JobKind::UpdatesFile),
        (server != state.server_stamp).then_some(JobKind::ServerFile),
    ];
    state.updates_stamp = updates;
    state.server_stamp = server;
    let config = state.scheduler_config();
    for kind in changed.into_iter().flatten() {
        if let Some(job) = config.jobs.iter().find(|spec| spec.id.kind == kind) {
            enqueue(
                actions,
                scheduler.handle(SchedulerEvent::RefreshTriggered {
                    at: now(clock),
                    job: job.id.clone(),
                    trigger: RefreshTrigger::FileChanged,
                }),
            );
        }
    }
}
