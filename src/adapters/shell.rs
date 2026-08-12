use std::future::poll_fn;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, watch};

use crate::domain::boundary::IoEvent;
use crate::error::{CriticalService, Error, Result};

use super::command::{self, ProductionCommandRunner};
use super::dbus::{self, ProductionDbusFacade, ProductionNotificationFacade};

const EVENT_CAPACITY: usize = 1;
const SHELL_SHUTDOWN_BUDGET: Duration = Duration::from_millis(500);

/// Coalesced system-service events delivered to the synchronous daemon loop.
pub struct ProductionIoEvents {
    upower: std::sync::mpsc::Receiver<()>,
    sleep: Arc<Mutex<SleepEvents>>,
}

impl ProductionIoEvents {
    /// Drains coalesced sleep transitions and at most one UPower refresh.
    #[must_use]
    pub fn drain(&mut self) -> Vec<IoEvent> {
        let mut events = Vec::with_capacity(2);
        events.extend(drain_sleep_events(&self.sleep));
        if self.upower.try_recv().is_ok() {
            events.push(IoEvent::UpowerChanged);
        }
        events
    }
}

#[derive(Clone)]
pub(super) struct IoEventSink {
    upower: std::sync::mpsc::SyncSender<()>,
    sleep: Arc<Mutex<SleepEvents>>,
}

impl IoEventSink {
    pub(super) fn upower_changed(&self) {
        let _ = self.upower.try_send(());
    }

    pub(super) fn prepare_for_sleep(&self, preparing: bool) {
        match self.sleep.lock() {
            Ok(mut sleep) => sleep.push(preparing),
            Err(poisoned) => poisoned.into_inner().push(preparing),
        }
    }
}

fn event_stream() -> (IoEventSink, ProductionIoEvents) {
    let (upower_sender, upower) = std::sync::mpsc::sync_channel(EVENT_CAPACITY);
    let sleep = Arc::new(Mutex::new(SleepEvents::default()));
    (
        IoEventSink {
            upower: upower_sender,
            sleep: Arc::clone(&sleep),
        },
        ProductionIoEvents { upower, sleep },
    )
}

#[derive(Default)]
struct SleepEvents {
    latest: Option<bool>,
    dirty: bool,
    resume_pending: bool,
}

impl SleepEvents {
    fn push(&mut self, preparing: bool) {
        if self.latest == Some(preparing) {
            return;
        }
        self.resume_pending |= self.latest == Some(true) && !preparing;
        self.latest = Some(preparing);
        self.dirty = true;
    }

    fn drain(&mut self) -> Vec<IoEvent> {
        if !std::mem::take(&mut self.dirty) {
            return Vec::new();
        }
        let resume_pending = std::mem::take(&mut self.resume_pending);
        match (resume_pending, self.latest) {
            (true, Some(true)) => vec![
                IoEvent::PrepareForSleep(true),
                IoEvent::PrepareForSleep(false),
                IoEvent::PrepareForSleep(true),
            ],
            (true, Some(false)) => vec![
                IoEvent::PrepareForSleep(true),
                IoEvent::PrepareForSleep(false),
            ],
            (false, Some(preparing)) => vec![IoEvent::PrepareForSleep(preparing)],
            (_, None) => Vec::new(),
        }
    }
}

fn drain_sleep_events(sleep: &Mutex<SleepEvents>) -> Vec<IoEvent> {
    match sleep.lock() {
        Ok(mut sleep) => sleep.drain(),
        Err(poisoned) => poisoned.into_inner().drain(),
    }
}

/// Production handles backed by one manually built current-thread Tokio shell.
pub struct ProductionIo {
    commands: ProductionCommandRunner,
    dbus: ProductionDbusFacade,
    notifications: ProductionNotificationFacade,
    events: Option<ProductionIoEvents>,
    initial_system_bus_readiness: Option<dbus::InitialSystemBusReadiness>,
    stopped: Arc<AtomicBool>,
    external_shutdown: watch::Sender<bool>,
    orchestration_done: watch::Sender<bool>,
    orchestration_registered: Arc<AtomicBool>,
    critical_failure: Arc<Mutex<Option<CriticalService>>>,
    shutdown_timed_out: Arc<AtomicBool>,
    runtime: tokio::runtime::Handle,
    thread: Option<JoinHandle<()>>,
    #[cfg(test)]
    shell_stop_selected: std::sync::mpsc::Receiver<()>,
}

