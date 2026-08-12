use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn check(
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    cpu_count: usize,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
    owner_messages: &mut VecDeque<(OwnerId, OwnerMessage)>,
) -> Result<bool> {
    if state.profile.is_none() {
        cache_live_geom();
    }
    let new_cfg = match state::load_replacement(config_path) {
        Ok(cfg) => cfg,
        Err(error) => {
            eprintln!("[reload] config reload failed, keeping previous: {error}");
            return Ok(false);
        }
    };
    let discovery =
        discover_local_hardware_attempt(&roots.sys_root, &roots.proc_root, &new_cfg, cpu_count);
    let mut hw = state.hw.clone();
    discovery.merge_into(&mut hw);
    if new_cfg == state.cfg && hw == state.hw {
        return Ok(false);
    }
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
    let transition = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: now(clock),
        config: state.scheduler_config(),
    });
    if let Some(profile) = &state.profile {
        profile.observe_config(
            state.config_generation,
            state.cfg.display.overlay,
            &transition,
        );
    }
    enqueue(actions, transition);
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
    request_selected_graph(scheduler, state, clock, actions);
    Ok(true)
}

pub(super) fn trigger_external_change(
    kind: JobKind,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
) {
    let config = state.scheduler_config();
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
