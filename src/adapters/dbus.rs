use std::collections::HashMap;
use std::future::poll_fn;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::{JoinHandle, JoinSet};
use zbus::export::futures_core::Stream;
use zbus::fdo::{ObjectManagerProxy, PropertiesProxy};
use zbus::names::InterfaceName;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, MatchRule, MessageStream, Proxy};

use crate::domain::boundary::{
    BoundaryError, BusKind, DbusFacade, DbusOutput, DbusRequest, NotificationError,
    NotificationFacade, NotificationPayload, UdisksManagedObject, UdisksSmartKind,
    UpowerDeviceProperties,
};

use super::shell::IoEventSink;

pub(super) const DBUS_QUEUE_CAPACITY: usize = 2;
const MAX_CALLS: usize = 2;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(5);
const BACKOFF_MIN: Duration = Duration::from_millis(100);
const BACKOFF_MAX: Duration = Duration::from_secs(2);
const IDLE_WAKE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitialSystemBusAttempt {
    Connected,
    Unavailable,
}

pub(super) struct InitialSystemBusReadiness {
    receiver: std::sync::mpsc::Receiver<InitialSystemBusAttempt>,
}

pub(super) struct InitialSystemBusReadinessSender {
    sender: Option<std::sync::mpsc::SyncSender<InitialSystemBusAttempt>>,
}

pub(super) fn initial_system_bus_readiness()
-> (InitialSystemBusReadinessSender, InitialSystemBusReadiness) {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    (
        InitialSystemBusReadinessSender {
            sender: Some(sender),
        },
        InitialSystemBusReadiness { receiver },
    )
}

impl InitialSystemBusReadiness {
    pub(super) fn wait(self) {
        let _ = self.receiver.recv();
    }
}

impl InitialSystemBusReadinessSender {
    fn complete(&mut self, connected: bool) {
        let outcome = if connected {
            InitialSystemBusAttempt::Connected
        } else {
            InitialSystemBusAttempt::Unavailable
        };
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(outcome);
        }
    }
}

const UPOWER_NAME: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const UPOWER_IFACE: &str = "org.freedesktop.UPower";
const UPOWER_DEVICE_IFACE: &str = "org.freedesktop.UPower.Device";
const UDISKS_NAME: &str = "org.freedesktop.UDisks2";
const UDISKS_PATH: &str = "/org/freedesktop/UDisks2";
const UDISKS_BLOCK_IFACE: &str = "org.freedesktop.UDisks2.Block";
const NOTIFICATIONS_NAME: &str = "org.freedesktop.Notifications";
const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";
const NOTIFICATIONS_IFACE: &str = "org.freedesktop.Notifications";

struct ReconnectBackoff {
    delay: Duration,
    retry_at: Instant,
}

impl ReconnectBackoff {
    fn new() -> Self {
        Self {
            delay: BACKOFF_MIN,
            retry_at: Instant::now(),
        }
    }

    fn is_due(&self) -> bool {
        Instant::now() >= self.retry_at
    }

    fn wait(&self) -> Duration {
        self.retry_at.saturating_duration_since(Instant::now())
    }

    fn failed(&mut self) {
        self.retry_at = Instant::now() + self.delay;
        self.delay = self.delay.saturating_mul(2).min(BACKOFF_MAX);
    }

    fn connected(&mut self) {
        self.delay = BACKOFF_MIN;
        self.retry_at = Instant::now();
    }
}

pub(super) struct DbusEnvelope {
    request: DbusRequest,
    reply: oneshot::Sender<Result<DbusOutput, BoundaryError>>,
}

pub(super) struct NotificationEnvelope {
    payload: NotificationPayload,
    reply: oneshot::Sender<Result<(), NotificationError>>,
}

/// Cloneable synchronous handle backed by the persistent system-bus service.
#[derive(Debug, Clone)]
pub struct ProductionDbusFacade {
    sender: mpsc::Sender<DbusEnvelope>,
    stopped: Arc<AtomicBool>,
}

