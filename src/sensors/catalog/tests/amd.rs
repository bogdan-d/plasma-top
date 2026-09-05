use super::*;
use crate::domain::Metric;
use crate::domain::readings::{AmdGpuMemoryPaths, AmdGpuSource};
use crate::scheduler::{CompletionKind, JobTicket, OwnerId, SchedulerConfig};

fn hardware() -> HardwareInventory {
    HardwareInventory {
        amd_gpu: Some(AmdGpuSource {
            pci_identity: String::from("0000:c3:00.0"),
            usage_path: Some("usage".into()),
            codec_usage_path: Some("codec".into()),
            memory_paths: Some(AmdGpuMemoryPaths {
                used: "used".into(),
                total: "total".into(),
            }),
            freq_path: Some("freq".into()),
            temp_path: Some("temp".into()),
            power_path: Some("power".into()),
            fan_speed_path: Some("fan".into()),
        }),
        ..HardwareInventory::default()
    }
}

fn config(panel: &[&str], tooltip: &[&str]) -> Config {
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(panel);
    cfg.tooltip = surface(tooltip);
    cfg.pages.order.clear();
    cfg
}

fn plan(cfg: &Config, hw: &HardwareInventory) -> SchedulerConfig {
    build_scheduler_config(cfg, hw, &[], ConfigGeneration(1))
}

fn starts(transition: crate::scheduler::Transition) -> Vec<JobTicket> {
    transition
        .actions
        .into_iter()
        .filter_map(|action| match action {
            SchedulerAction::StartJob { ticket } if ticket.job.owner == OwnerId::AmdGpu => {
                Some(ticket)
            }
            _ => None,
        })
        .collect()
}

fn startup(scheduler: &mut Scheduler, config: SchedulerConfig) -> Vec<JobTicket> {
    starts(scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config,
        inventory_generation: InventoryGeneration(1),
    }))
}

#[test]
fn every_amd_item_demands_only_its_supported_group_and_inventory() {
    for metric in crate::sensors::gpu_amd::FAST_METRICS
        .iter()
        .chain(crate::sensors::gpu_amd::SLOW_METRICS)
    {
        let cfg = config(&[metric.as_str()], &[]);
        let catalog = plan(&cfg, &hardware());
        assert_eq!(catalog.jobs.len(), 2);
        let sample = catalog
            .jobs
            .iter()
            .find(|spec| spec.amd.is_some())
            .expect("AMD job");
        let fast = crate::sensors::gpu_amd::FAST_METRICS.contains(metric);
        assert_eq!(
            sample.id.kind,
            if fast {
                JobKind::AmdFast
            } else {
                JobKind::AmdSlow
            }
        );
        assert_eq!(
            sample.timing,
            if fast {
                TimingClass::FastDisplay
            } else {
                TimingClass::Periodic
            }
        );
        assert_eq!(
            sample.freshness,
            if fast {
                cfg.display.poll_interval.duration()
            } else {
                Duration::from_secs(30)
            }
        );
        assert_eq!(
            sample.id.source,
            SourceIdentity::Device("amd:0000:c3:00.0".into())
        );
        assert_eq!(
            sample.amd.as_ref().expect("AMD demand").hidden,
            BTreeSet::from([*metric])
        );
        let discovery = catalog
            .jobs
            .iter()
            .find(|spec| spec.id.kind == JobKind::HardwareDiscovery)
            .expect("inventory");
        assert_eq!(
            discovery.id.source,
            SourceIdentity::Inventory(InventoryFamily::Amd)
        );
        assert_eq!(discovery.freshness, Duration::from_secs(60));
        let absent = plan(&cfg, &HardwareInventory::default());
        assert_eq!(absent.jobs.len(), 1);
        assert_eq!(absent.jobs[0].id.kind, JobKind::HardwareDiscovery);
    }
}

