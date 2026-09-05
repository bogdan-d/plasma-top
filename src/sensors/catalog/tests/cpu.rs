use super::*;

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