impl ProductionDbusFacade {
    pub(super) fn new(sender: mpsc::Sender<DbusEnvelope>, stopped: Arc<AtomicBool>) -> Self {
        Self { sender, stopped }
    }
}

impl DbusFacade for ProductionDbusFacade {
    fn call(&mut self, request: DbusRequest) -> Result<DbusOutput, BoundaryError> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(dbus_error(&request, "system D-Bus service is shut down"));
        }
        let (reply, response) = oneshot::channel();
        self.sender
            .blocking_send(DbusEnvelope { request, reply })
            .map_err(|error| dbus_error(&error.0.request, "system D-Bus service is unavailable"))?;
        response
            .blocking_recv()
            .map_err(|_| BoundaryError::DbusCallFailed {
                bus: BusKind::System,
                service: "system-bus-service".to_owned(),
                path: "/".to_owned(),
                interface: "service".to_owned(),
                member: "call".to_owned(),
                detail: "system D-Bus service stopped before replying".to_owned(),
            })?
    }
}

/// Cloneable synchronous notification handle backed by the persistent session-bus service.
#[derive(Debug, Clone)]
pub struct ProductionNotificationFacade {
    sender: mpsc::Sender<NotificationEnvelope>,
    stopped: Arc<AtomicBool>,
}

impl ProductionNotificationFacade {
    pub(super) fn new(
        sender: mpsc::Sender<NotificationEnvelope>,
        stopped: Arc<AtomicBool>,
    ) -> Self {
        Self { sender, stopped }
    }
}

impl NotificationFacade for ProductionNotificationFacade {
    fn send(&mut self, payload: &NotificationPayload) -> Result<(), NotificationError> {
        if self.stopped.load(Ordering::Acquire) {
            return Err(NotificationError {
                detail: "session D-Bus service is shut down".to_owned(),
            });
        }
        let (reply, response) = oneshot::channel();
        self.sender
            .blocking_send(NotificationEnvelope {
                payload: payload.clone(),
                reply,
            })
            .map_err(|_| NotificationError {
                detail: "session D-Bus service is unavailable".to_owned(),
            })?;
        response.blocking_recv().map_err(|_| NotificationError {
            detail: "session D-Bus service stopped before replying".to_owned(),
        })?
    }
}

pub(super) async fn serve_system(
    requests: mpsc::Receiver<DbusEnvelope>,
    shutdown: watch::Receiver<bool>,
    events: IoEventSink,
    initial_readiness: InitialSystemBusReadinessSender,
) {
    serve_bus(requests, shutdown, events, initial_readiness).await;
}

fn spawn_connection(bus: BusKind) -> JoinHandle<Option<Connection>> {
    tokio::spawn(async move {
        match bus {
            BusKind::System => tokio::time::timeout(DEFAULT_TIMEOUT, Connection::system())
                .await
                .ok()?
                .ok(),
            BusKind::Session => tokio::time::timeout(DEFAULT_TIMEOUT, Connection::session())
                .await
                .ok()?
                .ok(),
        }
    })
}

