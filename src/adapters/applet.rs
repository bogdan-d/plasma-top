//! Session-bus service the Plasma applet talks to.
//!
//! The daemon owns [`BUS_NAME`] and exports one versioned interface. Method calls are validated into [`AppletRequest`]s and delivered, in receive order and with backpressure, to the daemon loop; publications update the exported properties and are announced with `PropertiesChanged`.

use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::{Notify, mpsc, watch};
use zbus::fdo::{self, DBusProxy, RequestNameFlags, RequestNameReply};
use zbus::message::Header;
use zbus::names::{BusName, InterfaceName, UniqueName};
use zbus::object_server::SignalEmitter;
use zbus::zvariant::Value;
use zbus::{Connection, interface};

use crate::domain::applet::{
    AppletEvent, AppletGeometry, AppletRequest, ClientId, InstanceId, PageStep, PublicationSink,
};
use crate::error::{Error, Result};

use super::dbus::ReconnectBackoff;

/// Well-known bus name owned by the production daemon.
pub const BUS_NAME: &str = "com.github.bogdan_d.PlasmaTop";
/// Object path of the applet interface.
pub const OBJECT_PATH: &str = "/com/github/bogdan_d/PlasmaTop";
/// Versioned applet interface; the trailing number changes on any incompatible change.
pub const INTERFACE: &str = "com.github.bogdan_d.PlasmaTop1";

const EVENT_CAPACITY: usize = 64;
const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
struct Published {
    panel: String,
    tooltip: String,
    page: u32,
    page_count: u32,
    dirty_panel: bool,
    dirty_tooltip: bool,
    dirty_selection: bool,
}

#[derive(Debug)]
struct Shared {
    published: Mutex<Published>,
    changed: Notify,
}

impl Shared {
    // The main page always exists, so the selection is valid before the daemon's first announcement.
    fn new() -> Self {
        Self {
            published: Mutex::new(Published {
                panel: String::new(),
                tooltip: String::new(),
                page: 0,
                page_count: 1,
                dirty_panel: false,
                dirty_tooltip: false,
                dirty_selection: false,
            }),
            changed: Notify::new(),
        }
    }

    fn published(&self) -> MutexGuard<'_, Published> {
        self.published
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Production publication sink: stores the exported property values and wakes the signal emitter.
#[derive(Debug, Clone)]
pub struct AppletPublisher(Arc<Shared>);

impl PublicationSink for AppletPublisher {
    fn panel(&mut self, html: &str) {
        let mut published = self.0.published();
        html.clone_into(&mut published.panel);
        published.dirty_panel = true;
        drop(published);
        self.0.changed.notify_one();
    }

    fn tooltip(&mut self, html: &str) {
        let mut published = self.0.published();
        html.clone_into(&mut published.tooltip);
        published.dirty_tooltip = true;
        drop(published);
        self.0.changed.notify_one();
    }

