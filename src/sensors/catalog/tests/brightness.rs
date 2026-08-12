use super::*;
use crate::scheduler::OwnerId;

fn brightness_config() -> Config {
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(&["screen_brightness"]);
    cfg.tooltip.sections.clear();
    cfg.pages.order.clear();
    cfg
}

fn plan(cfg: &Config, has_backlight: bool, generation: u64) -> crate::scheduler::SchedulerConfig {
    scheduler_config(
        cfg,
        &HardwareInventory {
            has_backlight,
            ..HardwareInventory::default()
        },
        Path::new("/missing-proc"),
        ConfigGeneration(generation),
    )
}

#[test]
fn absent_backlight_keeps_inventory_but_omits_brightness_and_external_files_unchanged() {
    let mut cfg = brightness_config();
    cfg.panel = surface(&["screen_brightness", "system_updates", "server_check"]);
    cfg.system_updates.file = String::from("/run/updates");
    cfg.server_check.file = String::from("/run/server");

    let configured = plan(&cfg, false, 1);

    assert!(
        configured
            .jobs
            .iter()
            .all(|spec| spec.id.kind != JobKind::Brightness)
    );
    assert!(
        configured
            .demand
            .hidden
            .iter()
            .all(|job| job.kind != JobKind::Brightness)
    );
    assert!(
        configured
            .demand
            .hidden
            .iter()
            .any(|job| { job.source == SourceIdentity::Inventory(InventoryFamily::Backlight) })
    );
    for kind in [JobKind::UpdatesFile, JobKind::ServerFile] {
        assert!(
            configured
                .jobs
                .iter()
                .any(|spec| { spec.id.kind == kind && spec.timing == TimingClass::Triggered })
        );
        assert!(configured.demand.hidden.iter().any(|job| job.kind == kind));
    }
}

#[test]
fn inventory_appearance_starts_brightness_and_removal_cancels_and_invalidates_it() {
    let cfg = brightness_config();
    let brightness = JobId::singleton(OwnerId::External, JobKind::Brightness);
    let mut scheduler = Scheduler::new();
    let _ = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: plan(&cfg, false, 1),
        inventory_generation: InventoryGeneration(1),
    });

    let appeared = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::ZERO,
        update: {
            let configured = plan(&cfg, true, 1);
            crate::scheduler::InventoryUpdate {
                generation: InventoryGeneration(2),
                jobs: configured.jobs,
                demand: configured.demand,
                resume_acknowledgement: None,
            }
        },
    });
    let running = appeared
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } if ticket.job == brightness => {
                Some(ticket.clone())
            }
            _ => None,
        })
        .expect("brightness starts immediately after appearance");

    let removed = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::ZERO,
        update: {
            let configured = plan(&cfg, false, 1);
            crate::scheduler::InventoryUpdate {
                generation: InventoryGeneration(3),
                jobs: configured.jobs,
                demand: configured.demand,
                resume_acknowledgement: None,
            }
        },
    });
    assert!(removed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::InvalidateJob { job, .. } if job == &brightness)
    }));
    assert!(removed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &running)
    }));
}
