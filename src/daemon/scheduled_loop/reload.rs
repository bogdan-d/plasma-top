use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_config(
    new_cfg: Config,
    discovered_hw: Option<HardwareInventory>,
    paths: &DaemonPaths,
    roots: &FilesystemRoots,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    control: &mut dyn LoopControl,
    boundaries: &mut DaemonBoundaries<'_>,
    command_lookup: &crate::page_commands::CommandLookup,
    boot: ClockSnapshot,
) -> Result<()> {
    let previous_updates_file = state.cfg.system_updates.file.clone();
    let previous_server_file = state.cfg.server_check.file.clone();
    let mut resolved_mounts = state.resolved_mounts.clone();
    if matches!(new_cfg.disks.mounts, crate::config::Mounts::Auto)
        && let Ok(mounts) = disk::try_resolve_mounts(&roots.proc_root, &new_cfg)
    {
        resolved_mounts = mounts;
    }
    let hardware_changed = discovered_hw
        .as_ref()
        .is_some_and(|discovered| discovered != &state.hw);
    let at = scheduler_time(control.snapshot());
    let elapsed = scheduler.handle(SchedulerEvent::TimeAdvanced { at });
    execute_transition(
        elapsed,
        scheduler,
        state,
        roots,
        paths,
        boundaries,
        control,
        command_lookup,
        boot,
    )?;

    state.cfg = new_cfg;
    if let Some(discovered_hw) = discovered_hw {
        state.hw = discovered_hw;
    }
    state.resolved_mounts = resolved_mounts;
    replace_page_registry(
        paths,
        &state.cfg,
        &mut state.active,
        &mut state.command_cache,
        &mut state.owners.process,
    )?;
    state.config_generation = state.config_generation.next();
    reset_external_stamp_if_path_changed(
        &previous_updates_file,
        &state.cfg.system_updates.file,
        &mut state.updates_stamp,
    );
    reset_external_stamp_if_path_changed(
        &previous_server_file,
        &state.cfg.server_check.file,
        &mut state.server_stamp,
    );
    let at = scheduler_time(control.snapshot());
    let changed = scheduler.handle(SchedulerEvent::ConfigChanged {
        at,
        config: state.scheduler_config(roots),
    });
    let inventory = hardware_changed.then(|| {
        state.inventory_generation = state.inventory_generation.next();
        let config = state.scheduler_config(roots);
        scheduler.handle(SchedulerEvent::InventoryChanged {
            at,
            update: crate::scheduler::InventoryUpdate {
                generation: state.inventory_generation,
                jobs: config.jobs,
                demand: config.demand,
                resume_acknowledgement: None,
            },
        })
    });
    execute_transition(
        changed,
        scheduler,
        state,
        roots,
        paths,
        boundaries,
        control,
        command_lookup,
        boot,
    )?;
    if let Some(inventory) = inventory {
        execute_transition(
            inventory,
            scheduler,
            state,
            roots,
            paths,
            boundaries,
            control,
            command_lookup,
            boot,
        )?;
    }
    Ok(())
}
