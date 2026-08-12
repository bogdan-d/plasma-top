#![allow(clippy::expect_used)]

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use crate::config::{Config, Mounts, Section, Surface};
use crate::domain::readings::{HardwareInventory, InventoryFamily};
use crate::scheduler::{
    ConfigGeneration, InventoryGeneration, JobId, JobKind, PageId, PeripheralRole,
    PeripheralSource, Scheduler, SchedulerAction, SchedulerEvent, SchedulerTime, SourceIdentity,
    TimingClass,
};

use super::scheduler_config as build_scheduler_config;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

mod brightness;

struct TempTree(PathBuf);

impl TempTree {
    fn new() -> Self {
        let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("plasma-top-catalog-{}-{id}", std::process::id()));
        fs::create_dir_all(&path).expect("temp tree");
        Self(path)
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn without_notifications(mut cfg: Config) -> Config {
    cfg.notifications.cpu_temp = false;
    cfg.notifications.gpu_nvidia_temp = false;
    cfg.notifications.disk_usage = false;
    cfg.notifications.disk_smart = false;
    cfg.notifications.hd_temp = false;
    cfg.notifications.battery_sys = false;
    cfg.notifications.battery_mouse = false;
    cfg.notifications.battery_kbd = false;
    cfg.notifications.load_avg = false;
    cfg.notifications.server_check = false;
    cfg
}

fn surface(items: &[&str]) -> Surface {
    Surface {
        sections: vec![Section {
            key: String::from("main"),
            title: String::new(),
            items: items.iter().map(|item| (*item).to_owned()).collect(),
        }],
        glyphs: true,
    }
}

fn scheduler_config(
    cfg: &Config,
    hw: &HardwareInventory,
    proc_root: &Path,
    generation: ConfigGeneration,
) -> crate::scheduler::SchedulerConfig {
    let mounts = super::disk::resolve_mounts(proc_root, cfg);
    build_scheduler_config(cfg, hw, &mounts, generation)
}

#[test]
fn empty_demand_has_no_unconditional_cpu_or_memory_jobs() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip.sections.clear();
    cfg.pages.order.clear();

    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );

    assert!(plan.jobs.is_empty());
    assert!(plan.demand.hidden.is_empty());
}

#[test]
fn panel_and_main_tooltip_jobs_stay_in_separate_demand_sets() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(&["mem_usage"]);
    cfg.tooltip = surface(&["cpu_usage"]);
    cfg.pages.order.clear();

    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let hidden_kinds = plan
        .demand
        .hidden
        .iter()
        .map(|job| job.kind)
        .collect::<BTreeSet<_>>();
    let main_kinds = plan
        .demand
        .main_tooltip
        .iter()
        .map(|job| job.kind)
        .collect::<BTreeSet<_>>();

    assert_eq!(hidden_kinds, BTreeSet::from([JobKind::Memory]));
    assert_eq!(main_kinds, BTreeSet::from([JobKind::Cpu]));
}

#[test]
fn cpu_and_memory_histories_follow_rendered_form() {
    for metric in ["cpu_usage", "mem_usage"] {
        let history_kind = if metric == "cpu_usage" {
            JobKind::CpuHistory
        } else {
            JobKind::MemoryHistory
        };
        for suffix in ["", ":value", ":bar"] {
            let mut cfg = without_notifications(Config::default());
            let item = format!("{metric}{suffix}");
            cfg.panel = surface(&[&item]);
            cfg.tooltip.sections.clear();
            cfg.pages.order.clear();
            let plan = scheduler_config(
                &cfg,
                &HardwareInventory::default(),
                Path::new("/missing-proc"),
                ConfigGeneration(1),
            );
            assert!(
                plan.jobs.iter().all(|spec| spec.id.kind != history_kind),
                "{metric}{suffix} must not schedule history"
            );
        }
        for suffix in [
            ":spark",
            ":braille",
            ":spark_value",
            ":braille_value",
            ":bar_spark",
            ":bar_braille",
        ] {
            let mut cfg = without_notifications(Config::default());
            let item = format!("{metric}{suffix}");
            if matches!(suffix, ":spark" | ":braille") {
                cfg.panel = surface(&[&item]);
                cfg.tooltip.sections.clear();
            } else {
                cfg.panel.sections.clear();
                cfg.tooltip = surface(&[&item]);
            }
            cfg.pages.order.clear();
            let plan = scheduler_config(
                &cfg,
                &HardwareInventory::default(),
                Path::new("/missing-proc"),
                ConfigGeneration(1),
            );
            assert!(
                plan.jobs.iter().any(|spec| spec.id.kind == history_kind),
                "{metric}{suffix} must schedule history"
            );
        }
    }
}