    fn selection(&mut self, page: u32, count: u32) {
        let mut published = self.0.published();
        published.page = page;
        published.page_count = count;
        published.dirty_selection = true;
        drop(published);
        self.0.changed.notify_one();
    }
}

/// A running applet service: its ordered events and the sink feeding its properties.
pub struct AppletService {
    /// Validated requests and service lifecycle events, in receive order.
    pub events: mpsc::Receiver<AppletEvent>,
    /// Publication sink for the exported properties.
    pub publisher: AppletPublisher,
}

/// Starts the applet service and waits, bounded, until it owns [`BUS_NAME`].
///
/// `address` selects a private bus; `None` uses the session bus. The service releases the name when `shutdown` turns true.
///
/// # Errors
///
/// Fails when the bus is unreachable, when another daemon owns the name, or when acquisition exceeds its time bound.
pub fn start(
    runtime: &tokio::runtime::Handle,
    shutdown: watch::Receiver<bool>,
    address: Option<String>,
) -> Result<AppletService> {
    let shared = Arc::new(Shared::new());
    let (events, receiver) = mpsc::channel(EVENT_CAPACITY);
    let (ready, ready_receiver) = std::sync::mpsc::sync_channel(1);
    runtime.spawn(serve(address, Arc::clone(&shared), events, shutdown, ready));
    match ready_receiver.recv_timeout(ACQUIRE_TIMEOUT.saturating_add(Duration::from_secs(1))) {
        Ok(Ok(())) => Ok(AppletService {
            events: receiver,
            publisher: AppletPublisher(shared),
        }),
        Ok(Err(reason)) => Err(Error::Runtime(reason)),
        Err(_) => Err(Error::Runtime(format!(
            "timed out acquiring {BUS_NAME} on the session bus"
        ))),
    }
}

enum Acquire {
    NameTaken,
    Bus(String),
}

/// One connection's name ownership plus the disconnect subscription made before the name was requested.
struct Acquired {
    connection: Connection,
    disconnects: fdo::NameOwnerChangedStream,
    known: Arc<Mutex<BTreeSet<String>>>,
    outbox: Arc<Outbox>,
}

/// Event path of one bus connection.
///
/// Retiring it orders `ConnectionLost` after every event the connection already delivered and drops any it delivers later, so buffered calls or disconnects of a dead connection (whose unique names a restarted bus reuses) cannot reach the fresh lease table.
struct Outbox {
    events: mpsc::Sender<AppletEvent>,
    retired: Mutex<bool>,
}

struct Retired;

impl Outbox {
    fn new(events: &mpsc::Sender<AppletEvent>) -> Self {
        Self {
            events: events.clone(),
            retired: Mutex::new(false),
        }
    }

    async fn send(&self, event: AppletEvent) -> std::result::Result<(), Retired> {
        let permit = self.events.reserve().await.map_err(|_| Retired)?;
        let retired = lock(&self.retired);
        if *retired {
            return Err(Retired);
        }
        permit.send(event);
        Ok(())
    }

    /// Sends `ConnectionLost` as this connection's last event; false when the daemon loop is gone.
    async fn retire(&self) -> bool {
        let Ok(permit) = self.events.reserve().await else {
            return false;
        };
        *lock(&self.retired) = true;
        permit.send(AppletEvent::ConnectionLost);
        true
    }
}

async fn serve(
    address: Option<String>,
    shared: Arc<Shared>,
    events: mpsc::Sender<AppletEvent>,
    mut shutdown: watch::Receiver<bool>,
    ready: std::sync::mpsc::SyncSender<std::result::Result<(), String>>,
) {
    let mut ready = Some(ready);
    let mut backoff = ReconnectBackoff::new();
    loop {
        if *shutdown.borrow() {
            return;
        }
        let attempt = tokio::select! {
            _ = shutdown.changed() => return,
            attempt = tokio::time::timeout(ACQUIRE_TIMEOUT, acquire(address.as_deref(), &shared, &events)) => attempt,
        };
        let failure = match attempt {
            Ok(Ok(acquired)) => {
                if let Some(ready) = ready.take() {
                    let _ = ready.send(Ok(()));
                }
                backoff.connected();
                let Acquired {
                    connection,
                    disconnects,
                    known,
                    outbox,
                } = acquired;
                let forwarder =
                    tokio::spawn(forward_disconnects(disconnects, known, Arc::clone(&outbox)));
                let emitter = tokio::spawn(emit_changes(connection.clone(), Arc::clone(&shared)));
                let lost = tokio::select! {
                    _ = shutdown.changed() => false,
                    () = connection.closed() => true,
                };
                emitter.abort();
                forwarder.abort();
                if !lost {
                    let _ = connection.release_name(BUS_NAME).await;
                    return;
                }
                eprintln!("[applet] session bus connection lost; reconnecting");
                if !outbox.retire().await {
                    return;
                }
                continue;
            }
            Ok(Err(Acquire::NameTaken)) => {
                let reason = format!("another PlasmaTop daemon owns {BUS_NAME}");
                match ready.take() {
                    Some(ready) => {
                        let _ = ready.send(Err(reason));
                    }
                    None => {
                        let _ = events.send(AppletEvent::ServiceFailed(reason)).await;
                    }
                }
                return;
            }
            Ok(Err(Acquire::Bus(detail))) => detail,
            Err(_) => String::from("timed out"),
        };
        if let Some(ready) = ready.take() {
            let _ = ready.send(Err(format!(
                "cannot serve the applet on the session bus: {failure}"
            )));
            return;
        }
        backoff.failed();
        tokio::select! {
            _ = shutdown.changed() => return,
            () = tokio::time::sleep(backoff.wait()) => {}
        }
    }
}

/// Connects, subscribes to client disconnects, installs the interface, then requests the name with neither queueing nor replacement.
///
/// Nothing outlives a cancelled attempt: the subscription is only consumed once the name is owned, so dropping the attempt closes its connection.
async fn acquire(
    address: Option<&str>,
    shared: &Arc<Shared>,
    events: &mpsc::Sender<AppletEvent>,
) -> std::result::Result<Acquired, Acquire> {
    let bus = |error: zbus::Error| Acquire::Bus(error.to_string());
    let connection = match address {
        Some(address) => zbus::connection::Builder::address(address)
            .map_err(bus)?
            .build()
            .await
            .map_err(bus)?,
        None => Connection::session().await.map_err(bus)?,
    };
    let known = Arc::new(Mutex::new(BTreeSet::new()));
    let outbox = Arc::new(Outbox::new(events));
    let disconnects = DBusProxy::new(&connection)
        .await
        .map_err(bus)?
        .receive_name_owner_changed()
        .await
        .map_err(bus)?;
    connection
        .object_server()
        .at(
            OBJECT_PATH,
            AppletInterface {
                shared: Arc::clone(shared),
                outbox: Arc::clone(&outbox),
                known: Arc::clone(&known),
            },
        )
        .await
        .map_err(bus)?;
    match connection
        .request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into())
        .await
    {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => Ok(Acquired {
            connection,
            disconnects,
            known,
            outbox,
        }),
        Ok(RequestNameReply::InQueue | RequestNameReply::Exists) | Err(zbus::Error::NameTaken) => {
            Err(Acquire::NameTaken)
        }
        Err(error) => Err(bus(error)),
    }
}