impl ProductionIo {
    /// Starts command, system-bus, session-bus, signal-stream, and Unix termination services with daemon orchestration expected.
    ///
    /// # Errors
    ///
    /// Returns a runtime error when the shell thread or Tokio runtime cannot be created.
    pub fn start() -> Result<Self> {
        Self::start_inner(true, true)
    }

    /// Starts I/O services without taking ownership of SIGINT/SIGTERM.
    ///
    /// Diagnostics use this mode so they cannot consume a daemon termination signal or reserve the runtime for orchestration completion.
    ///
    /// # Errors
    ///
    /// Returns a runtime error when the shell thread or Tokio runtime cannot be created.
    pub fn start_without_signals() -> Result<Self> {
        Self::start_inner(false, false)
    }

    /// Starts signal-free services with detailed counters for a timed profiling session.
    pub(crate) fn start_for_profiling() -> Result<Self> {
        Self::start_inner(false, true)
    }

    /// Starts diagnostics I/O services after the first system-bus attempt completes.
    ///
    /// The wait is bounded by the system-bus connection attempt itself. An unavailable bus leaves the service running so diagnostics can report absent D-Bus data.
    ///
    /// # Errors
    ///
    /// Returns a runtime error when the shell thread or Tokio runtime cannot be created.
    pub fn start_for_diagnostics() -> Result<Self> {
        let mut io = Self::start_without_signals()?;
        io.wait_for_initial_system_bus_attempt();
        Ok(io)
    }

