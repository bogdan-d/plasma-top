use super::*;

fn disk_io_config() -> Config {
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(&["disk_io"]);
    cfg.tooltip.sections.clear();
    cfg.pages.order.clear();
    cfg
}

fn plan(cfg: &Config, device: Option<&str>) -> crate::scheduler::SchedulerConfig {
    scheduler_config(
        cfg,
        &HardwareInventory {
            disk_io_device: device.map(String::from),
            ..HardwareInventory::default()
        },
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    )
}

fn inventory_update(
    generation: u64,
    configured: crate::scheduler::SchedulerConfig,
) -> crate::scheduler::InventoryUpdate {
    crate::scheduler::InventoryUpdate {
        generation: InventoryGeneration(generation),
        jobs: configured.jobs,
        demand: configured.demand,
        resume_acknowledgement: None,
    }
}

#[test]
fn captured_disk_io_absence_waits_for_normal_inventory_cadence() {
    let cfg = disk_io_config();
    let mut scheduler = Scheduler::new();
    let startup = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: plan(&cfg, None),
        inventory_generation: InventoryGeneration(1),
    });
    let inventory = startup
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket }
                if ticket.job.source == SourceIdentity::Inventory(InventoryFamily::DiskIo) =>
            {
                Some(ticket.clone())
            }
            _ => None,
        })
        .expect("initial disk-I/O inventory attempt");

    let captured = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::ZERO,
        ticket: inventory,
        completion: crate::scheduler::CompletionKind::Captured,
        notification_ready: false,
    });
    assert!(
        captured
            .actions
            .iter()
            .all(|action| { !matches!(action, SchedulerAction::ApplyBackoff { .. }) })
    );
    let early = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_millis(4_200)),
    });
    assert!(early.actions.iter().all(|action| {
        !matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::HardwareDiscovery)
    }));
    let due = scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_secs(60)),
    });
    assert!(due.actions.iter().any(|action| {
        matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::HardwareDiscovery)
    }));
}

#[test]
fn disk_io_source_appearance_change_and_removal_reconcile_by_identity() {
    let cfg = disk_io_config();
    let mut scheduler = Scheduler::new();
    let startup = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: plan(&cfg, None),
        inventory_generation: InventoryGeneration(1),
    });
    assert!(startup.actions.iter().all(|action| {
        !matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::DiskIo)
    }));

    let appeared = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::ZERO,
        update: inventory_update(2, plan(&cfg, Some("sda"))),
    });
    let sda = appeared
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::DiskIo => {
                Some(ticket.clone())
            }
            _ => None,
        })
        .expect("appearing disk-I/O source starts immediately");
    assert_eq!(sda.job.source, SourceIdentity::Device(String::from("sda")));

    let changed = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::ZERO,
        update: inventory_update(3, plan(&cfg, Some("nvme0n1"))),
    });
    assert!(changed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::InvalidateJob { job, .. } if job == &sda.job)
    }));
    assert!(changed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &sda)
    }));

    let stale = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::ZERO,
        ticket: sda,
        completion: crate::scheduler::CompletionKind::Captured,
        notification_ready: false,
    });
    let nvme = stale
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::DiskIo => {
                Some(ticket.clone())
            }
            _ => None,
        })
        .expect("replacement starts after obsolete completion releases owner");
    assert_eq!(
        nvme.job.source,
        SourceIdentity::Device(String::from("nvme0n1"))
    );

    let removed = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::ZERO,
        update: inventory_update(4, plan(&cfg, None)),
    });
    assert!(removed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::InvalidateJob { job, .. } if job == &nvme.job)
    }));
    assert!(removed.actions.iter().any(|action| {
        matches!(action, SchedulerAction::CancelJob { ticket, .. } if ticket == &nvme)
    }));
}
