use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::config::{
    Config, apply_canonical_width, cache_live_geom, default_config_path, load_config,
    machine_source_paths, resolve_style,
};
use crate::domain::boundary::{ClockSnapshot, FilesystemRoots, IoEvent};
use crate::domain::readings::{DisplaySnapshot, HardwareInventory, InventoryFamily};
use crate::domain::state::NotificationState;
use crate::error::Result;
use crate::notify::check_and_notify;
use crate::page_commands::{
    Page, PageCommandAttempt, PageCommandCache, PageSource, attempt_command_with_state_and_clock,
};
use crate::render::PanelFormatter;
use crate::scheduler::{
    CompletionKind, ConfigGeneration, InventoryGeneration, JobId, JobKind, PageId, PublicationId,
    RefreshTrigger, RescanKind, RunId, Scheduler, SchedulerEvent, SchedulerTime, SourceIdentity,
    Transition,
};
use crate::sensors::process::ProcessState;
use crate::sensors::{
    OwnerRefs, cpu, discover_hardware_attempt, discover_local_hardware,
    discover_local_hardware_attempt, disk, external, gpu_history, gpu_intel, gpu_nvidia,
    invalidate_scheduled_job, memory, network, power, rescan_peripherals, reset_counter_baseline,
    scheduler_config,
};

use super::{
    DaemonBoundaries, DaemonPaths, LoopControl, PAGE_WAKE_INTERVAL, cleanup, clock_unix,
    executable_lookup, is_light_rgb, kdeglobals_background, log_boot_ready, mtime, page_index,
    plasma_is_light, publish_pages, read_css, render_page_cached, replace_page_registry,
    write_atomic,
};

#[path = "scheduled_loop/executor.rs"]
mod executor;
use executor::{ActionQueue, execute_transition};

#[path = "scheduled_loop/publication.rs"]
mod publication;
#[path = "scheduled_loop/reload.rs"]
mod reload;