async fn serve_bus(
    mut requests: mpsc::Receiver<DbusEnvelope>,
    mut shutdown: watch::Receiver<bool>,
    events: IoEventSink,
    mut initial_readiness: InitialSystemBusReadinessSender,
) {
    let mut connection = None;
    let mut connection_generation = 0_u64;
    let mut connecting: Option<JoinHandle<Option<Connection>>> = None;
    let mut calls = JoinSet::new();
    let mut signals: Option<JoinHandle<()>> = None;
    let mut reconnect = ReconnectBackoff::new();
    loop {
        if *shutdown.borrow() {
            break;
        }
        if connection.is_none() && connecting.is_none() && reconnect.is_due() {
            connecting = Some(spawn_connection(BusKind::System));
        }
        let wake_after = if connection.is_none() && connecting.is_none() {
            reconnect.wait()
        } else {
            IDLE_WAKE
        };
        let accept_request = calls.len() < MAX_CALLS;
        match wait_system_event(
            &mut requests,
            &mut calls,
            &mut connecting,
            &mut signals,
            &mut shutdown,
            wake_after,
            accept_request,
        )
        .await
        {
            SystemEvent::Request(envelope) => {
                let Some((generation, active)) = connection.clone() else {
                    let _ = envelope.reply.send(Err(dbus_error(
                        &envelope.request,
                        "system bus is disconnected; reconnect is pending",
                    )));
                    continue;
                };
                calls.spawn(async move {
                    let call = execute_system(&active, &envelope.request).await;
                    let _ = envelope.reply.send(call.result);
                    SystemCallCompletion {
                        generation,
                        disconnected: call.disconnected,
                    }
                });
            }
            SystemEvent::Joined(disconnected) => {
                if let Some(disconnected) = disconnected {
                    if connection_matches(
                        connection.as_ref().map(|(generation, _)| *generation),
                        disconnected.generation,
                    ) && disconnected.disconnected
                    {
                        connection = None;
                        if let Some(task) = signals.take() {
                            task.abort();
                        }
                        reconnect.failed();
                    }
                }
            }
            SystemEvent::Connected(result) => {
                connecting = None;
                let connected = result.is_some();
                if let Some(new_connection) = result {
                    signals = Some(spawn_system_signals(new_connection.clone(), events.clone()));
                    connection_generation = connection_generation.checked_add(1).unwrap_or(1);
                    connection =
                        Some((ConnectionGeneration(connection_generation), new_connection));
                    reconnect.connected();
                } else {
                    reconnect.failed();
                }
                initial_readiness.complete(connected);
            }
            SystemEvent::Wake => {}
            SystemEvent::SignalsEnded => {
                connection = None;
                signals = None;
                reconnect.failed();
            }
            SystemEvent::Shutdown | SystemEvent::Closed => break,
        }
    }
    requests.close();
    if let Some(task) = connecting {
        task.abort();
    }
    if let Some(task) = signals {
        task.abort();
    }
    while let Ok(envelope) = requests.try_recv() {
        let _ = envelope.reply.send(Err(dbus_error(
            &envelope.request,
            "system D-Bus service is shut down",
        )));
    }
    calls.abort_all();
    while calls.join_next().await.is_some() {}
    initial_readiness.complete(false);
}

pub(super) async fn serve_session(
    mut requests: mpsc::Receiver<NotificationEnvelope>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut connection = None;
    let mut connection_generation = 0_u64;
    let mut connecting: Option<JoinHandle<Option<Connection>>> = None;
    let mut calls = JoinSet::new();
    let mut reconnect = ReconnectBackoff::new();
    loop {
        if *shutdown.borrow() {
            break;
        }
        if connection.is_none() && connecting.is_none() && reconnect.is_due() {
            connecting = Some(spawn_connection(BusKind::Session));
        }
        let wake_after = if connection.is_none() && connecting.is_none() {
            reconnect.wait()
        } else {
            IDLE_WAKE
        };
        let accept_request = calls.len() < MAX_CALLS;
        match wait_session_event(
            &mut requests,
            &mut calls,
            &mut connecting,
            &mut shutdown,
            wake_after,
            accept_request,
        )
        .await
        {
            SessionEvent::Request(envelope) => {
                let Some((generation, active)) = connection.clone() else {
                    let _ = envelope.reply.send(Err(NotificationError {
                        detail: "session bus is disconnected; reconnect is pending".to_owned(),
                    }));
                    continue;
                };
                calls.spawn(async move {
                    let call = send_notification(&active, &envelope.payload).await;
                    let _ = envelope.reply.send(call.result);
                    SessionCallCompletion {
                        generation,
                        disconnected: call.disconnected,
                    }
                });
            }
            SessionEvent::Joined(disconnected) => {
                if let Some(disconnected) = disconnected {
                    if connection_matches(
                        connection.as_ref().map(|(generation, _)| *generation),
                        disconnected.generation,
                    ) && disconnected.disconnected
                    {
                        connection = None;
                        reconnect.failed();
                    }
                }
            }
            SessionEvent::Connected(result) => {
                connecting = None;
                if let Some(new_connection) = result {
                    connection_generation = connection_generation.checked_add(1).unwrap_or(1);
                    connection =
                        Some((ConnectionGeneration(connection_generation), new_connection));
                    reconnect.connected();
                } else {
                    reconnect.failed();
                }
            }
            SessionEvent::Wake => {}
            SessionEvent::Shutdown | SessionEvent::Closed => break,
        }
    }
    requests.close();
    if let Some(task) = connecting {
        task.abort();
    }
    while let Ok(envelope) = requests.try_recv() {
        let _ = envelope.reply.send(Err(NotificationError {
            detail: "session D-Bus service is shut down".to_owned(),
        }));
    }
    calls.abort_all();
    while calls.join_next().await.is_some() {}
}