#[test]
fn network_history_is_only_configured_graph_demand() {
    let hw = HardwareInventory {
        net_device: Some(String::from("eth0")),
        ..HardwareInventory::default()
    };
    let mut item_cfg = without_notifications(Config::default());
    item_cfg.panel = surface(&["net_speed"]);
    item_cfg.tooltip.sections.clear();
    item_cfg.pages.order.clear();
    let item = scheduler_config(
        &item_cfg,
        &hw,
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    assert!(
        item.jobs
            .iter()
            .any(|spec| spec.id.kind == JobKind::NetworkRate)
    );
    assert!(
        item.jobs
            .iter()
            .all(|spec| spec.id.kind != JobKind::NetworkHistory)
    );

    item_cfg.pages.order = vec![String::from("graphs")];
    let graphs = scheduler_config(
        &item_cfg,
        &hw,
        Path::new("/missing-proc"),
        ConfigGeneration(2),
    );
    assert!(
        graphs
            .demand
            .hidden
            .iter()
            .any(|job| job.kind == JobKind::NetworkHistory)
    );
}

#[test]
fn configured_graph_histories_are_hidden_demand() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip.sections.clear();
    cfg.pages.order = vec![String::from("graphs")];
    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let kinds = plan
        .demand
        .hidden
        .iter()
        .map(|job| job.kind)
        .collect::<BTreeSet<_>>();

    assert!(kinds.contains(&JobKind::Cpu));
    assert!(kinds.contains(&JobKind::CpuHistory));
    assert!(kinds.contains(&JobKind::Memory));
    assert!(kinds.contains(&JobKind::MemoryHistory));
}

#[test]
fn cpu_core_history_is_only_cpu_cores_page_demand() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip.sections.clear();
    cfg.pages.order = vec![String::from("cpu_cores")];
    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let jobs = &plan.demand.pages[&PageId::CpuCores];

    assert!(jobs.iter().any(|job| job.kind == JobKind::CpuCores));
    assert!(jobs.iter().any(|job| job.kind == JobKind::CpuCoreHistory));
    assert!(
        plan.demand
            .hidden
            .iter()
            .all(|job| job.kind != JobKind::CpuCoreHistory)
    );
}

