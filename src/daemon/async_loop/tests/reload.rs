use super::*;

#[test]
fn presented_page_reorder_reconciles_graph_demand_and_publication() {
    let (root, paths) = test_paths("reload-page-reorder");
    fs::create_dir_all(&paths.state).expect("state root");
    write_atomic(&paths.page, "2").expect("selected page index");
    let config_path = root.join("config.toml");
    fs::write(
        &config_path,
        "[pages]\norder = [\"processes\", \"graphs\"]\n",
    )
    .expect("replacement config");

    let mut cfg = Config::default();
    cfg.pages.order = vec![String::from("graphs"), String::from("processes")];
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    state.configure_profiling(Arc::new(ProfileSession::default()));
    assert_eq!(state.selected_page(), (2, PageId::Processes));

    let mut scheduler = Scheduler::new();
    let _ = scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config: SchedulerConfig {
            generation: ConfigGeneration(1),
            display_interval: Duration::from_secs(1),
            jobs: Vec::new(),
            demand: DemandPlan::default(),
        },
        inventory_generation: InventoryGeneration(1),
    });
    let _ = scheduler.handle(SchedulerEvent::SelectedPageChanged {
        at: SchedulerTime::ZERO,
        page: PageId::Processes,
    });
    let _ = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });

    let roots = FilesystemRoots {
        proc_root: root.join("proc"),
        sys_root: root.join("sys"),
        ..FilesystemRoots::default()
    };
    let clock = ProductionClock::default();
    let mut actions = VecDeque::new();
    let mut owner_messages = VecDeque::new();
    assert!(
        reload::check(
            Some(&config_path),
            &roots,
            &paths,
            1,
            &mut scheduler,
            &mut state,
            &clock,
            &mut actions,
            &mut owner_messages,
        )
        .expect("reload config")
    );

    assert_eq!(state.selected_page(), (2, PageId::Graphs));
    assert!(actions.iter().any(|action| {
        matches!(action, SchedulerAction::StartJob { ticket } if ticket.job.kind == JobKind::PageRender)
    }));
    assert!(actions.iter().any(|action| {
        matches!(
            action,
            SchedulerAction::PublishDisplay {
                reason: PublishReason::PageChanged,
                panel: false,
                tooltip: true,
                ..
            }
        )
    }));
    let _ = fs::remove_dir_all(root);
}
