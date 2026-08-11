use super::*;

#[test]
fn stale_generation_completion_is_rejected_before_commit() {
    let (root, paths) = test_paths("stale");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let hw = HardwareInventory::default();
    let active = build_pages(&cfg.pages.order);
    let runtime_state = RuntimeState::new(&paths, cfg, hw.clone(), active);
    let mut scheduler = Scheduler::new();
    let initial = startup(&mut scheduler, vec![job(OwnerId::Cpu, JobKind::Cpu, false)]);
    let ticket = started_ticket(&initial);
    let changed = scheduler_config(2, vec![job(OwnerId::Cpu, JobKind::Cpu, false)]);
    let _ = scheduler.handle(SchedulerEvent::ConfigChanged {
        at: SchedulerTime::ZERO,
        config: changed,
    });
    let completion = worker::JobCompletion {
        ticket,
        completion: CompletionKind::Captured,
        readings: DisplaySnapshot {
            cpu_usage: Some(99),
            ..DisplaySnapshot::default()
        },
        notifications: DisplaySnapshot::default(),
        notification_ready: false,
        hw,
        resolved_mounts: vec![String::from("/")],
        command_cache: None,
        rendered_page: None,
        style_generation: 1,
        render_generation: 1,
        decoder_outcome: None,
        gpu_history_point: None,
        decision: None,
    };
    assert!(!runtime_state.can_commit(&scheduler, &completion));
    assert_eq!(runtime_state.readings.cpu_usage, None);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn completion_merges_only_inventory_owned_by_its_job() {
    let (root, paths) = test_paths("inventory-merge");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let mut hw = HardwareInventory {
        has_nvidia: true,
        ..HardwareInventory::default()
    };
    let active = build_pages(&cfg.pages.order);
    let mut runtime_state = RuntimeState::new(&paths, cfg, hw.clone(), active);
    hw.net_device = Some(String::from("wlan0"));
    let completion = worker::JobCompletion {
        ticket: JobTicket {
            run_id: RunId(1),
            job: JobId::singleton(OwnerId::Network, JobKind::NetworkIdentity),
            config_generation: ConfigGeneration(1),
            inventory_generation: InventoryGeneration(1),
            history_deadline: None,
        },
        completion: CompletionKind::Captured,
        readings: DisplaySnapshot::default(),
        notifications: DisplaySnapshot::default(),
        notification_ready: false,
        hw,
        resolved_mounts: vec![String::from("/")],
        command_cache: None,
        rendered_page: None,
        style_generation: 1,
        render_generation: 1,
        decoder_outcome: None,
        gpu_history_point: None,
        decision: None,
    };

    assert!(runtime_state.commit(&completion));
    assert!(runtime_state.hw.has_nvidia);
    assert_eq!(runtime_state.hw.net_device.as_deref(), Some("wlan0"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn intel_frequency_completion_cannot_replace_decoder_outcome() {
    let (root, paths) = test_paths("decoder-owner");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut runtime_state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    runtime_state.nvidia_decoder_outcome = Some((
        SourceIdentity::Device(String::from("nvidia")),
        crate::sensors::gpu_history::DecoderOutcome::Value(37),
    ));
    let completion = worker::JobCompletion {
        ticket: JobTicket {
            run_id: RunId(1),
            job: JobId::singleton(OwnerId::IntelGpu, JobKind::IntelFrequency),
            config_generation: ConfigGeneration(1),
            inventory_generation: InventoryGeneration(1),
            history_deadline: None,
        },
        completion: CompletionKind::Captured,
        readings: DisplaySnapshot::default(),
        notifications: DisplaySnapshot::default(),
        notification_ready: false,
        hw: HardwareInventory::default(),
        resolved_mounts: vec![String::from("/")],
        command_cache: None,
        rendered_page: None,
        style_generation: 1,
        render_generation: 1,
        decoder_outcome: None,
        gpu_history_point: None,
        decision: None,
    };

    assert!(!runtime_state.commit(&completion));
    assert_eq!(
        runtime_state.nvidia_decoder_outcome,
        Some((
            SourceIdentity::Device(String::from("nvidia")),
            crate::sensors::gpu_history::DecoderOutcome::Value(37)
        ))
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn replaced_intel_source_cannot_feed_new_source_history() {
    let (root, paths) = test_paths("decoder-source");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut runtime_state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    runtime_state.intel_decoder_outcome = Some((
        SourceIdentity::Device(String::from("intel:0000:00:02.0")),
        crate::sensors::gpu_history::DecoderOutcome::Value(42),
    ));
    let replacement = JobId::with_source(
        OwnerId::GpuHistory,
        JobKind::GpuHistory,
        SourceIdentity::Device(String::from("intel:0000:00:03.0")),
    );

    assert_eq!(
        runtime_state.decoder_outcome_for(&replacement),
        crate::sensors::gpu_history::DecoderOutcome::Unmeasured
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn intel_usage_completion_feeds_matching_history_source() {
    let (root, paths) = test_paths("decoder-matching-source");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut runtime_state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    let pci = String::from("0000:00:02.0");
    let completion = worker::JobCompletion {
        ticket: JobTicket {
            run_id: RunId(1),
            job: JobId::with_source(
                OwnerId::IntelGpu,
                JobKind::IntelUsage,
                SourceIdentity::Device(pci.clone()),
            ),
            config_generation: ConfigGeneration(1),
            inventory_generation: InventoryGeneration(1),
            history_deadline: None,
        },
        completion: CompletionKind::Captured,
        readings: DisplaySnapshot::default(),
        notifications: DisplaySnapshot::default(),
        notification_ready: false,
        hw: HardwareInventory::default(),
        resolved_mounts: vec![String::from("/")],
        command_cache: None,
        rendered_page: None,
        style_generation: 1,
        render_generation: 1,
        decoder_outcome: Some(crate::sensors::gpu_history::DecoderOutcome::Value(23)),
        gpu_history_point: None,
        decision: None,
    };
    runtime_state.commit(&completion);
    let history = JobId::with_source(
        OwnerId::GpuHistory,
        JobKind::GpuHistory,
        SourceIdentity::Device(format!("intel:{pci}")),
    );

    assert_eq!(
        runtime_state.decoder_outcome_for(&history),
        crate::sensors::gpu_history::DecoderOutcome::Value(23)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn skipped_nvidia_fallback_preserves_nvml_decoder_outcome() {
    assert_eq!(
        worker::job_decoder_outcome(
            JobKind::NvidiaFallback,
            CompletionKind::ConfirmedAbsent,
            Some(61),
            None,
        ),
        None
    );
}

#[test]
fn gpu_history_selects_sample_at_or_before_nominal_deadline() {
    let (root, paths) = test_paths("gpu-history-cutoff");
    fs::create_dir_all(&paths.state).expect("state root");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut runtime_state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    let source = SourceIdentity::Device(String::from("nvidia"));
    let mut samples = RetainedMetricSample::default();
    samples.record_value(
        (
            Some(10),
            Some(20),
            crate::sensors::gpu_history::DecoderOutcome::Value(20),
        ),
        Duration::from_millis(100),
    );
    samples.record_value(
        (
            Some(30),
            None,
            crate::sensors::gpu_history::DecoderOutcome::ConfirmedAbsent,
        ),
        Duration::from_millis(300),
    );
    runtime_state
        .gpu_history_samples
        .insert(source.clone(), samples);
    let mut ticket = JobTicket {
        run_id: RunId(1),
        job: JobId::with_source(OwnerId::GpuHistory, JobKind::GpuHistory, source),
        config_generation: ConfigGeneration(1),
        inventory_generation: InventoryGeneration(1),
        history_deadline: Some(HistoryDeadline::new(SchedulerTime::from_duration(
            Duration::from_millis(200),
        ))),
    };

    let selected = runtime_state
        .gpu_history_point_for(&ticket)
        .expect("eligible GPU sample");
    assert_eq!(
        selected.value,
        (
            Some(10),
            Some(20),
            crate::sensors::gpu_history::DecoderOutcome::Value(20)
        )
    );
    assert_eq!(selected.captured_at, Duration::from_millis(100));
    ticket.history_deadline = Some(HistoryDeadline::new(SchedulerTime::from_duration(
        Duration::from_millis(400),
    )));
    let absence = runtime_state
        .gpu_history_point_for(&ticket)
        .expect("confirmed decoder absence");
    assert_eq!(
        absence.value.2,
        crate::sensors::gpu_history::DecoderOutcome::ConfirmedAbsent
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn panel_only_publication_leaves_existing_tooltip_unchanged() {
    let (root, paths) = test_paths("panel-only-publication");
    fs::create_dir_all(&paths.state).expect("state root");
    write_atomic(&paths.page, "0").expect("page state");
    fs::write(&paths.panel, "stale panel").expect("stale panel");
    fs::write(&paths.tooltip, "existing tooltip").expect("existing tooltip");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    state.panel_html = Some(String::from("stale panel"));
    state.tooltip_html = Some(String::from("existing tooltip"));
    let roots = FilesystemRoots {
        runtime_root: Some(paths.runtime.clone()),
        ..FilesystemRoots::default()
    };
    let mut scheduler = Scheduler::new();
    let initial = startup(&mut scheduler, Vec::new());
    let (publication, reason, panel, tooltip) = initial
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::PublishDisplay {
                publication,
                reason,
                panel,
                tooltip,
            } => Some((*publication, *reason, *panel, *tooltip)),
            _ => None,
        })
        .expect("panel-only publication");
    assert!(panel && !tooltip);

    let acknowledgement = state
        .publish(
            publication,
            reason,
            panel,
            tooltip,
            &mut scheduler,
            &roots,
            &paths,
            ClockSnapshot::default(),
            ClockSnapshot::default(),
        )
        .expect("publish panel")
        .expect("panel acknowledgement");

    assert_eq!(acknowledgement.disposition, EventDisposition::Accepted);
    assert!(fs::read_to_string(&paths.panel).is_ok_and(|html| html.contains("class=\"panel")));
    assert_eq!(
        fs::read(&paths.tooltip).expect("tooltip bytes"),
        b"existing tooltip"
    );
    assert_eq!(state.tooltip_html.as_deref(), Some("existing tooltip"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn tooltip_only_publication_writes_tooltip_without_touching_panel() {
    let (root, paths) = test_paths("tooltip-only-publication");
    fs::create_dir_all(&paths.state).expect("state root");
    write_atomic(&paths.page, "0").expect("page state");
    fs::write(&paths.panel, "existing panel").expect("existing panel");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    state.panel_html = Some(String::from("existing panel"));
    let roots = FilesystemRoots {
        runtime_root: Some(paths.runtime.clone()),
        ..FilesystemRoots::default()
    };
    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, Vec::new());
    let activated = scheduler.handle(SchedulerEvent::TooltipPresented {
        at: SchedulerTime::ZERO,
        presented: true,
    });
    let (publication, reason, panel, tooltip) = activated
        .actions
        .iter()
        .find_map(|action| match action {
            SchedulerAction::PublishDisplay {
                publication,
                reason,
                panel,
                tooltip,
            } => Some((*publication, *reason, *panel, *tooltip)),
            _ => None,
        })
        .expect("tooltip-only publication");
    assert!(!panel && tooltip);

    let acknowledgement = state
        .publish(
            publication,
            reason,
            panel,
            tooltip,
            &mut scheduler,
            &roots,
            &paths,
            ClockSnapshot::default(),
            ClockSnapshot::default(),
        )
        .expect("publish tooltip");

    assert!(acknowledgement.is_none());
    assert!(fs::read_to_string(&paths.tooltip).is_ok_and(|html| html.contains("class=\"tooltip")));
    assert_eq!(
        fs::read(&paths.panel).expect("panel bytes"),
        b"existing panel"
    );
    assert_eq!(state.panel_html.as_deref(), Some("existing panel"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn forced_style_rescan_rereads_bytes_when_mtime_is_preserved() {
    let (root, paths) = test_paths("forced-style-rescan");
    fs::create_dir_all(&paths.state).expect("state root");
    let style = root.join("style.css");
    fs::write(&style, ".old { color: red; }").expect("initial style");
    let preserved = fs::metadata(&style)
        .expect("style metadata")
        .modified()
        .expect("style mtime");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    state.css_path = style.clone();
    state.overlay_path = None;
    state.css_stamp = Some(preserved);
    state.css = String::from("old");
    fs::write(&style, ".new { color: blue; }").expect("replacement style");
    fs::File::options()
        .write(true)
        .open(&style)
        .expect("open style")
        .set_modified(preserved)
        .expect("preserve mtime");

    assert!(state.reload_style_files(true));
    assert!(state.css.contains(".new"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn forced_theme_rescan_reselects_style_when_mtime_is_preserved() {
    let (root, paths) = test_paths("forced-theme-rescan");
    fs::create_dir_all(&paths.state).expect("state root");
    fs::write(
        &paths.kdeglobals,
        "[Colors:Window]\nBackgroundNormal=0,0,0\n",
    )
    .expect("initial theme");
    let preserved = fs::metadata(&paths.kdeglobals)
        .expect("theme metadata")
        .modified()
        .expect("theme mtime");
    let cfg = Config::default();
    let active = build_pages(&cfg.pages.order);
    let mut state = RuntimeState::new(&paths, cfg, HardwareInventory::default(), active);
    assert_eq!(
        state.css_path.file_name().and_then(|name| name.to_str()),
        Some("style-dark.css")
    );
    fs::write(
        &paths.kdeglobals,
        "[Colors:Window]\nBackgroundNormal=255,255,255\n",
    )
    .expect("replacement theme");
    fs::File::options()
        .write(true)
        .open(&paths.kdeglobals)
        .expect("open theme")
        .set_modified(preserved)
        .expect("preserve mtime");

    assert!(state.update_style(&paths, true));
    assert_eq!(
        state.css_path.file_name().and_then(|name| name.to_str()),
        Some("style-light.css")
    );
    let _ = fs::remove_dir_all(root);
}