#[test]
fn startup_blockers_are_exactly_non_history_panel_jobs() {
    let mut disk_cfg = without_notifications(Config::default());
    disk_cfg.panel = surface(&["disk_usage"]);
    disk_cfg.tooltip.sections.clear();
    disk_cfg.pages.order.clear();
    disk_cfg.disks.mounts = Mounts::Explicit(vec![String::from("/")]);
    let disk = scheduler_config(
        &disk_cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    assert!(
        disk.jobs
            .iter()
            .any(|spec| { spec.id.kind == JobKind::DiskUsage && spec.startup_panel })
    );

    let mut battery_cfg = without_notifications(Config::default());
    battery_cfg.panel = surface(&["battery_sys"]);
    battery_cfg.tooltip.sections.clear();
    battery_cfg.pages.order.clear();
    let hw = HardwareInventory {
        battery_sys_ids: vec![String::from("BAT0")],
        ..HardwareInventory::default()
    };
    let battery = scheduler_config(
        &battery_cfg,
        &hw,
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    assert!(
        battery
            .jobs
            .iter()
            .any(|spec| { spec.id.kind == JobKind::SystemBattery && spec.startup_panel })
    );

    let mut background = without_notifications(Config::default());
    background.panel.sections.clear();
    background.tooltip.sections.clear();
    background.pages.order = vec![String::from("graphs")];
    background.notifications.cpu_temp = true;
    let plan = scheduler_config(
        &background,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    assert!(plan.jobs.iter().all(|spec| !spec.startup_panel));
}

#[test]
fn diff_jobs_are_marked_as_counters() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(&["cpu_usage", "net_speed", "disk_io", "top_process"]);
    cfg.tooltip.sections.clear();
    cfg.pages.order = vec![
        String::from("cpu_cores"),
        String::from("processes"),
        String::from("graphs"),
    ];
    let hw = HardwareInventory {
        net_device: Some(String::from("eth0")),
        disk_io_device: Some(String::from("sda")),
        intel_gpu_pci: Some(String::from("0000:00:02.0")),
        ..HardwareInventory::default()
    };
    let plan = scheduler_config(&cfg, &hw, Path::new("/missing-proc"), ConfigGeneration(1));
    for kind in [
        JobKind::Cpu,
        JobKind::CpuCores,
        JobKind::NetworkRate,
        JobKind::DiskIo,
        JobKind::PanelProcesses,
        JobKind::PageProcesses,
        JobKind::IntelUsage,
    ] {
        assert!(
            plan.jobs
                .iter()
                .find(|spec| spec.id.kind == kind)
                .is_some_and(|spec| spec.counter),
            "{kind:?} must reset its diff baseline"
        );
    }
}

#[test]
fn graphs_without_route_demand_discovery_reconciliation() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip.sections.clear();
    cfg.pages.order = vec![String::from("graphs")];
    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    assert!(
        plan.demand
            .hidden
            .iter()
            .any(|job| job.kind == JobKind::HardwareDiscovery)
    );
}

#[test]
fn demanded_inventory_families_use_budgeted_bucketed_reconciliation() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(&[
        "cpu_temp",
        "hd_temp",
        "disk_smart:pair",
        "gpu_nvidia_temp",
        "gpu_intel_freq",
        "screen_brightness",
        "battery_sys",
        "battery_mouse",
        "net_speed",
        "disk_io",
    ]);
    cfg.tooltip = surface(&["fan_speed"]);
    cfg.pages.order.clear();
    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let hidden = plan
        .demand
        .hidden
        .iter()
        .filter_map(|job| match &job.source {
            SourceIdentity::Inventory(family) => Some(*family),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let tooltip = plan
        .demand
        .main_tooltip
        .iter()
        .filter_map(|job| match &job.source {
            SourceIdentity::Inventory(family) => Some(*family),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(
        hidden,
        BTreeSet::from([
            InventoryFamily::Cpu,
            InventoryFamily::Thermal,
            InventoryFamily::SystemBattery,
            InventoryFamily::Smart,
            InventoryFamily::Nvidia,
            InventoryFamily::Intel,
            InventoryFamily::Backlight,
            InventoryFamily::Network,
            InventoryFamily::DiskIo,
            InventoryFamily::Peripheral,
        ])
    );
    assert_eq!(tooltip, BTreeSet::from([InventoryFamily::Thermal]));
    for spec in plan
        .jobs
        .iter()
        .filter(|spec| matches!(&spec.id.source, SourceIdentity::Inventory(_)))
    {
        let expected = match &spec.id.source {
            SourceIdentity::Inventory(
                InventoryFamily::SystemBattery | InventoryFamily::Peripheral,
            ) => Duration::from_secs(30),
            SourceIdentity::Inventory(_) => Duration::from_secs(60),
            _ => unreachable!(),
        };
        assert_eq!(spec.freshness, expected);
    }
}

#[test]
fn one_hardware_family_does_not_poll_absent_unrelated_families() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(&["battery_sys"]);
    cfg.tooltip.sections.clear();
    cfg.pages.order.clear();
    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let families = plan
        .jobs
        .iter()
        .filter_map(|spec| match &spec.id.source {
            SourceIdentity::Inventory(family) => Some(*family),
            _ => None,
        })
        .collect::<BTreeSet<_>>();

    assert_eq!(families, BTreeSet::from([InventoryFamily::SystemBattery]));
}

#[test]
fn configured_unifying_mouse_and_bolt_keyboard_are_immediate_startup_jobs() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(&["battery_mouse", "battery_kbd"]);
    cfg.tooltip.sections.clear();
    cfg.pages.order.clear();
    cfg.battery.mouse_unifying = Some(String::from("/configured_mouse"));
    cfg.battery.mouse_bolt = Some(9);
    cfg.battery.kbd_bolt = Some(7);
    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let mouse = JobId::with_source(
        crate::scheduler::OwnerId::Power,
        JobKind::PeripheralBattery,
        SourceIdentity::Peripheral {
            role: PeripheralRole::Mouse,
            source: PeripheralSource::Upower(String::from("/configured_mouse")),
        },
    );
    let keyboard = JobId::with_source(
        crate::scheduler::OwnerId::Power,
        JobKind::PeripheralBattery,
        SourceIdentity::Peripheral {
            role: PeripheralRole::Keyboard,
            source: PeripheralSource::Bolt(7),
        },
    );

    for job in [&mouse, &keyboard] {
        assert!(plan.demand.hidden.contains(job));
        assert!(
            plan.jobs
                .iter()
                .any(|spec| &spec.id == job && spec.startup_panel)
        );
    }
    assert!(
        plan.jobs.iter().all(|spec| {
            spec.id.source != SourceIdentity::Inventory(InventoryFamily::Peripheral)
        })
    );
}

#[test]
fn reload_explicit_sources_replace_and_invalidate_discovered_upower_jobs() {
    let mut automatic = without_notifications(Config::default());
    automatic.panel = surface(&["battery_mouse", "battery_kbd"]);
    automatic.tooltip.sections.clear();
    automatic.pages.order.clear();
    let hw = HardwareInventory {
        battery_mouse_id: Some(String::from("/discovered_mouse")),
        battery_kbd_id: Some(String::from("/discovered_keyboard")),
        ..HardwareInventory::default()
    };
    let initial = scheduler_config(
        &automatic,
        &hw,
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let old_jobs = initial
        .jobs
        .iter()
        .filter(|spec| spec.id.kind == JobKind::PeripheralBattery)
        .map(|spec| spec.id.clone())
        .collect::<BTreeSet<_>>();
    let mut scheduler = Scheduler::new();
    let _ = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: initial,
        inventory_generation: InventoryGeneration(1),
    });

    let mut explicit = automatic;
    explicit.battery.mouse_bolt = Some(3);
    explicit.battery.kbd_unifying = Some(String::from("/configured_keyboard"));
    let replacement = scheduler_config(
        &explicit,
        &hw,
        Path::new("/missing-proc"),
        ConfigGeneration(2),
    );
    let replacement_sources = replacement
        .jobs
        .iter()
        .filter_map(|spec| match &spec.id.source {
            SourceIdentity::Peripheral { role, source } => Some((*role, source.clone())),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        replacement_sources,
        BTreeSet::from([
            (PeripheralRole::Mouse, PeripheralSource::Bolt(3)),
            (
                PeripheralRole::Keyboard,
                PeripheralSource::Upower(String::from("/configured_keyboard")),
            ),
        ])
    );

    let changed = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: SchedulerTime::ZERO,
        config: replacement,
    });
    let invalidated = changed
        .actions
        .iter()
        .filter_map(|action| match action {
            SchedulerAction::InvalidateJob { job, .. } => Some(job.clone()),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert!(old_jobs.is_subset(&invalidated));
}

#[test]
fn tooltip_only_missing_sources_discover_only_while_presented() {
    for item in ["net_speed", "battery_mouse", "battery_kbd"] {
        let mut cfg = without_notifications(Config::default());
        cfg.panel.sections.clear();
        cfg.tooltip = surface(&[item]);
        cfg.pages.order.clear();
        let plan = scheduler_config(
            &cfg,
            &HardwareInventory::default(),
            Path::new("/missing-proc"),
            ConfigGeneration(1),
        );

        assert!(
            plan.demand
                .main_tooltip
                .iter()
                .any(|job| job.kind == JobKind::HardwareDiscovery),
            "{item} must discover while the tooltip is presented"
        );
        assert!(
            plan.demand
                .hidden
                .iter()
                .all(|job| job.kind != JobKind::HardwareDiscovery),
            "{item} must remain idle while the tooltip is hidden"
        );
    }
}

#[test]
fn notification_only_missing_peripheral_keeps_hidden_discovery_alive() {
    for mouse in [true, false] {
        let mut cfg = without_notifications(Config::default());
        cfg.panel.sections.clear();
        cfg.tooltip.sections.clear();
        cfg.pages.order.clear();
        cfg.notifications.battery_mouse = mouse;
        cfg.notifications.battery_kbd = !mouse;
        let plan = scheduler_config(
            &cfg,
            &HardwareInventory::default(),
            Path::new("/missing-proc"),
            ConfigGeneration(1),
        );

        assert!(
            plan.demand
                .hidden
                .iter()
                .any(|job| job.kind == JobKind::HardwareDiscovery)
        );
        assert!(plan.demand.main_tooltip.is_empty());
    }
}

#[test]
fn gpu_history_is_deadline_only_and_selected_source_aware() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip.sections.clear();
    cfg.pages.order = vec![String::from("graphs")];
    let nvidia = scheduler_config(
        &cfg,
        &HardwareInventory {
            has_nvidia: true,
            ..HardwareInventory::default()
        },
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let nvidia_history = nvidia
        .jobs
        .iter()
        .find(|spec| spec.id.kind == JobKind::GpuHistory)
        .expect("NVIDIA history");
    assert_eq!(nvidia_history.timing, TimingClass::History);
    assert_eq!(
        nvidia_history.id.source,
        SourceIdentity::Device(String::from("nvidia"))
    );

    let intel = scheduler_config(
        &cfg,
        &HardwareInventory {
            intel_gpu_pci: Some(String::from("0000:00:02.0")),
            ..HardwareInventory::default()
        },
        Path::new("/missing-proc"),
        ConfigGeneration(2),
    );
    let intel_history = intel
        .jobs
        .iter()
        .find(|spec| spec.id.kind == JobKind::GpuHistory)
        .expect("Intel history");
    assert_ne!(nvidia_history.id, intel_history.id);
}

#[test]
fn aggregate_cpu_identity_replaces_and_removes_selected_sources_before_publication() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip = surface(&["cpu_usage", "cpu_temp", "cpu_freq", "cpu_turbo"]);
    cfg.pages.order.clear();
    let first_hw = HardwareInventory {
        cpu_temp_path: Some(PathBuf::from("/sys/temp-a")),
        cpu_freq_path: Some(PathBuf::from("/sys/freq-a")),
        cpu_turbo_path: Some(PathBuf::from("/sys/turbo-a")),
        cpu_turbo_supported: true,
        ..HardwareInventory::default()
    };
    let first = scheduler_config(
        &cfg,
        &first_hw,
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let first_cpu = first
        .jobs
        .iter()
        .find(|spec| spec.id.kind == JobKind::Cpu)
        .expect("first CPU job")
        .id
        .clone();
    assert_eq!(
        first_cpu.source,
        SourceIdentity::CpuSources {
            temperature: first_hw.cpu_temp_path.clone(),
            frequency: first_hw.cpu_freq_path.clone(),
            turbo: first_hw.cpu_turbo_path.clone(),
        }
    );

    let replacement_hw = HardwareInventory {
        cpu_temp_path: Some(PathBuf::from("/sys/temp-b")),
        cpu_freq_path: Some(PathBuf::from("/sys/freq-b")),
        cpu_turbo_path: Some(PathBuf::from("/sys/turbo-b")),
        cpu_turbo_supported: true,
        ..HardwareInventory::default()
    };
    let replacement = scheduler_config(
        &cfg,
        &replacement_hw,
        Path::new("/missing-proc"),
        ConfigGeneration(2),
    );
    let removed = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(3),
    );
    let mut scheduler = Scheduler::new();
    let _ = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: first,
        inventory_generation: InventoryGeneration(1),
    });

    let mut previous_cpu = first_cpu;
    for config in [replacement, removed] {
        let current_cpu = config
            .jobs
            .iter()
            .find(|spec| spec.id.kind == JobKind::Cpu)
            .expect("replacement CPU job")
            .id
            .clone();
        let changed = scheduler.handle(SchedulerEvent::ConfigChanged {
            at: SchedulerTime::ZERO,
            config,
        });
        let invalidation = changed
            .actions
            .iter()
            .position(|action| {
                matches!(action, SchedulerAction::InvalidateJob { job, .. } if job == &previous_cpu)
            })
            .expect("old CPU source invalidation");
        let publication = changed
            .actions
            .iter()
            .position(|action| matches!(action, SchedulerAction::PublishDisplay { .. }))
            .expect("config publication");
        assert!(invalidation < publication);
        assert_ne!(current_cpu, previous_cpu);
        previous_cpu = current_cpu;
    }
}

