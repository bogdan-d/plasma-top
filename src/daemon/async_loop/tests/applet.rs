use super::*;
use crate::config::{PanelGeometry, read_geom_cache};
use crate::domain::applet::{AppletGeometry, AppletRequest, PageStep};
use crate::scheduler::PublishReason;

struct Fixture {
    root: PathBuf,
    config: PathBuf,
    paths: DaemonPaths,
    state: RuntimeState,
    sink: RecordingSink,
    scheduler: Scheduler,
    clock: ProductionClock,
    actions: VecDeque<SchedulerAction>,
    owner_messages: VecDeque<(OwnerId, OwnerMessage)>,
}

impl Fixture {
    fn new(label: &str, pages: &[&str]) -> Self {
        let (root, paths) = test_paths(label);
        fs::create_dir_all(&root).expect("test root");
        let config = root.join("config.toml");
        let order = pages
            .iter()
            .map(|page| format!("\"{page}\""))
            .collect::<Vec<_>>()
            .join(", ");
        fs::write(&config, format!("[pages]\norder = [{order}]\n")).expect("config fixture");
        let cfg = crate::config::load_config(Some(&config), None).expect("load config fixture");
        let active = build_pages(&cfg.pages.order);
        let (mut state, sink) = recorded_state(&paths, cfg, HardwareInventory::default(), active);
        state.announce_selection();
        let mut scheduler = Scheduler::new();
        let _ = startup(&mut scheduler, Vec::new());
        Self {
            root,
            config,
            paths,
            state,
            sink,
            scheduler,
            clock: ProductionClock::default(),
            actions: VecDeque::new(),
            owner_messages: VecDeque::new(),
        }
    }

    fn send(&mut self, event: AppletEvent) {
        let roots = FilesystemRoots {
            proc_root: self.root.join("proc"),
            sys_root: self.root.join("sys"),
            ..FilesystemRoots::default()
        };
        let context = applet::AppletContext {
            config_path: Some(&self.config),
            roots: &roots,
            paths: &self.paths,
            cpu_count: 1,
            clock: &self.clock,
        };
        applet::handle_event(
            event,
            &context,
            &mut self.scheduler,
            &mut self.state,
            &mut self.actions,
            &mut self.owner_messages,
        )
        .expect("applet event");
    }

    fn request(&mut self, request: AppletRequest) {
        self.send(AppletEvent::Request(request));
    }

    fn present(&mut self, client: &str, instance: u64) {
        self.request(AppletRequest::Present {
            client: ClientId::new(client),
            instance: InstanceId::new(instance).expect("instance"),
        });
    }

    fn dismiss(&mut self, client: &str, instance: u64) {
        self.request(AppletRequest::Dismiss {
            client: ClientId::new(client),
            instance: InstanceId::new(instance).expect("instance"),
        });
    }

    fn selections(&self) -> Vec<(u32, u32)> {
        self.sink
            .frames()
            .into_iter()
            .filter_map(|frame| match frame {
                Frame::Selection(page, count) => Some((page, count)),
                _ => None,
            })
            .collect()
    }