#[test]
fn hidden_fast_job_excludes_tooltip_fields_and_serializes_slow_sibling() {
    let cfg = config(
        &["gpu_amd_usage", "gpu_amd_temp"],
        &["gpu_amd_mem_usage", "gpu_amd_power"],
    );
    let mut scheduler = Scheduler::new();
    let tickets = startup(&mut scheduler, plan(&cfg, &hardware()));
    assert_eq!(tickets.len(), 1);
    assert_eq!(tickets[0].metrics, BTreeSet::from([Metric::GpuAmdUsage]));
    let next = starts(scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::ZERO,
        ticket: tickets[0].clone(),
        completion: CompletionKind::Captured,
        notification_ready: false,
    }));
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].metrics, BTreeSet::from([Metric::GpuAmdTemp]));
    scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::ZERO,
        ticket: next[0].clone(),
        completion: CompletionKind::Captured,
        notification_ready: false,
    });
    let activated = starts(scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::from_duration(Duration::from_secs(1)),
        presented: true,
    }));
    assert_eq!(activated.len(), 1);
    assert_eq!(
        activated[0].metrics,
        BTreeSet::from([Metric::GpuAmdUsage, Metric::GpuAmdMemUsage])
    );
    let slow = starts(scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::from_duration(Duration::from_secs(1)),
        ticket: activated[0].clone(),
        completion: CompletionKind::Captured,
        notification_ready: false,
    }));
    assert_eq!(slow.len(), 1);
    assert_eq!(
        slow[0].metrics,
        BTreeSet::from([Metric::GpuAmdTemp, Metric::GpuAmdPower])
    );
}

#[test]
fn notification_and_graph_demand_have_minimal_field_sets() {
    let hw = hardware();
    let mut cfg = config(&[], &[]);
    cfg.notifications.gpu_amd_temp = true;
    let notification = plan(&cfg, &hw);
    let job = notification
        .jobs
        .iter()
        .find_map(|spec| spec.amd.as_ref())
        .expect("notification job");
    assert_eq!(job.hidden, BTreeSet::from([Metric::GpuAmdTemp]));
    assert!(
        super::super::configured_capabilities(&cfg)
            .contains(&crate::domain::metric::Capability::GpuAmdTemperature)
    );
    cfg.notifications.gpu_amd_temp = false;
    cfg.pages.order = vec![String::from("graphs")];
    let graphs = plan(&cfg, &hw);
    let job = graphs
        .jobs
        .iter()
        .find_map(|spec| spec.amd.as_ref())
        .expect("graph sample job");
    assert_eq!(
        job.hidden,
        BTreeSet::from([Metric::GpuAmdUsage, Metric::GpuAmdCodecUsage])
    );
    assert!(
        !graphs
            .jobs
            .iter()
            .any(|spec| spec.id.kind == JobKind::AmdSlow)
    );
    let mut nvidia = hw.clone();
    nvidia.has_nvidia = true;
    assert!(
        !plan(&cfg, &nvidia)
            .jobs
            .iter()
            .any(|spec| spec.amd.is_some())
    );
    let mut unsupported = hw;
    unsupported.amd_gpu.as_mut().expect("AMD").temp_path = None;
    cfg.pages.order.clear();
    cfg.notifications.gpu_amd_temp = true;
    assert!(
        !plan(&cfg, &unsupported)
            .jobs
            .iter()
            .any(|spec| spec.amd.is_some())
    );
}

#[test]
fn slow_failure_backs_off_and_replaced_paths_reject_obsolete_generations() {
    let cfg = config(&["gpu_amd_temp"], &[]);
    let mut hw = hardware();
    let mut scheduler = Scheduler::new();
    let ticket = startup(&mut scheduler, plan(&cfg, &hw)).remove(0);
    let failure = scheduler.handle(SchedulerEvent::JobFinished {
        at: SchedulerTime::ZERO,
        ticket: ticket.clone(),
        completion: CompletionKind::Failed,
        notification_ready: false,
    });
    assert!(failure.actions.iter().any(|action| matches!(action, SchedulerAction::ApplyBackoff { retry_at, .. } if retry_at.duration() == Duration::from_millis(100))));
    let retry = starts(scheduler.handle(SchedulerEvent::TimeAdvanced {
        at: SchedulerTime::from_duration(Duration::from_millis(100)),
    }))
    .remove(0);
    hw.amd_gpu.as_mut().expect("AMD").temp_path = Some("replacement".into());
    let replacement = plan(&cfg, &hw);
    let changed = scheduler.handle(SchedulerEvent::InventoryChanged {
        at: SchedulerTime::from_duration(Duration::from_millis(101)),
        update: crate::scheduler::InventoryUpdate {
            generation: InventoryGeneration(2),
            jobs: replacement.jobs,
            demand: replacement.demand,
            resume_acknowledgement: None,
        },
    });
    assert!(!scheduler.is_current_ticket(&retry));
    assert!(changed.actions.iter().any(
        |action| matches!(action, SchedulerAction::InvalidateJob { job, .. } if job == &ticket.job)
    ));
}