#[path = "scheduled_loop/state.rs"]
mod state;
use state::{Owners, RuntimeState};

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub(super) fn run(
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    poll_limit: Option<usize>,
) -> Result<()> {
    fs::create_dir_all(&paths.runtime)?;
    fs::create_dir_all(&paths.state)?;
    cleanup(paths);
    write_atomic(&paths.page, "0")?;

    let boot = control.snapshot();
    let cfg = load_config(config_path, None)?;
    let cpu_count = std::thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let hw = discover_local_hardware(&roots.sys_root, &roots.proc_root, &cfg, cpu_count);
    let active = publish_pages(paths, &cfg)?;
    let watch_path = config_path.map_or_else(default_config_path, Path::to_path_buf);
    let machine_paths = machine_source_paths(config_path);
    let light = kdeglobals_background(&paths.kdeglobals).is_some_and(is_light_rgb);
    let css_path = resolve_style(if light {
        "style-light.css"
    } else {
        "style-dark.css"
    });
    let overlay_path = cfg
        .display
        .overlay
        .then(|| resolve_style("style-overlay.css"));
    let css = read_css(&css_path, overlay_path.as_deref());
    let resolved_mounts = if matches!(cfg.disks.mounts, crate::config::Mounts::Auto) {
        disk::try_resolve_mounts(&roots.proc_root, &cfg).unwrap_or_else(|_| vec![String::from("/")])
    } else {
        vec![String::from("/")]
    };
    let mut state = RuntimeState {
        config_stamp: mtime(&watch_path),
        machine_stamps: machine_paths.iter().map(|path| mtime(path)).collect(),
        plasma_stamp: mtime(&paths.plasma_config),
        geom_stamp: mtime(&paths.geom),
        kde_stamp: mtime(&paths.kdeglobals),
        css_stamp: mtime(&css_path),
        overlay_stamp: overlay_path.as_deref().and_then(mtime),
        updates_stamp: nonempty_mtime(&cfg.system_updates.file),
        server_stamp: nonempty_mtime(&cfg.server_check.file),
        cfg,
        hw,
        active,
        owners: Owners::new(),
        readings: DisplaySnapshot::default(),
        notifications: NotificationState::default(),
        notification_samples: BTreeMap::new(),
        command_cache: PageCommandCache::new(),
        config_generation: ConfigGeneration(1),
        inventory_generation: InventoryGeneration(1),
        css,
        light,
        css_path,
        overlay_path,
        panel_html: None,
        tooltip_html: None,
        display_publication_count: 0,
        first_paint_published: false,
        theme_reconciliation_pending: true,
        shutdown_requested: false,
        action_queue: ActionQueue::default(),
        resolved_mounts,
        boot_pending: BTreeSet::from([
            "battery_sys",
            "battery_mouse",
            "battery_kbd",
            "hd_temps",
            "fan_speeds",
            "gpu_nvidia",
            "gpu_intel",
            "system_updates",
            "server_check",
            "top_process",
        ]),
        terminate: false,
    };
    let command_lookup = executable_lookup();
    let mut scheduler = Scheduler::new();
    let startup = scheduler.handle(SchedulerEvent::Startup {
        at: scheduler_time(boot),
        config: state.scheduler_config(roots),
        inventory_generation: state.inventory_generation,
    });
    execute_transition(
        startup,
        &mut scheduler,
        &mut state,
        roots,
        paths,
        boundaries,
        control,
        &command_lookup,
        boot,
    )?;
    let presented = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: scheduler_time(control.snapshot()),
        presented: true,
    });
    execute_transition(
        presented,
        &mut scheduler,
        &mut state,
        roots,
        paths,
        boundaries,
        control,
        &command_lookup,
        boot,
    )?;
    let (_, selected) = state.selected_page(paths);
    let selected = scheduler.handle(SchedulerEvent::SelectedPageChanged {
        at: scheduler_time(control.snapshot()),
        page: selected,
    });
    execute_transition(
        selected,
        &mut scheduler,
        &mut state,
        roots,
        paths,
        boundaries,
        control,
        &command_lookup,
        boot,
    )?;
    cache_live_geom();

    while !control.should_stop()
        && !state.terminate
        && poll_limit.is_none_or(|limit| state.display_publication_count < limit)
    {
        process_io_events(
            roots,
            paths,
            boundaries,
            control,
            &mut scheduler,
            &mut state,
            &command_lookup,
            boot,
        )?;
        let transition = scheduler.handle(SchedulerEvent::TimeAdvanced {
            at: scheduler_time(control.snapshot()),
        });
        execute_transition(
            transition,
            &mut scheduler,
            &mut state,
            roots,
            paths,
            boundaries,
            control,
            &command_lookup,
            boot,
        )?;
        reload_and_refresh(
            config_path,
            roots,
            paths,
            boundaries,
            control,
            cpu_count,
            &watch_path,
            &machine_paths,
            &mut scheduler,
            &mut state,
            &command_lookup,
            boot,
        )?;
        if control.should_stop() || state.terminate {
            break;
        }
        let now = scheduler_time(control.snapshot());
        let sleep = scheduler.next_wake().map_or(PAGE_WAKE_INTERVAL, |wake| {
            wake.duration()
                .saturating_sub(now.duration())
                .min(PAGE_WAKE_INTERVAL)
        });
        if !sleep.is_zero() {
            control.sleep(sleep);
        }
    }
    let shutdown = scheduler.handle(SchedulerEvent::Shutdown {
        at: scheduler_time(control.snapshot()),
    });
    execute_transition(
        shutdown,
        &mut scheduler,
        &mut state,
        roots,
        paths,
        boundaries,
        control,
        &command_lookup,
        boot,
    )?;
    cleanup(paths);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn process_io_events(
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    command_lookup: &crate::page_commands::CommandLookup,
    boot: ClockSnapshot,
) -> Result<()> {
    for event in control.drain_io_events() {
        match event {
            IoEvent::PrepareForSleep(true) => {
                let transition = scheduler.handle(SchedulerEvent::Suspend {
                    at: scheduler_time(control.snapshot()),
                });
                execute_transition(
                    transition,
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
            IoEvent::PrepareForSleep(false) => {
                let transition = scheduler.handle(SchedulerEvent::Resume {
                    at: scheduler_time(control.snapshot()),
                });
                execute_transition(
                    transition,
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
            IoEvent::UpowerChanged => {
                let jobs = state
                    .scheduler_config(roots)
                    .jobs
                    .into_iter()
                    .filter(|spec| {
                        matches!(
                            spec.id.kind,
                            JobKind::SystemBattery | JobKind::PeripheralBattery
                        ) || matches!(
                            spec.id.source,
                            SourceIdentity::Inventory(
                                InventoryFamily::SystemBattery | InventoryFamily::Peripheral
                            )
                        )
                    })
                    .map(|spec| spec.id)
                    .collect::<Vec<_>>();
                for job in jobs {
                    let transition = scheduler.handle(SchedulerEvent::RefreshTriggered {
                        at: scheduler_time(control.snapshot()),
                        job,
                        trigger: RefreshTrigger::PeripheralChanged,
                    });
                    execute_transition(
                        transition,
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
            }
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn reload_and_refresh(
    config_path: Option<&Path>,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    cpu_count: usize,
    watch_path: &Path,
    machine_paths: &[PathBuf],
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    command_lookup: &crate::page_commands::CommandLookup,
    boot: ClockSnapshot,
) -> Result<()> {
    let new_config_stamp = mtime(watch_path);
    let new_machine_stamps = machine_paths
        .iter()
        .map(|path| mtime(path))
        .collect::<Vec<_>>();
    if new_config_stamp != state.config_stamp || new_machine_stamps != state.machine_stamps {
        state.config_stamp = new_config_stamp;
        state.machine_stamps = new_machine_stamps;
        match load_config(config_path, None) {
            Ok(new_cfg) => {
                let before_hw = state.hw.clone();
                let discovery = discover_local_hardware_attempt(
                    &roots.sys_root,
                    &roots.proc_root,
                    &new_cfg,
                    cpu_count,
                );
                let mut discovered_hw = before_hw;
                discovery.merge_into(&mut discovered_hw);
                reload::apply_config(
                    new_cfg,
                    Some(discovered_hw),
                    paths,
                    roots,
                    scheduler,
                    state,
                    control,
                    boundaries,
                    command_lookup,
                    boot,
                )?;
            }
            Err(error) => eprintln!("[reload] config reload failed, keeping previous: {error}"),
        }
    }

    let new_plasma_stamp = mtime(&paths.plasma_config);
    let new_geom_stamp = mtime(&paths.geom);
    if new_plasma_stamp != state.plasma_stamp || new_geom_stamp != state.geom_stamp {
        state.plasma_stamp = new_plasma_stamp;
        state.geom_stamp = new_geom_stamp;
        cache_live_geom();
        match load_config(config_path, None) {
            Ok(new_cfg) => reload::apply_config(
                new_cfg,
                None,
                paths,
                roots,
                scheduler,
                state,
                control,
                boundaries,
                command_lookup,
                boot,
            )?,
            Err(error) => {
                eprintln!("[reload] plasma-triggered reload failed, keeping previous: {error}")
            }
        }
    }

    let new_kde_stamp = mtime(&paths.kdeglobals);
    if new_kde_stamp != state.kde_stamp {
        state.kde_stamp = new_kde_stamp;
        state.theme_reconciliation_pending = true;
        if let Some(background) = kdeglobals_background(&paths.kdeglobals) {
            apply_theme(
                is_light_rgb(background),
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
    }
    let wanted_overlay = state
        .cfg
        .display
        .overlay
        .then(|| resolve_style("style-overlay.css"));
    if wanted_overlay != state.overlay_path {
        state.overlay_path = wanted_overlay;
        state.overlay_stamp = state.overlay_path.as_deref().and_then(mtime);
        state.css = read_css(&state.css_path, state.overlay_path.as_deref());
        request_display(
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
    let new_css_stamp = mtime(&state.css_path);
    let new_overlay_stamp = state.overlay_path.as_deref().and_then(mtime);
    if new_css_stamp != state.css_stamp || new_overlay_stamp != state.overlay_stamp {
        state.css_stamp = new_css_stamp;
        state.overlay_stamp = new_overlay_stamp;
        state.css = read_css(&state.css_path, state.overlay_path.as_deref());
        request_display(
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

    let (_, selected) = state.selected_page(paths);
    let page = scheduler.handle(SchedulerEvent::SelectedPageChanged {
        at: scheduler_time(control.snapshot()),
        page: selected,
    });
    execute_transition(
        page,
        scheduler,
        state,
        roots,
        paths,
        boundaries,
        control,
        command_lookup,
        boot,
    )?;
    trigger_file_changes(
        scheduler,
        state,
        roots,
        paths,
        boundaries,
        control,
        command_lookup,
        boot,
    )?;
    reconcile_command_theme(
        scheduler,
        state,
        roots,
        paths,
        boundaries,
        control,
        command_lookup,
        boot,
    )?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn reconcile_command_theme(
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    command_lookup: &crate::page_commands::CommandLookup,
    boot: ClockSnapshot,
) -> Result<()> {
    if !state.first_paint_published || !state.theme_reconciliation_pending {
        return Ok(());
    }
    state.theme_reconciliation_pending = false;
    let light = plasma_is_light(boundaries.commands, &paths.kdeglobals);
    apply_theme(
        light,
        scheduler,
        state,
        roots,
        paths,
        boundaries,
        control,
        command_lookup,
        boot,
    )
}

#[allow(clippy::too_many_arguments)]
fn apply_theme(
    light: bool,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    command_lookup: &crate::page_commands::CommandLookup,
    boot: ClockSnapshot,
) -> Result<()> {
    if light == state.light {
        return Ok(());
    }
    state.light = light;
    state.css_path = resolve_style(if light {
        "style-light.css"
    } else {
        "style-dark.css"
    });
    state.css_stamp = mtime(&state.css_path);
    state.css = read_css(&state.css_path, state.overlay_path.as_deref());
    request_display(
        scheduler,
        state,
        roots,
        paths,
        boundaries,
        control,
        command_lookup,
        boot,
    )
}

#[allow(clippy::too_many_arguments)]
fn request_display(
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    command_lookup: &crate::page_commands::CommandLookup,
    boot: ClockSnapshot,
) -> Result<()> {
    let refresh = scheduler.handle(SchedulerEvent::DisplayRefreshRequested {
        at: scheduler_time(control.snapshot()),
    });
    execute_transition(
        refresh,
        scheduler,
        state,
        roots,
        paths,
        boundaries,
        control,
        command_lookup,
        boot,
    )
}

#[allow(clippy::too_many_arguments)]
fn trigger_file_changes(
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    command_lookup: &crate::page_commands::CommandLookup,
    boot: ClockSnapshot,
) -> Result<()> {
    for kind in changed_external_file_jobs(
        &state.cfg,
        &mut state.updates_stamp,
        &mut state.server_stamp,
    )
    .into_iter()
    .flatten()
    {
        trigger_file_job(
            kind,
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

fn changed_external_file_jobs(
    cfg: &Config,
    updates_stamp: &mut Option<SystemTime>,
    server_stamp: &mut Option<SystemTime>,
) -> [Option<JobKind>; 2] {
    let new_updates_stamp = nonempty_mtime(&cfg.system_updates.file);
    let updates_changed = new_updates_stamp != *updates_stamp;
    *updates_stamp = new_updates_stamp;
    let new_server_stamp = nonempty_mtime(&cfg.server_check.file);
    let server_changed = new_server_stamp != *server_stamp;
    *server_stamp = new_server_stamp;
    [
        updates_changed.then_some(JobKind::UpdatesFile),
        server_changed.then_some(JobKind::ServerFile),
    ]
}

fn reset_external_stamp_if_path_changed(
    previous_path: &str,
    current_path: &str,
    stamp: &mut Option<SystemTime>,
) {
    if current_path != previous_path {
        *stamp = nonempty_mtime(current_path);
    }
}

#[allow(clippy::too_many_arguments)]
fn trigger_file_job(
    kind: JobKind,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    boundaries: &mut DaemonBoundaries<'_>,
    control: &mut dyn LoopControl,
    command_lookup: &crate::page_commands::CommandLookup,
    boot: ClockSnapshot,
) -> Result<()> {
    let config = state.scheduler_config(roots);
    let Some(job) = config.jobs.into_iter().find(|spec| spec.id.kind == kind) else {
        return Ok(());
    };
    let triggered = scheduler.handle(SchedulerEvent::RefreshTriggered {
        at: scheduler_time(control.snapshot()),
        job: job.id,
        trigger: RefreshTrigger::FileChanged,
    });
    execute_transition(
        triggered,
        scheduler,
        state,
        roots,
        paths,
        boundaries,
        control,
        command_lookup,
        boot,
    )
}

fn rescan(
    kind: RescanKind,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    boundaries: &mut DaemonBoundaries<'_>,
) {
    match kind {
        RescanKind::Hardware | RescanKind::VolatileInventoryAndRoute => {
            let cpu_count = state.hw.cpu_count.max(1);
            discover_hardware_attempt(
                &roots.sys_root,
                &roots.proc_root,
                &state.cfg,
                boundaries.dbus,
                boundaries.commands,
                cpu_count,
            )
            .merge_into(&mut state.hw);
        }
        RescanKind::Peripherals => rescan_peripherals(
            &mut state.hw,
            &state.cfg,
            boundaries.dbus,
            boundaries.commands,
        ),
    }
}

fn page_for_job(job: &JobId, active: &[Page]) -> Option<Page> {
    let SourceIdentity::Page(page_id) = &job.source else {
        return None;
    };
    active.iter().copied().find(|page| {
        PageId::from_id(page.id) == *page_id && matches!(page.source, PageSource::Command(_))
    })
}

fn scheduler_time(snapshot: ClockSnapshot) -> SchedulerTime {
    SchedulerTime::from_duration(snapshot.monotonic)
}

#[cfg(test)]
#[path = "scheduled_loop/tests.rs"]
mod tests;

fn nonempty_mtime(path: &str) -> Option<SystemTime> {
    (!path.is_empty()).then(|| mtime(Path::new(path))).flatten()
}