    fn start_inner(capture_termination_signals: bool, expects_orchestration: bool) -> Result<Self> {
        let stopped = Arc::new(AtomicBool::new(false));
        let (command_sender, command_receiver) = mpsc::channel(command::COMMAND_QUEUE_CAPACITY);
        let (dbus_sender, dbus_receiver) = mpsc::channel(dbus::DBUS_QUEUE_CAPACITY);
        let (notification_sender, notification_receiver) = mpsc::channel(dbus::DBUS_QUEUE_CAPACITY);
        let (event_sender, event_receiver) = event_stream();
        let (initial_system_bus_sender, initial_system_bus_readiness) =
            dbus::initial_system_bus_readiness();
        let (external_shutdown, external_shutdown_receiver) = watch::channel(false);
        let (service_shutdown, service_shutdown_receiver) = watch::channel(false);
        let (orchestration_done, mut orchestration_done_receiver) = watch::channel(false);
        let orchestration_registered = Arc::new(AtomicBool::new(expects_orchestration));
        let thread_orchestration_registered = Arc::clone(&orchestration_registered);
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        #[cfg(test)]
        let (shell_stop_selected_sender, shell_stop_selected) = std::sync::mpsc::sync_channel(1);
        let thread_stopped = Arc::clone(&stopped);
        let critical_failure = Arc::new(Mutex::new(None));
        let thread_critical_failure = Arc::clone(&critical_failure);
        let shutdown_timed_out = Arc::new(AtomicBool::new(false));
        let thread_shutdown_timed_out = Arc::clone(&shutdown_timed_out);
        let signal_shutdown = external_shutdown.clone();
        let shell_shutdown = external_shutdown.clone();
        let thread = thread::Builder::new()
            .name("plasma-top-io".to_owned())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_io()
                    .enable_time()
                    .build();
                let Ok(runtime) = runtime else {
                    let _ = ready_sender.send(Err("cannot build Tokio runtime".to_owned()));
                    thread_stopped.store(true, Ordering::Release);
                    return;
                };
                let runtime_handle = runtime.handle().clone();
                let shutdown_started = runtime.block_on(async move {
                    let mut command_task = tokio::spawn(command::serve(
                        command_receiver,
                        service_shutdown_receiver.clone(),
                    ));
                    let mut system_task = tokio::spawn(dbus::serve_system(
                        dbus_receiver,
                        service_shutdown_receiver.clone(),
                        event_sender,
                        initial_system_bus_sender,
                    ));
                    let mut session_task = tokio::spawn(dbus::serve_session(
                        notification_receiver,
                        service_shutdown_receiver,
                    ));
                    let signal_task = if capture_termination_signals {
                        match spawn_termination_signals(
                            Arc::clone(&thread_stopped),
                            signal_shutdown.clone(),
                        ) {
                            Ok(task) => Some(task),
                            Err(error) => {
                                let _ = ready_sender.send(Err(error));
                                command_task.abort();
                                system_task.abort();
                                session_task.abort();
                                return Instant::now();
                            }
                        }
                    } else {
                        None
                    };
                    let _ = ready_sender.send(Ok(runtime_handle));
                    let mut wait_for_shutdown = external_shutdown_receiver;
                    let critical_service = wait_for_shell_stop(
                        &mut wait_for_shutdown,
                        &mut command_task,
                        &mut system_task,
                        &mut session_task,
                    )
                    .await;
                    #[cfg(test)]
                    let _ = shell_stop_selected_sender.send(());
                    let shutdown_started = Instant::now();
                    if let Some(task) = signal_task {
                        task.abort();
                    }
                    if let Some(critical_service) = critical_service {
                        thread_stopped.store(true, Ordering::Release);
                        if let Ok(mut failure) = thread_critical_failure.lock() {
                            *failure = Some(critical_service);
                        }
                        let _ = shell_shutdown.send(true);
                        let _ = service_shutdown.send(true);
                        tokio::time::sleep(
                            SHELL_SHUTDOWN_BUDGET.saturating_sub(Duration::from_millis(50)),
                        )
                        .await;
                        command_task.abort();
                        system_task.abort();
                        session_task.abort();
                    } else {
                        let _ = service_shutdown.send(true);
                        let joined = async {
                            let _ = command_task.await;
                            let _ = system_task.await;
                            let _ = session_task.await;
                        };
                        if tokio::time::timeout(
                            SHELL_SHUTDOWN_BUDGET.saturating_sub(Duration::from_millis(50)),
                            joined,
                        )
                        .await
                        .is_err()
                        {
                            thread_shutdown_timed_out.store(true, Ordering::Release);
                        }
                    }
                    let remaining =
                        SHELL_SHUTDOWN_BUDGET.saturating_sub(shutdown_started.elapsed());
                    let orchestration_timed_out = thread_orchestration_registered
                        .load(Ordering::Acquire)
                        && !*orchestration_done_receiver.borrow()
                        && tokio::time::timeout(remaining, orchestration_done_receiver.changed())
                            .await
                            .is_err();
                    if orchestration_timed_out {
                        thread_shutdown_timed_out.store(true, Ordering::Release);
                    }
                    shutdown_started
                });
                runtime.shutdown_timeout(
                    SHELL_SHUTDOWN_BUDGET.saturating_sub(shutdown_started.elapsed()),
                );
            })
            .map_err(|error| Error::Runtime(format!("cannot start async I/O shell: {error}")))?;
        let runtime = ready_receiver
            .recv()
            .map_err(|_| Error::Runtime("async I/O shell stopped during startup".to_owned()))?
            .map_err(Error::Runtime)?;
        Ok(Self {
            commands: ProductionCommandRunner::new(command_sender, Arc::clone(&stopped)),
            dbus: ProductionDbusFacade::new(dbus_sender, Arc::clone(&stopped)),
            notifications: ProductionNotificationFacade::new(
                notification_sender,
                Arc::clone(&stopped),
            ),
            events: Some(event_receiver),
            initial_system_bus_readiness: Some(initial_system_bus_readiness),
            stopped,
            external_shutdown,
            orchestration_done,
            orchestration_registered,
            critical_failure,
            shutdown_timed_out,
            runtime,
            thread: Some(thread),
            #[cfg(test)]
            shell_stop_selected,
        })
    }

    /// Returns the command-service handle.
    #[must_use]
    pub fn commands(&self) -> ProductionCommandRunner {
        self.commands.clone()
    }

    /// Returns the typed system-bus handle.
    #[must_use]
    pub fn dbus(&self) -> ProductionDbusFacade {
        self.dbus.clone()
    }

    /// Returns the typed session-notification handle.
    #[must_use]
    pub fn notifications(&self) -> ProductionNotificationFacade {
        self.notifications.clone()
    }

    /// Shared termination flag set by Tokio SIGINT/SIGTERM handling.
    #[must_use]
    pub fn stopped(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.stopped)
    }

    /// Returns the handle for spawning daemon orchestration onto the owned current-thread shell.
    pub(crate) fn runtime_handle(&self) -> tokio::runtime::Handle {
        self.runtime.clone()
    }

    /// Subscribes daemon orchestration to shell shutdown without polling.
    pub(crate) fn shutdown_receiver(&self) -> watch::Receiver<bool> {
        self.external_shutdown.subscribe()
    }

    /// Returns a profiling-only handle that initiates the same bounded shell shutdown as a signal.
    pub(crate) fn shutdown_request(&self) -> watch::Sender<bool> {
        self.external_shutdown.clone()
    }

    /// Marks the daemon orchestration task finished so the shell can close its runtime safely.
    pub(crate) fn orchestration_done(&self) -> watch::Sender<bool> {
        self.orchestration_registered.store(true, Ordering::Release);
        self.orchestration_done.clone()
    }

    /// Transfers the event receiver to the daemon loop.
    pub fn take_event_receiver(&mut self) -> Option<ProductionIoEvents> {
        self.events.take()
    }

    fn wait_for_initial_system_bus_attempt(&mut self) {
        if let Some(readiness) = self.initial_system_bus_readiness.take() {
            readiness.wait();
        }
    }

    /// Returns the unexpected critical-service failure, if any.
    #[must_use]
    pub fn critical_failure(&self) -> Option<CriticalService> {
        self.critical_failure
            .lock()
            .ok()
            .and_then(|failure| *failure)
    }

    /// Returns whether shutdown exceeded the shell budget.
    #[must_use]
    pub fn shutdown_timed_out(&self) -> bool {
        self.shutdown_timed_out.load(Ordering::Acquire)
    }

    /// Requests service shutdown and waits no longer than the shell budget.
    ///
    /// If the shell has not finished by the deadline, its join handle is dropped without joining so shutdown never blocks past the budget.
    pub fn shutdown(&mut self) {
        self.shutdown_with_budget(SHELL_SHUTDOWN_BUDGET);
    }

    /// Requests service shutdown with a caller-owned remaining process budget.
    pub(crate) fn shutdown_with_budget(&mut self, budget: Duration) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.external_shutdown.send(true);
        let Some(thread) = self.thread.take() else {
            return;
        };
        let deadline = Instant::now() + budget;
        if !join_finished_before(thread, deadline) {
            self.shutdown_timed_out.store(true, Ordering::Release);
        }
    }
}