struct SystemCall {
    result: Result<DbusOutput, BoundaryError>,
    disconnected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConnectionGeneration(u64);

struct SystemCallCompletion {
    generation: ConnectionGeneration,
    disconnected: bool,
}

struct SessionCallCompletion {
    generation: ConnectionGeneration,
    disconnected: bool,
}

fn connection_matches(
    active: Option<ConnectionGeneration>,
    completed: ConnectionGeneration,
) -> bool {
    active == Some(completed)
}

async fn execute_system(connection: &Connection, request: &DbusRequest) -> SystemCall {
    let timeout = match request {
        DbusRequest::UdisksSmartUpdate { timeout, .. } => *timeout,
        DbusRequest::UpowerEnumerate
        | DbusRequest::UpowerDeviceProperties { .. }
        | DbusRequest::UdisksManagedObjects
        | DbusRequest::UdisksSmartProperty { .. } => DEFAULT_TIMEOUT,
    };
    match tokio::time::timeout(timeout, execute_system_inner(connection, request)).await {
        Ok(Ok(output)) => SystemCall {
            result: Ok(output),
            disconnected: false,
        },
        Ok(Err(error)) => SystemCall {
            disconnected: is_disconnect_error(&error),
            result: Err(dbus_error(request, error.to_string())),
        },
        Err(_) => SystemCall {
            result: Err(dbus_error(
                request,
                format!("timed out after {:.3}s", timeout.as_secs_f64()),
            )),
            disconnected: false,
        },
    }
}

async fn execute_system_inner(
    connection: &Connection,
    request: &DbusRequest,
) -> zbus::Result<DbusOutput> {
    match request {
        DbusRequest::UpowerEnumerate => {
            let proxy = Proxy::new(connection, UPOWER_NAME, UPOWER_PATH, UPOWER_IFACE).await?;
            let paths: Vec<OwnedObjectPath> = proxy.call("EnumerateDevices", &()).await?;
            Ok(DbusOutput::UpowerDevices(
                paths.into_iter().map(|path| path.to_string()).collect(),
            ))
        }
        DbusRequest::UpowerDeviceProperties { object_path } => {
            let proxy = PropertiesProxy::builder(connection)
                .destination(UPOWER_NAME)?
                .path(object_path.as_str())?
                .build()
                .await?;
            let properties = proxy
                .get_all(InterfaceName::try_from(UPOWER_DEVICE_IFACE)?)
                .await?;
            Ok(DbusOutput::UpowerDeviceProperties(upower_properties(
                properties,
            )))
        }
        DbusRequest::UdisksManagedObjects => {
            let proxy = ObjectManagerProxy::builder(connection)
                .destination(UDISKS_NAME)?
                .path(UDISKS_PATH)?
                .build()
                .await?;
            let objects = proxy.get_managed_objects().await?;
            let mut selected = objects
                .into_iter()
                .map(|(path, interfaces)| {
                    let drive = interfaces
                        .get(UDISKS_BLOCK_IFACE)
                        .and_then(|properties| properties.get("Drive"))
                        .and_then(|value| ObjectPath::try_from(&**value).ok())
                        .map(|path| path.to_string());
                    UdisksManagedObject {
                        path: path.to_string(),
                        interfaces: interfaces.keys().map(ToString::to_string).collect(),
                        drive,
                    }
                })
                .collect::<Vec<_>>();
            selected.sort_by(|left, right| left.path.cmp(&right.path));
            Ok(DbusOutput::UdisksManagedObjects(selected))
        }
        DbusRequest::UdisksSmartUpdate {
            object_path, kind, ..
        } => {
            let proxy = Proxy::new(
                connection,
                UDISKS_NAME,
                object_path.as_str(),
                kind.interface(),
            )
            .await?;
            let options = HashMap::<&str, Value<'_>>::new();
            proxy.call::<_, _, ()>("SmartUpdate", &(options,)).await?;
            Ok(DbusOutput::UdisksSmartUpdated)
        }
        DbusRequest::UdisksSmartProperty { object_path, kind } => {
            let proxy = PropertiesProxy::builder(connection)
                .destination(UDISKS_NAME)?
                .path(object_path.as_str())?
                .build()
                .await?;
            let interface = InterfaceName::try_from(kind.interface())?;
            match kind {
                UdisksSmartKind::Nvme => {
                    let value = proxy.get(interface, "SmartCriticalWarning").await?;
                    Ok(DbusOutput::UdisksNvmeCriticalWarnings(
                        Vec::<String>::try_from(value)?,
                    ))
                }
                UdisksSmartKind::Ata => {
                    let value = proxy.get(interface, "SmartFailing").await?;
                    Ok(DbusOutput::UdisksAtaFailing(bool::try_from(value)?))
                }
            }
        }
    }
}

fn upower_properties(mut values: HashMap<String, OwnedValue>) -> UpowerDeviceProperties {
    UpowerDeviceProperties {
        percentage: values
            .remove("Percentage")
            .and_then(|value| f64::try_from(value).ok()),
        state: values
            .remove("State")
            .and_then(|value| u32::try_from(value).ok()),
        energy_rate: values
            .remove("EnergyRate")
            .and_then(|value| f64::try_from(value).ok()),
        model: values
            .remove("Model")
            .and_then(|value| String::try_from(value).ok()),
        kind: values
            .remove("Type")
            .and_then(|value| u32::try_from(value).ok()),
    }
}

struct NotificationCall {
    result: Result<(), NotificationError>,
    disconnected: bool,
}

async fn send_notification(
    connection: &Connection,
    payload: &NotificationPayload,
) -> NotificationCall {
    let operation = async {
        let proxy = Proxy::new(
            connection,
            NOTIFICATIONS_NAME,
            NOTIFICATIONS_PATH,
            NOTIFICATIONS_IFACE,
        )
        .await?;
        let actions: Vec<&str> = Vec::new();
        let mut hints = HashMap::<&str, Value<'_>>::new();
        hints.insert("urgency", Value::from(2_u8));
        let _: u32 = proxy
            .call(
                "Notify",
                &(
                    "PlasmaTop",
                    0_u32,
                    payload.icon.as_str(),
                    payload.title.as_str(),
                    payload.body.as_str(),
                    actions,
                    hints,
                    0_i32,
                ),
            )
            .await?;
        Ok::<(), zbus::Error>(())
    };
    match tokio::time::timeout(DEFAULT_TIMEOUT, operation).await {
        Ok(Ok(())) => NotificationCall {
            result: Ok(()),
            disconnected: false,
        },
        Ok(Err(error)) => NotificationCall {
            disconnected: is_disconnect_error(&error),
            result: Err(NotificationError {
                detail: error.to_string(),
            }),
        },
        Err(_) => NotificationCall {
            result: Err(NotificationError {
                detail: "Notify timed out after 5.000s".to_owned(),
            }),
            disconnected: false,
        },
    }
}

fn is_disconnect_error(error: &zbus::Error) -> bool {
    matches!(
        error,
        zbus::Error::Connection(_, _) | zbus::Error::InputOutput(_)
    )
}

fn spawn_system_signals(connection: Connection, events: IoEventSink) -> JoinHandle<()> {
    tokio::spawn(async move {
        let _ = monitor_system_signals(&connection, &events).await;
    })
}

async fn monitor_system_signals(connection: &Connection, events: &IoEventSink) -> zbus::Result<()> {
    let upower_rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(UPOWER_NAME)?
        .interface(UPOWER_IFACE)?
        .build();
    let properties_rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(UPOWER_NAME)?
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .path_namespace(UPOWER_PATH)?
        .build();
    let sleep_rule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.login1")?
        .interface("org.freedesktop.login1.Manager")?
        .member("PrepareForSleep")?
        .path("/org/freedesktop/login1")?
        .build();
    let mut upower = MessageStream::for_match_rule(upower_rule, connection, Some(1)).await?;
    let mut properties =
        MessageStream::for_match_rule(properties_rule, connection, Some(1)).await?;
    let mut sleep = MessageStream::for_match_rule(sleep_rule, connection, Some(1)).await?;
    events.upower_changed();
    loop {
        match next_signal(&mut upower, &mut properties, &mut sleep).await {
            SignalEvent::Upower => {
                events.upower_changed();
            }
            SignalEvent::Sleep(message) => {
                if let Ok((preparing,)) = message.body().deserialize::<(bool,)>() {
                    events.prepare_for_sleep(preparing);
                }
            }
            SignalEvent::Closed => return Ok(()),
        }
    }
}

enum SignalEvent {
    Upower,
    Sleep(zbus::Message),
    Closed,
}

async fn next_signal(
    upower: &mut MessageStream,
    properties: &mut MessageStream,
    sleep: &mut MessageStream,
) -> SignalEvent {
    poll_fn(|cx| {
        if let Poll::Ready(value) = Pin::new(&mut *upower).poll_next(cx) {
            return Poll::Ready(value.map_or(SignalEvent::Closed, |_| SignalEvent::Upower));
        }
        if let Poll::Ready(value) = Pin::new(&mut *properties).poll_next(cx) {
            return Poll::Ready(value.map_or(SignalEvent::Closed, |_| SignalEvent::Upower));
        }
        if let Poll::Ready(value) = Pin::new(&mut *sleep).poll_next(cx) {
            return Poll::Ready(value.map_or(SignalEvent::Closed, |result| match result {
                Ok(message) => SignalEvent::Sleep(message),
                Err(_) => SignalEvent::Closed,
            }));
        }
        Poll::Pending
    })
    .await
}

enum SystemEvent {
    Request(DbusEnvelope),
    Joined(Option<SystemCallCompletion>),
    Connected(Option<Connection>),
    Wake,
    SignalsEnded,
    Shutdown,
    Closed,
}

async fn wait_system_event(
    requests: &mut mpsc::Receiver<DbusEnvelope>,
    calls: &mut JoinSet<SystemCallCompletion>,
    connecting: &mut Option<JoinHandle<Option<Connection>>>,
    signals: &mut Option<JoinHandle<()>>,
    shutdown: &mut watch::Receiver<bool>,
    wake_after: Duration,
    accept_request: bool,
) -> SystemEvent {
    let has_calls = !calls.is_empty();
    let is_connecting = connecting.is_some();
    let has_signals = signals.is_some();
    let mut request = Box::pin(requests.recv());
    let mut joined = Box::pin(calls.join_next());
    let mut changed = Box::pin(shutdown.changed());
    let mut wake = Box::pin(tokio::time::sleep(wake_after));
    let mut signal_finished = Box::pin(async {
        if let Some(task) = signals.as_mut() {
            let _ = task.await;
        }
    });
    let mut connection_finished = Box::pin(async {
        if let Some(task) = connecting.as_mut() {
            return task.await.ok().flatten();
        }
        None
    });
    poll_fn(|cx| {
        if changed.as_mut().poll(cx).is_ready() {
            return Poll::Ready(SystemEvent::Shutdown);
        }
        if has_calls {
            if let Poll::Ready(Some(result)) = joined.as_mut().poll(cx) {
                return Poll::Ready(SystemEvent::Joined(result.ok()));
            }
        }
        if is_connecting {
            if let Poll::Ready(result) = connection_finished.as_mut().poll(cx) {
                return Poll::Ready(SystemEvent::Connected(result));
            }
        }
        if has_signals && signal_finished.as_mut().poll(cx).is_ready() {
            return Poll::Ready(SystemEvent::SignalsEnded);
        }
        if accept_request {
            match request.as_mut().poll(cx) {
                Poll::Ready(Some(envelope)) => return Poll::Ready(SystemEvent::Request(envelope)),
                Poll::Ready(None) => return Poll::Ready(SystemEvent::Closed),
                Poll::Pending => {}
            }
        }
        if wake.as_mut().poll(cx).is_ready() {
            return Poll::Ready(SystemEvent::Wake);
        }
        Poll::Pending
    })
    .await
}

enum SessionEvent {
    Request(NotificationEnvelope),
    Joined(Option<SessionCallCompletion>),
    Connected(Option<Connection>),
    Wake,
    Shutdown,
    Closed,
}

async fn wait_session_event(
    requests: &mut mpsc::Receiver<NotificationEnvelope>,
    calls: &mut JoinSet<SessionCallCompletion>,
    connecting: &mut Option<JoinHandle<Option<Connection>>>,
    shutdown: &mut watch::Receiver<bool>,
    wake_after: Duration,
    accept_request: bool,
) -> SessionEvent {
    let has_calls = !calls.is_empty();
    let is_connecting = connecting.is_some();
    let mut request = Box::pin(requests.recv());
    let mut joined = Box::pin(calls.join_next());
    let mut changed = Box::pin(shutdown.changed());
    let mut wake = Box::pin(tokio::time::sleep(wake_after));
    let mut connection_finished = Box::pin(async {
        if let Some(task) = connecting.as_mut() {
            return task.await.ok().flatten();
        }
        None
    });
    poll_fn(|cx| {
        if changed.as_mut().poll(cx).is_ready() {
            return Poll::Ready(SessionEvent::Shutdown);
        }
        if has_calls {
            if let Poll::Ready(Some(result)) = joined.as_mut().poll(cx) {
                return Poll::Ready(SessionEvent::Joined(result.ok()));
            }
        }
        if is_connecting {
            if let Poll::Ready(result) = connection_finished.as_mut().poll(cx) {
                return Poll::Ready(SessionEvent::Connected(result));
            }
        }
        if accept_request {
            match request.as_mut().poll(cx) {
                Poll::Ready(Some(envelope)) => return Poll::Ready(SessionEvent::Request(envelope)),
                Poll::Ready(None) => return Poll::Ready(SessionEvent::Closed),
                Poll::Pending => {}
            }
        }
        if wake.as_mut().poll(cx).is_ready() {
            return Poll::Ready(SessionEvent::Wake);
        }
        Poll::Pending
    })
    .await
}

fn dbus_error(request: &DbusRequest, detail: impl Into<String>) -> BoundaryError {
    let (bus, service, path, interface, member) = request.metadata();
    BoundaryError::DbusCallFailed {
        bus,
        service: service.to_owned(),
        path: path.to_owned(),
        interface: interface.to_owned(),
        member: member.to_owned(),
        detail: detail.into(),
    }
}

#[cfg(test)]
#[path = "dbus/tests.rs"]
mod tests;
