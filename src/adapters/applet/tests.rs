#![allow(clippy::expect_used)]

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};

use zbus::fdo::PropertiesProxy;
use zbus::{MatchRule, MessageStream, Proxy};

use super::*;
use crate::adapters::ProductionIo;

/// Private `dbus-daemon` listening on a fixed socket so a test can restart it at the same address.
struct PrivateBus {
    dir: PathBuf,
    address: String,
    daemon: Child,
}

impl PrivateBus {
    fn start(label: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("plasma-top-bus-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("bus directory");
        let address = format!("unix:path={}", dir.join("bus").display());
        let daemon = Self::spawn(&address);
        Self {
            dir,
            address,
            daemon,
        }
    }

    fn spawn(address: &str) -> Child {
        let mut daemon = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .arg(format!("--address={address}"))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("dbus-daemon must be installed for the applet bus suite");
        let mut line = String::new();
        BufReader::new(daemon.stdout.take().expect("bus stdout"))
            .read_line(&mut line)
            .expect("bus address");
        assert!(line.starts_with("unix:"), "bus did not start: {line}");
        daemon
    }

    fn restart(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_file(self.dir.join("bus"));
        self.daemon = Self::spawn(&self.address);
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn client_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("client runtime")
}

async fn client(bus: &PrivateBus) -> Connection {
    zbus::connection::Builder::address(bus.address.as_str())
        .expect("bus address")
        .build()
        .await
        .expect("client connection")
}

async fn proxy(connection: &Connection) -> Proxy<'static> {
    Proxy::new(connection, BUS_NAME, OBJECT_PATH, INTERFACE)
        .await
        .expect("applet proxy")
}

async fn next_event(service: &mut AppletService) -> AppletEvent {
    tokio::time::timeout(Duration::from_secs(2), service.events.recv())
        .await
        .expect("event deadline")
        .expect("event channel open")
}

async fn name_owner(connection: &Connection) -> Option<String> {
    DBusProxy::new(connection)
        .await
        .expect("bus proxy")
        .get_name_owner(BusName::try_from(BUS_NAME).expect("bus name"))
        .await
        .ok()
        .map(|owner| owner.to_string())
}

fn invalid_args(result: zbus::Result<()>) -> bool {
    matches!(result, Err(zbus::Error::MethodError(name, _, _)) if name.as_str() == "org.freedesktop.DBus.Error.InvalidArgs")
}

#[test]
fn methods_map_to_requests_from_the_sender_and_bad_arguments_are_rejected() {
    let bus = PrivateBus::start("methods");
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut service = start(
        &io.runtime_handle(),
        io.shutdown_receiver(),
        Some(bus.address.clone()),
    )
    .expect("applet service");

    client_runtime().block_on(async {
        let connection = client(&bus).await;
        let me = ClientId::new(connection.unique_name().expect("unique name").to_string());
        let applet = proxy(&connection).await;

        applet
            .call::<_, _, ()>("StepPage", &(1_i32,))
            .await
            .expect("step");
        assert_eq!(
            next_event(&mut service).await,
            AppletEvent::Request(AppletRequest::StepPage(PageStep::Next))
        );
        applet
            .call::<_, _, ()>("Present", &(5_u64,))
            .await
            .expect("present");
        assert_eq!(
            next_event(&mut service).await,
            AppletEvent::Request(AppletRequest::Present {
                client: me.clone(),
                instance: InstanceId::new(5).expect("instance"),
            })
        );
        applet
            .call::<_, _, ()>("Dismiss", &(5_u64,))
            .await
            .expect("dismiss");
        assert_eq!(
            next_event(&mut service).await,
            AppletEvent::Request(AppletRequest::Dismiss {
                client: me,
                instance: InstanceId::new(5).expect("instance"),
            })
        );
        applet
            .call::<_, _, ()>("SetGeometry", &(120.0_f64, 8.0_f64, true, 7.5_f64))
            .await
            .expect("geometry");
        assert_eq!(
            next_event(&mut service).await,
            AppletEvent::Request(AppletRequest::Geometry(
                AppletGeometry::new(120.0, 8.0, true, 7.5).expect("geometry")
            ))
        );

        assert!(invalid_args(applet.call("StepPage", &(2_i32,)).await));
        assert!(invalid_args(applet.call("Present", &(0_u64,)).await));
        assert!(invalid_args(applet.call("Dismiss", &(0_u64,)).await));
        assert!(invalid_args(
            applet
                .call("SetGeometry", &(16_385.0_f64, 8.0_f64, true, 7.5_f64))
                .await
        ));
        assert!(invalid_args(
            applet
                .call("SetGeometry", &(120.0_f64, f64::NAN, true, 7.5_f64))
                .await
        ));
        let wrong: zbus::Result<()> = applet.call("StepPage", &("next",)).await;
        assert!(wrong.is_err(), "a wrong signature must be rejected");
        assert!(
            service.events.try_recv().is_err(),
            "rejected calls must not reach the daemon"
        );
    });
}

#[test]
fn properties_hold_published_values_and_announce_page_with_page_count() {
    let bus = PrivateBus::start("properties");
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let service = start(
        &io.runtime_handle(),
        io.shutdown_receiver(),
        Some(bus.address.clone()),
    )
    .expect("applet service");
    let mut publisher = service.publisher.clone();

    client_runtime().block_on(async {
        let connection = client(&bus).await;
        let rule = MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.DBus.Properties")
            .expect("interface")
            .member("PropertiesChanged")
            .expect("member")
            .path(OBJECT_PATH)
            .expect("path")
            .build();
        let mut signals = MessageStream::for_match_rule(rule, &connection, None)
            .await
            .expect("signal subscription");

        let properties = PropertiesProxy::builder(&connection)
            .destination(BUS_NAME)
            .expect("destination")
            .path(OBJECT_PATH)
            .expect("path")
            .build()
            .await
            .expect("properties proxy");
        let initial = properties
            .get_all(InterfaceName::try_from(INTERFACE).expect("interface"))
            .await
            .expect("initial GetAll");
        assert_eq!(u32::try_from(&initial["Page"]).expect("page"), 0);
        assert_eq!(
            u32::try_from(&initial["PageCount"]).expect("count"),
            1,
            "the selection is valid before the daemon announces one"
        );

        publisher.selection(1, 3);
        let signal = tokio::time::timeout(
            Duration::from_secs(2),
            std::future::poll_fn(|cx| {
                use zbus::export::futures_core::Stream;
                std::pin::Pin::new(&mut signals).poll_next(cx)
            }),
        )
        .await
        .expect("signal deadline")
        .expect("signal")
        .expect("signal message");
        let (interface, changed, _): (
            String,
            HashMap<String, zbus::zvariant::OwnedValue>,
            Vec<String>,
        ) = signal.body().deserialize().expect("PropertiesChanged body");
        assert_eq!(interface, INTERFACE);
        assert_eq!(u32::try_from(&changed["Page"]).expect("page"), 1);
        assert_eq!(u32::try_from(&changed["PageCount"]).expect("count"), 3);
        assert!(!changed.contains_key("PanelHtml"));

        publisher.panel("<div class=\"panel\">7</div>");
        publisher.tooltip("<div class=\"tooltip\">8</div>");
        tokio::time::sleep(Duration::from_millis(50)).await;
        let all = properties
            .get_all(InterfaceName::try_from(INTERFACE).expect("interface"))
            .await
            .expect("GetAll");
        assert_eq!(
            String::try_from(all["PanelHtml"].clone()).expect("panel"),
            "<div class=\"panel\">7</div>"
        );
        assert_eq!(
            String::try_from(all["TooltipHtml"].clone()).expect("tooltip"),
            "<div class=\"tooltip\">8</div>"
        );
        assert_eq!(u32::try_from(&all["Page"]).expect("page"), 1);
        assert_eq!(u32::try_from(&all["PageCount"]).expect("count"), 3);
    });
}

#[test]
fn a_new_client_pipelining_calls_after_its_first_present_cannot_stall_the_service() {
    const STEPS: usize = 200;
    let bus = PrivateBus::start("pipeline");
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut service = start(
        &io.runtime_handle(),
        io.shutdown_receiver(),
        Some(bus.address.clone()),
    )
    .expect("applet service");

    client_runtime().block_on(async {
        let connection = client(&bus).await;
        let applet = proxy(&connection).await;
        applet
            .call_noreply("Present", &(1_u64,))
            .await
            .expect("present");
        for _ in 0..STEPS {
            applet
                .call_noreply("StepPage", &(1_i32,))
                .await
                .expect("step");
        }

        let mut steps = 0;
        while steps < STEPS {
            match next_event(&mut service).await {
                AppletEvent::Request(AppletRequest::StepPage(PageStep::Next)) => steps += 1,
                AppletEvent::Request(AppletRequest::Present { .. }) => {}
                other => panic!("unexpected event {other:?}"),
            }
        }
        let panel: String =
            tokio::time::timeout(Duration::from_secs(2), applet.get_property("PanelHtml"))
                .await
                .expect("service still answers")
                .expect("panel property");
        assert!(panel.is_empty());
    });
}

#[test]
fn a_second_service_fails_clearly_without_replacing_the_first() {
    let bus = PrivateBus::start("second");
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let _first = start(
        &io.runtime_handle(),
        io.shutdown_receiver(),
        Some(bus.address.clone()),
    )
    .expect("first service");
    let owner = client_runtime().block_on(async { name_owner(&client(&bus).await).await });

    let second = start(
        &io.runtime_handle(),
        io.shutdown_receiver(),
        Some(bus.address.clone()),
    );

    let error = second.err().expect("second service must fail").to_string();
    assert!(error.contains("another PlasmaTop daemon owns"), "{error}");
    let after = client_runtime().block_on(async { name_owner(&client(&bus).await).await });
    assert!(owner.is_some());
    assert_eq!(after, owner);
}

#[test]
fn a_client_disconnecting_right_after_present_is_reported_gone() {
    let bus = PrivateBus::start("disconnect");
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut service = start(
        &io.runtime_handle(),
        io.shutdown_receiver(),
        Some(bus.address.clone()),
    )
    .expect("applet service");

    client_runtime().block_on(async {
        let connection = client(&bus).await;
        let me = ClientId::new(connection.unique_name().expect("unique name").to_string());
        proxy(&connection)
            .await
            .call::<_, _, ()>("Present", &(1_u64,))
            .await
            .expect("present");
        connection.graceful_shutdown().await;

        assert!(matches!(
            next_event(&mut service).await,
            AppletEvent::Request(AppletRequest::Present { .. })
        ));
        assert_eq!(
            next_event(&mut service).await,
            AppletEvent::Request(AppletRequest::ClientGone { client: me })
        );
    });
}

#[test]
fn the_service_reacquires_its_name_after_its_bus_connection_drops() {
    let mut bus = PrivateBus::start("reconnect");
    let io = ProductionIo::start_without_signals().expect("I/O shell");
    let mut service = start(
        &io.runtime_handle(),
        io.shutdown_receiver(),
        Some(bus.address.clone()),
    )
    .expect("applet service");
    let mut publisher = service.publisher.clone();
    publisher.panel("kept panel");

    bus.restart();

    client_runtime().block_on(async {
        assert_eq!(next_event(&mut service).await, AppletEvent::ConnectionLost);
        let connection = client(&bus).await;
        tokio::time::timeout(Duration::from_secs(5), async {
            while name_owner(&connection).await.is_none() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("name reacquired");
        let panel: String = proxy(&connection)
            .await
            .get_property("PanelHtml")
            .await
            .expect("panel property");
        assert_eq!(panel, "kept panel");
    });
}