fn join_finished_before(thread: JoinHandle<()>, deadline: Instant) -> bool {
    while !thread.is_finished() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return false;
        }
        thread::sleep(remaining.min(Duration::from_millis(2)));
    }
    let _ = thread.join();
    true
}

async fn wait_for_shell_stop(
    external_stop: &mut watch::Receiver<bool>,
    command: &mut tokio::task::JoinHandle<()>,
    system: &mut tokio::task::JoinHandle<()>,
    session: &mut tokio::task::JoinHandle<()>,
) -> Option<CriticalService> {
    let external_stop_requested = *external_stop.borrow();
    let mut external_stop_changed = Box::pin(external_stop.changed());
    poll_fn(|cx| {
        if Pin::new(&mut *command).poll(cx).is_ready() {
            return Poll::Ready(Some(CriticalService::Command));
        }
        if Pin::new(&mut *system).poll(cx).is_ready() {
            return Poll::Ready(Some(CriticalService::SystemDbus));
        }
        if Pin::new(&mut *session).poll(cx).is_ready() {
            return Poll::Ready(Some(CriticalService::SessionDbus));
        }
        if external_stop_requested || external_stop_changed.as_mut().poll(cx).is_ready() {
            return Poll::Ready(None);
        }
        Poll::Pending
    })
    .await
}

impl Drop for ProductionIo {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn spawn_termination_signals(
    stopped: Arc<AtomicBool>,
    shutdown: watch::Sender<bool>,
) -> std::result::Result<tokio::task::JoinHandle<()>, String> {
    let terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .map_err(|error| format!("cannot register SIGTERM: {error}"))?;
    let interrupt = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
        .map_err(|error| format!("cannot register SIGINT: {error}"))?;
    Ok(tokio::spawn(async move {
        wait_for_termination(terminate, interrupt).await;
        stopped.store(true, Ordering::Release);
        let _ = shutdown.send(true);
    }))
}

async fn wait_for_termination(
    mut terminate: tokio::signal::unix::Signal,
    mut interrupt: tokio::signal::unix::Signal,
) {
    let mut term = Box::pin(terminate.recv());
    let mut int = Box::pin(interrupt.recv());
    poll_fn(|cx| {
        if let Poll::Ready(value) = Pin::new(&mut term).poll(cx) {
            return Poll::Ready(value);
        }
        if let Poll::Ready(value) = Pin::new(&mut int).poll(cx) {
            return Poll::Ready(value);
        }
        Poll::Pending
    })
    .await;
}

#[cfg(test)]
#[path = "shell/tests.rs"]
mod tests;