/// Reports departed clients that have called the interface; the stream ends with the connection.
async fn forward_disconnects(
    mut disconnects: fdo::NameOwnerChangedStream,
    known: Arc<Mutex<BTreeSet<String>>>,
    outbox: Arc<Outbox>,
) {
    use zbus::export::futures_core::Stream;
    while let Some(signal) =
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut disconnects).poll_next(cx)).await
    {
        let Ok(args) = signal.args() else {
            continue;
        };
        if args.new_owner().is_some() {
            continue;
        }
        let name = args.name().to_string();
        if !lock(&known).remove(&name) {
            continue;
        }
        let client = ClientId::new(name);
        if outbox
            .send(AppletEvent::Request(AppletRequest::ClientGone { client }))
            .await
            .is_err()
        {
            return;
        }
    }
}

/// Announces changed properties; one signal carries every property changed since the last one.
async fn emit_changes(connection: Connection, shared: Arc<Shared>) {
    let Ok(interface) = InterfaceName::try_from(INTERFACE) else {
        return;
    };
    let Ok(emitter) = SignalEmitter::new(&connection, OBJECT_PATH) else {
        return;
    };
    loop {
        shared.changed.notified().await;
        let mut changed: HashMap<&str, Value<'_>> = HashMap::new();
        {
            let mut published = shared.published();
            if std::mem::take(&mut published.dirty_panel) {
                changed.insert("PanelHtml", Value::from(published.panel.clone()));
            }
            if std::mem::take(&mut published.dirty_tooltip) {
                changed.insert("TooltipHtml", Value::from(published.tooltip.clone()));
            }
            if std::mem::take(&mut published.dirty_selection) {
                changed.insert("Page", Value::from(published.page));
                changed.insert("PageCount", Value::from(published.page_count));
            }
        }
        if changed.is_empty() {
            continue;
        }
        let _ = fdo::Properties::properties_changed(
            &emitter,
            interface.clone(),
            changed,
            Cow::Borrowed(&[]),
        )
        .await;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

struct AppletInterface {
    shared: Arc<Shared>,
    outbox: Arc<Outbox>,
    known: Arc<Mutex<BTreeSet<String>>>,
}

impl AppletInterface {
    async fn deliver(&self, request: AppletRequest) -> fdo::Result<()> {
        self.outbox
            .send(AppletEvent::Request(request))
            .await
            .map_err(|Retired| fdo::Error::Failed(String::from("service is not accepting calls")))
    }
}

fn sender(header: &Header<'_>) -> fdo::Result<String> {
    header
        .sender()
        .map(ToString::to_string)
        .ok_or_else(|| fdo::Error::InvalidArgs(String::from("call has no sender")))
}

fn instance(raw: u64) -> fdo::Result<InstanceId> {
    InstanceId::new(raw)
        .ok_or_else(|| fdo::Error::InvalidArgs(String::from("instance must be positive")))
}

// Calls are dispatched without per-call tasks, so one client's calls reach the daemon in receive order and a full event queue holds further calls back.
#[interface(name = "com.github.bogdan_d.PlasmaTop1", spawn = false)]
impl AppletInterface {
    async fn step_page(&self, delta: i32) -> fdo::Result<()> {
        let step = PageStep::from_delta(delta)
            .ok_or_else(|| fdo::Error::InvalidArgs(String::from("delta must be +1 or -1")))?;
        self.deliver(AppletRequest::StepPage(step)).await
    }

    async fn present(
        &self,
        instance_id: u64,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
    ) -> fdo::Result<()> {
        let instance = instance(instance_id)?;
        let name = sender(&header)?;
        let first = lock(&self.known).insert(name.clone());
        self.deliver(AppletRequest::Present {
            client: ClientId::new(name.clone()),
            instance,
        })
        .await?;
        if first {
            tokio::spawn(confirm_connected(
                connection.clone(),
                name,
                Arc::clone(&self.known),
                Arc::clone(&self.outbox),
            ));
        }
        Ok(())
    }

    async fn dismiss(
        &self,
        instance_id: u64,
        #[zbus(header)] header: Header<'_>,
    ) -> fdo::Result<()> {
        let instance = instance(instance_id)?;
        let client = ClientId::new(sender(&header)?);
        self.deliver(AppletRequest::Dismiss { client, instance })
            .await
    }

    async fn set_geometry(
        &self,
        usable_px: f64,
        glyph_advance: f64,
        vertical: bool,
        tooltip_advance: f64,
    ) -> fdo::Result<()> {
        let geometry = AppletGeometry::new(usable_px, glyph_advance, vertical, tooltip_advance)
            .ok_or_else(|| {
                fdo::Error::InvalidArgs(String::from(
                    "usable_px must be in (0, 16384] and advances in (0, 512]",
                ))
            })?;
        self.deliver(AppletRequest::Geometry(geometry)).await
    }

    #[zbus(property)]
    fn panel_html(&self) -> String {
        self.shared.published().panel.clone()
    }

    #[zbus(property)]
    fn tooltip_html(&self) -> String {
        self.shared.published().tooltip.clone()
    }

    #[zbus(property)]
    fn page(&self) -> u32 {
        self.shared.published().page
    }

    #[zbus(property)]
    fn page_count(&self) -> u32 {
        self.shared.published().page_count
    }
}

/// Reports a client whose disconnect was seen before it was known.
///
/// Runs after its first `Present` was delivered, so `ClientGone` follows it; it runs outside dispatch because waiting there for the bus reply could stall behind the caller's own queued calls.
async fn confirm_connected(
    connection: Connection,
    name: String,
    known: Arc<Mutex<BTreeSet<String>>>,
    outbox: Arc<Outbox>,
) {
    if still_connected(&connection, &name).await || !lock(&known).remove(&name) {
        return;
    }
    let _ = outbox
        .send(AppletEvent::Request(AppletRequest::ClientGone {
            client: ClientId::new(name),
        }))
        .await;
}

async fn still_connected(connection: &Connection, name: &str) -> bool {
    let Ok(unique) = UniqueName::try_from(name) else {
        return false;
    };
    match DBusProxy::new(connection).await {
        Ok(proxy) => proxy
            .name_has_owner(BusName::Unique(unique))
            .await
            .unwrap_or(true),
        Err(_) => true,
    }
}

#[cfg(test)]
#[path = "applet/tests.rs"]
mod tests;