#[test]
fn aggregate_cpu_identity_ignores_unselected_auxiliary_sources() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(&["cpu_usage"]);
    cfg.tooltip.sections.clear();
    cfg.pages.order.clear();
    let plan = scheduler_config(
        &cfg,
        &HardwareInventory {
            cpu_temp_path: Some(PathBuf::from("/sys/temp")),
            cpu_freq_path: Some(PathBuf::from("/sys/freq")),
            cpu_turbo_path: Some(PathBuf::from("/sys/turbo")),
            cpu_turbo_supported: true,
            ..HardwareInventory::default()
        },
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let source = &plan
        .jobs
        .iter()
        .find(|spec| spec.id.kind == JobKind::Cpu)
        .expect("CPU job")
        .id
        .source;

    assert_eq!(
        source,
        &SourceIdentity::CpuSources {
            temperature: None,
            frequency: None,
            turbo: None,
        }
    );
}

#[test]
fn automatic_mount_inventory_tracks_mount_add_and_remove() {
    let tmp = TempTree::new();
    let proc_root = tmp.0.join("proc");
    fs::create_dir_all(&proc_root).expect("proc fixture");
    let mut cfg = without_notifications(Config::default());
    cfg.panel = surface(&["disk_usage"]);
    cfg.tooltip.sections.clear();
    cfg.pages.order.clear();
    cfg.disks.mounts = Mounts::Auto;
    cfg.disks.auto_roots = vec![String::from("/mnt")];
    fs::write(
        proc_root.join("mounts"),
        "/dev/root / ext4 rw 0 0\n/dev/a /mnt/a ext4 rw 0 0\n",
    )
    .expect("first mounts");
    let first = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        &proc_root,
        ConfigGeneration(1),
    );
    fs::write(
        proc_root.join("mounts"),
        "/dev/root / ext4 rw 0 0\n/dev/b /mnt/b ext4 rw 0 0\n",
    )
    .expect("second mounts");
    let second = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        &proc_root,
        ConfigGeneration(2),
    );
    let mounts = |plan: &crate::scheduler::SchedulerConfig| {
        plan.jobs
            .iter()
            .filter_map(|spec| match (&spec.id.kind, &spec.id.source) {
                (JobKind::DiskUsage, SourceIdentity::Mount(path)) => Some(path.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>()
    };
    assert_ne!(mounts(&first), mounts(&second));
    assert!(
        second
            .jobs
            .iter()
            .any(|spec| spec.id.kind == JobKind::MountInventory)
    );
    assert!(
        second
            .demand
            .hidden
            .iter()
            .any(|job| job.kind == JobKind::MountInventory)
    );
}

#[test]
fn tooltip_only_automatic_mount_inventory_runs_only_while_presented() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip = surface(&["disk_usage"]);
    cfg.pages.order.clear();
    cfg.disks.mounts = Mounts::Auto;
    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );

    assert!(
        plan.demand
            .main_tooltip
            .iter()
            .any(|job| job.kind == JobKind::MountInventory)
    );
    assert!(
        plan.demand
            .hidden
            .iter()
            .all(|job| job.kind != JobKind::MountInventory)
    );
}

#[test]
fn automatic_mount_inventory_has_one_spec_across_demand_buckets() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip = surface(&["disk_usage"]);
    cfg.pages.order.clear();
    cfg.disks.mounts = Mounts::Auto;
    cfg.notifications.disk_usage = true;
    let plan = scheduler_config(
        &cfg,
        &HardwareInventory::default(),
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );

    assert!(
        plan.demand
            .hidden
            .iter()
            .any(|job| job.kind == JobKind::MountInventory)
    );
    assert!(
        plan.demand
            .main_tooltip
            .iter()
            .any(|job| job.kind == JobKind::MountInventory)
    );
    assert_eq!(
        plan.jobs
            .iter()
            .filter(|spec| spec.id.kind == JobKind::MountInventory)
            .count(),
        1
    );
}