    /// Takes queued tooltip publications, so each step can be checked on its own.
    fn take_tooltip_publications(&mut self) -> Vec<PublishReason> {
        self.actions
            .drain(..)
            .filter_map(|action| match action {
                SchedulerAction::PublishDisplay {
                    reason,
                    tooltip: true,
                    ..
                } => Some(reason),
                _ => None,
            })
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn page_steps_apply_in_order_wrap_both_ways_and_announce_each_selection() {
    let mut fixture = Fixture::new("applet-steps", &["cpu_cores", "graphs"]);

    for step in [
        PageStep::Next,
        PageStep::Next,
        PageStep::Next,
        PageStep::Prev,
        PageStep::Prev,
    ] {
        fixture.request(AppletRequest::StepPage(step));
    }

    assert_eq!(
        fixture.selections(),
        vec![(0, 3), (1, 3), (2, 3), (0, 3), (2, 3), (1, 3)]
    );
    assert_eq!(fixture.state.selected_page(), (1, PageId::CpuCores));
}

#[test]
fn page_steps_without_deep_dive_pages_do_nothing() {
    let mut fixture = Fixture::new("applet-single-page", &[]);

    fixture.request(AppletRequest::StepPage(PageStep::Next));
    fixture.request(AppletRequest::StepPage(PageStep::Prev));

    assert_eq!(fixture.selections(), vec![(0, 1)]);
    assert!(fixture.actions.is_empty());
}

#[test]
fn shrinking_page_count_clamps_selection_and_announces_it_without_tooltip_html_while_hidden() {
    let mut fixture = Fixture::new("applet-clamp", &["cpu_cores", "graphs"]);
    fixture.request(AppletRequest::StepPage(PageStep::Prev));
    fs::write(&fixture.config, "[pages]\norder = [\"cpu_cores\"]\n").expect("shrunk config");

    // Geometry is one input that reloads config; the reload installs the shorter page list.
    fixture.request(AppletRequest::Geometry(
        AppletGeometry::new(120.0, 8.0, true, 8.0).expect("geometry"),
    ));

    assert_eq!(fixture.selections(), vec![(0, 3), (2, 3), (1, 2)]);
    assert_eq!(fixture.state.selected_page(), (1, PageId::CpuCores));
    assert!(fixture.take_tooltip_publications().is_empty());
    assert!(fixture.sink.last_tooltip().is_none());
}

#[test]
fn leases_aggregate_across_clients_and_instances_and_reject_present_after_client_gone() {
    let mut fixture = Fixture::new("applet-leases", &[]);

    fixture.present(":1.10", 1);
    assert!(fixture.state.leases.presented());
    assert_eq!(
        fixture.take_tooltip_publications(),
        vec![PublishReason::TooltipActivated]
    );
    fixture.present(":1.10", 2);
    fixture.present(":1.20", 1);
    fixture.dismiss(":1.10", 1);
    assert!(
        fixture.state.leases.presented(),
        "instance 2 still presents"
    );
    fixture.request(AppletRequest::ClientGone {
        client: ClientId::new(":1.10"),
    });
    assert!(fixture.state.leases.presented(), "the other client remains");
    fixture.present(":1.10", 3);
    fixture.dismiss(":1.99", 7);
    assert!(fixture.take_tooltip_publications().is_empty());

    fixture.dismiss(":1.20", 1);

    assert!(
        !fixture.state.leases.presented(),
        "the delayed present from the gone client must not hold a lease"
    );
}

#[test]
fn repeated_present_refreshes_the_tooltip_without_toggling_presentation() {
    let mut fixture = Fixture::new("applet-represent", &[]);
    fixture.present(":1.10", 1);
    fixture.present(":1.20", 1);
    let _ = fixture.take_tooltip_publications();

    fixture.present(":1.10", 1);

    assert_eq!(
        fixture.take_tooltip_publications(),
        vec![PublishReason::TooltipActivated]
    );
    fixture.dismiss(":1.10", 1);
    assert!(fixture.state.leases.presented());
}

#[test]
fn own_connection_loss_ends_every_lease_and_forgets_gone_clients() {
    let mut fixture = Fixture::new("applet-connection-loss", &[]);
    fixture.present(":1.10", 1);
    fixture.present(":1.20", 2);
    fixture.request(AppletRequest::ClientGone {
        client: ClientId::new(":1.20"),
    });

    fixture.send(AppletEvent::ConnectionLost);

    assert!(!fixture.state.leases.presented());
    fixture.present(":1.20", 2);
    assert!(fixture.state.leases.presented());
}

#[test]
fn accepted_geometry_sizes_graphs_persists_to_the_cache_and_ignores_repeats() {
    let mut fixture = Fixture::new("applet-geometry", &["graphs"]);
    // A tooltip item gives the canonical width something to measure.
    fs::write(
        &fixture.config,
        "[pages]\norder = [\"graphs\"]\n[tooltip]\norder = [\"cpu\"]\n[tooltip.cpu]\nitems = [\"cpu_usage\"]\n",
    )
    .expect("tooltip config");
    let geometry = AppletGeometry::new(120.0, 8.0, true, 7.5).expect("geometry");

    fixture.request(AppletRequest::Geometry(geometry));

    let measured = PanelGeometry::from(geometry);
    assert_eq!(fixture.state.geometry, Some(measured));
    assert_eq!(read_geom_cache(&fixture.paths.geom_cache), Some(measured));
    let generation = fixture.state.config_generation;
    fixture.request(AppletRequest::Geometry(geometry));
    assert_eq!(fixture.state.config_generation, generation);

    let mut scheduler = Scheduler::new();
    let _ = startup(&mut scheduler, Vec::new());
    let _ = fixture.state.publish(
        crate::scheduler::PublicationId(1),
        PublishReason::FirstPaintReady,
        None,
        0,
        true,
        false,
        &mut scheduler,
        &FilesystemRoots::default(),
        ClockSnapshot::default(),
        ClockSnapshot::default(),
    );
    let width = fixture.state.cfg.display.tooltip_width;
    assert_eq!(
        fixture.state.cfg.pages.graph_width,
        (f64::from(width) * 7.5).round() as i32
    );
}

#[test]
fn service_failure_terminates_the_daemon_with_its_reason() {
    let mut fixture = Fixture::new("applet-service-failure", &[]);

    fixture.send(AppletEvent::ServiceFailed(String::from("name taken")));

    assert_eq!(fixture.state.service_error.as_deref(), Some("name taken"));
    assert!(
        fixture
            .actions
            .iter()
            .any(|action| matches!(action, SchedulerAction::Terminate { component: Some(_) }))
    );
}
