use std::collections::BTreeSet;
use std::ffi::OsString;
use std::future::poll_fn;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;
use std::time::{Duration, Instant};

use nix::sys::signal::{Signal, killpg};
use nix::unistd::Pid;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinSet;

use crate::domain::boundary::{
    BoundaryError, CommandOutput, CommandRunner, CommandStatus, CommandTruncation,
};

pub(super) const COMMAND_QUEUE_CAPACITY: usize = 2;
const MAX_COMMANDS: usize = 2;
const OUTPUT_LIMIT: usize = 1024 * 1024;

pub(super) struct CommandEnvelope {
    request: CommandRequest,
    reply: oneshot::Sender<Result<CommandOutput, BoundaryError>>,
}

#[derive(Debug)]
struct CommandRequest {
    program: PathBuf,
    args: Vec<OsString>,
    timeout: Duration,
}

/// Cloneable synchronous handle backed by the one bounded async command service.
#[derive(Debug, Clone)]
pub struct ProductionCommandRunner {
    sender: mpsc::Sender<CommandEnvelope>,
    stopped: Arc<AtomicBool>,
}

impl ProductionCommandRunner {
    pub(super) fn new(sender: mpsc::Sender<CommandEnvelope>, stopped: Arc<AtomicBool>) -> Self {
        Self { sender, stopped }
    }
}

impl CommandRunner for ProductionCommandRunner {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        timeout: Duration,
    ) -> Result<CommandOutput, BoundaryError> {
        let request = CommandRequest {
            program: program.to_path_buf(),
            args: args.to_vec(),
            timeout,
        };
        if self.stopped.load(Ordering::Acquire) {
            return Err(command_error(&request, "command service is shut down"));
        }
        let (reply, response) = oneshot::channel();
        self.sender
            .blocking_send(CommandEnvelope { request, reply })
            .map_err(|error| {
                command_error(
                    &error.0.request,
                    "command service is unavailable or shut down",
                )
            })?;
        response
            .blocking_recv()
            .map_err(|_| BoundaryError::CommandFailed {
                program: program.to_path_buf(),
                args: args.to_vec(),
                detail: "command service stopped before replying".to_owned(),
            })?
    }
}

pub(super) async fn serve(
    mut requests: mpsc::Receiver<CommandEnvelope>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut running = JoinSet::new();
    let active_groups = Arc::new(Mutex::new(BTreeSet::new()));
    loop {
        if *shutdown.borrow() {
            break;
        }
        if running.len() >= MAX_COMMANDS {
            let _ = wait_join_or_shutdown(&mut running, &mut shutdown).await;
            continue;
        }
        match wait_request_join_or_shutdown(&mut requests, &mut running, &mut shutdown).await {
            ServiceEvent::Request(envelope) => {
                let child_shutdown = shutdown.clone();
                let child_groups = Arc::clone(&active_groups);
                running.spawn(async move {
                    let result = execute(envelope.request, child_shutdown, child_groups).await;
                    let _ = envelope.reply.send(result);
                });
            }
            ServiceEvent::Joined => {}
            ServiceEvent::Shutdown | ServiceEvent::Closed => break,
        }
    }
    requests.close();
    while let Ok(envelope) = requests.try_recv() {
        let _ = envelope.reply.send(Err(command_error(
            &envelope.request,
            "command service is shut down",
        )));
    }
    kill_active_groups(&active_groups);
    while running.join_next().await.is_some() {}
}

enum ServiceEvent {
    Request(CommandEnvelope),
    Joined,
    Shutdown,
    Closed,
}

async fn wait_request_join_or_shutdown(
    requests: &mut mpsc::Receiver<CommandEnvelope>,
    running: &mut JoinSet<()>,
    shutdown: &mut watch::Receiver<bool>,
) -> ServiceEvent {
    let has_running = !running.is_empty();
    let mut request = Box::pin(requests.recv());
    let mut joined = Box::pin(running.join_next());
    let mut changed = Box::pin(shutdown.changed());
    poll_fn(|cx| {
        if let Poll::Ready(result) = changed.as_mut().poll(cx) {
            return Poll::Ready(if result.is_ok() {
                ServiceEvent::Shutdown
            } else {
                ServiceEvent::Closed
            });
        }
        if has_running && let Poll::Ready(Some(_)) = joined.as_mut().poll(cx) {
            return Poll::Ready(ServiceEvent::Joined);
        }
        match request.as_mut().poll(cx) {
            Poll::Ready(Some(envelope)) => Poll::Ready(ServiceEvent::Request(envelope)),
            Poll::Ready(None) => Poll::Ready(ServiceEvent::Closed),
            Poll::Pending => Poll::Pending,
        }
    })
    .await
}

async fn wait_join_or_shutdown(
    running: &mut JoinSet<()>,
    shutdown: &mut watch::Receiver<bool>,
) -> bool {
    let mut joined = Box::pin(running.join_next());
    let mut changed = Box::pin(shutdown.changed());
    poll_fn(|cx| {
        if changed.as_mut().poll(cx).is_ready() {
            return Poll::Ready(false);
        }
        if joined.as_mut().poll(cx).is_ready() {
            return Poll::Ready(true);
        }
        Poll::Pending
    })
    .await
}

async fn execute(
    request: CommandRequest,
    mut shutdown: watch::Receiver<bool>,
    active_groups: Arc<Mutex<BTreeSet<i32>>>,
) -> Result<CommandOutput, BoundaryError> {
    let deadline = Instant::now() + request.timeout;
    let mut command = Command::new(&request.program);
    command
        .args(&request.args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| command_error(&request, error.to_string()))?;
    let process_group = child.id().and_then(|id| i32::try_from(id).ok());
    let _active_group = ActiveProcessGroup::new(active_groups, process_group);
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| command_error(&request, "child stdout pipe is unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| command_error(&request, "child stderr pipe is unavailable"))?;
    let output = Arc::new(Mutex::new(OutputBudget::default()));
    let stdout_task = tokio::spawn(drain(stdout, Arc::clone(&output), Stream::Stdout));
    let stderr_task = tokio::spawn(drain(stderr, Arc::clone(&output), Stream::Stderr));

    let outcome = wait_for_child(&mut child, deadline, &mut shutdown).await;
    let status = match outcome {
        ChildOutcome::Exited(result) => result
            .map(status_from_exit)
            .map_err(|error| command_error(&request, error.to_string()))?,
        ChildOutcome::Timeout => {
            kill_and_reap(&mut child, process_group).await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(command_error(
                &request,
                format!("timed out after {:.3}s", request.timeout.as_secs_f64()),
            ));
        }
        ChildOutcome::Shutdown => {
            kill_and_reap(&mut child, process_group).await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(command_error(&request, "command service shut down"));
        }
    };
    let mut stdout_task = stdout_task;
    let mut stderr_task = stderr_task;
    match wait_for_drains(&mut stdout_task, &mut stderr_task, deadline, &mut shutdown).await {
        DrainOutcome::Drained { stdout, stderr } => {
            join_drain(stdout, &request, "stdout")?;
            join_drain(stderr, &request, "stderr")?;
        }
        DrainOutcome::Timeout { stdout, stderr } => {
            kill_and_reap(&mut child, process_group).await;
            if stdout.is_none() {
                let _ = stdout_task.await;
            }
            if stderr.is_none() {
                let _ = stderr_task.await;
            }
            return Err(command_error(
                &request,
                format!("timed out after {:.3}s", request.timeout.as_secs_f64()),
            ));
        }
        DrainOutcome::Shutdown { stdout, stderr } => {
            kill_and_reap(&mut child, process_group).await;
            if stdout.is_none() {
                let _ = stdout_task.await;
            }
            if stderr.is_none() {
                let _ = stderr_task.await;
            }
            return Err(command_error(&request, "command service shut down"));
        }
    }
    let output = Arc::into_inner(output)
        .ok_or_else(|| command_error(&request, "command output drain did not finish"))?
        .into_inner()
        .map_err(|_| command_error(&request, "command output drain lock was poisoned"))?;
    let (stdout, stderr, truncation) = output.into_parts();
    Ok(CommandOutput {
        program: request.program,
        args: request.args,
        status,
        stdout,
        stderr,
        truncation,
    })
}

enum ChildOutcome {
    Exited(io::Result<std::process::ExitStatus>),
    Timeout,
    Shutdown,
}

async fn wait_for_child(
    child: &mut tokio::process::Child,
    deadline: Instant,
    shutdown: &mut watch::Receiver<bool>,
) -> ChildOutcome {
    let mut wait = Box::pin(child.wait());
    let mut timer = Box::pin(tokio::time::sleep_until(tokio::time::Instant::from_std(
        deadline,
    )));
    let mut cancelled = Box::pin(wait_for_shutdown(shutdown));
    poll_fn(|cx| {
        if let Poll::Ready(result) = wait.as_mut().poll(cx) {
            return Poll::Ready(ChildOutcome::Exited(result));
        }
        if timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(ChildOutcome::Timeout);
        }
        if cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(ChildOutcome::Shutdown);
        }
        Poll::Pending
    })
    .await
}

async fn wait_for_shutdown(shutdown: &mut watch::Receiver<bool>) {
    while !*shutdown.borrow() {
        if shutdown.changed().await.is_err() {
            break;
        }
    }
}

async fn kill_and_reap(child: &mut tokio::process::Child, process_group: Option<i32>) {
    if let Some(process_group) = process_group {
        let _ = killpg(Pid::from_raw(process_group), Signal::SIGKILL);
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

struct ActiveProcessGroup {
    groups: Arc<Mutex<BTreeSet<i32>>>,
    process_group: Option<i32>,
}

impl ActiveProcessGroup {
    fn new(groups: Arc<Mutex<BTreeSet<i32>>>, process_group: Option<i32>) -> Self {
        if let Some(process_group) = process_group
            && let Ok(mut groups) = groups.lock()
        {
            groups.insert(process_group);
        }
        Self {
            groups,
            process_group,
        }
    }
}

impl Drop for ActiveProcessGroup {
    fn drop(&mut self) {
        if let Some(process_group) = self.process_group
            && let Ok(mut groups) = self.groups.lock()
        {
            groups.remove(&process_group);
        }
    }
}

fn kill_active_groups(groups: &Mutex<BTreeSet<i32>>) {
    if let Ok(groups) = groups.lock() {
        for process_group in groups.iter().copied() {
            let _ = killpg(Pid::from_raw(process_group), Signal::SIGKILL);
        }
    }
}

struct OutputBudget {
    bytes: Vec<u8>,
    stdout_len: usize,
    stderr_len: usize,
    stdout_discarded: u64,
    stderr_discarded: u64,
}

impl Default for OutputBudget {
    fn default() -> Self {
        Self {
            bytes: Vec::with_capacity(OUTPUT_LIMIT),
            stdout_len: 0,
            stderr_len: 0,
            stdout_discarded: 0,
            stderr_discarded: 0,
        }
    }
}

impl OutputBudget {
    fn append(&mut self, stream: Stream, bytes: &[u8]) {
        match stream {
            Stream::Stdout => self.append_stdout(bytes),
            Stream::Stderr => self.append_stderr(bytes),
        }
    }

    fn append_stdout(&mut self, bytes: &[u8]) {
        let keep = bytes
            .len()
            .min(OUTPUT_LIMIT.saturating_sub(self.stdout_len));
        let required = self
            .stdout_len
            .saturating_add(self.stderr_len)
            .saturating_add(keep)
            .saturating_sub(OUTPUT_LIMIT);
        if required != 0 {
            let discarded = required.min(self.stderr_len);
            self.stderr_len -= discarded;
            self.bytes.truncate(self.stdout_len + self.stderr_len);
            self.stderr_discarded = self.stderr_discarded.saturating_add(to_u64(discarded));
        }
        let stderr_start = self.stdout_len;
        let old_len = self.bytes.len();
        self.bytes.resize(old_len + keep, 0);
        self.bytes
            .copy_within(stderr_start..old_len, stderr_start + keep);
        self.bytes[stderr_start..stderr_start + keep].copy_from_slice(&bytes[..keep]);
        self.stdout_len += keep;
        self.stdout_discarded = self
            .stdout_discarded
            .saturating_add(to_u64(bytes.len() - keep));
    }

    fn append_stderr(&mut self, bytes: &[u8]) {
        let available = OUTPUT_LIMIT.saturating_sub(self.stdout_len + self.stderr_len);
        let keep = bytes.len().min(available);
        self.bytes.extend_from_slice(&bytes[..keep]);
        self.stderr_len += keep;
        self.stderr_discarded = self
            .stderr_discarded
            .saturating_add(to_u64(bytes.len() - keep));
    }

    fn into_parts(mut self) -> (Vec<u8>, Vec<u8>, CommandTruncation) {
        let mut stderr = self.bytes.split_off(self.stdout_len);
        self.bytes.shrink_to_fit();
        stderr.shrink_to_fit();
        (
            self.bytes,
            stderr,
            CommandTruncation {
                stdout_bytes: self.stdout_discarded,
                stderr_bytes: self.stderr_discarded,
            },
        )
    }
}

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

fn to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

async fn drain(
    mut pipe: impl AsyncRead + Unpin,
    output: Arc<Mutex<OutputBudget>>,
    stream: Stream,
) -> io::Result<()> {
    let mut chunk = [0_u8; 16 * 1024];
    loop {
        let count = pipe.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        let mut output = output
            .lock()
            .map_err(|_| io::Error::other("command output drain lock was poisoned"))?;
        output.append(stream, &chunk[..count]);
    }
    Ok(())
}

fn join_drain(
    result: Result<io::Result<()>, tokio::task::JoinError>,
    request: &CommandRequest,
    stream: &str,
) -> Result<(), BoundaryError> {
    result
        .map_err(|error| command_error(request, format!("{stream} drain task failed: {error}")))?
        .map_err(|error| command_error(request, format!("{stream} drain failed: {error}")))
}

enum DrainOutcome {
    Drained {
        stdout: Result<io::Result<()>, tokio::task::JoinError>,
        stderr: Result<io::Result<()>, tokio::task::JoinError>,
    },
    Timeout {
        stdout: Option<Result<io::Result<()>, tokio::task::JoinError>>,
        stderr: Option<Result<io::Result<()>, tokio::task::JoinError>>,
    },
    Shutdown {
        stdout: Option<Result<io::Result<()>, tokio::task::JoinError>>,
        stderr: Option<Result<io::Result<()>, tokio::task::JoinError>>,
    },
}

async fn wait_for_drains(
    stdout: &mut tokio::task::JoinHandle<io::Result<()>>,
    stderr: &mut tokio::task::JoinHandle<io::Result<()>>,
    deadline: Instant,
    shutdown: &mut watch::Receiver<bool>,
) -> DrainOutcome {
    let mut stdout_result = None;
    let mut stderr_result = None;
    let mut timer = Box::pin(tokio::time::sleep_until(tokio::time::Instant::from_std(
        deadline,
    )));
    let mut cancelled = Box::pin(wait_for_shutdown(shutdown));
    poll_fn(|cx| {
        if stdout_result.is_none()
            && let Poll::Ready(result) = Pin::new(&mut *stdout).poll(cx)
        {
            stdout_result = Some(result);
        }
        if stderr_result.is_none()
            && let Poll::Ready(result) = Pin::new(&mut *stderr).poll(cx)
        {
            stderr_result = Some(result);
        }
        if stdout_result.is_some()
            && stderr_result.is_some()
            && let (Some(stdout), Some(stderr)) = (stdout_result.take(), stderr_result.take())
        {
            return Poll::Ready(DrainOutcome::Drained { stdout, stderr });
        }
        if timer.as_mut().poll(cx).is_ready() {
            return Poll::Ready(DrainOutcome::Timeout {
                stdout: stdout_result.take(),
                stderr: stderr_result.take(),
            });
        }
        if cancelled.as_mut().poll(cx).is_ready() {
            return Poll::Ready(DrainOutcome::Shutdown {
                stdout: stdout_result.take(),
                stderr: stderr_result.take(),
            });
        }
        Poll::Pending
    })
    .await
}

fn status_from_exit(status: std::process::ExitStatus) -> CommandStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        status.code().map_or_else(
            || CommandStatus::Signal(status.signal().unwrap_or(0)),
            CommandStatus::Exit,
        )
    }
    #[cfg(not(unix))]
    {
        CommandStatus::Exit(status.code().unwrap_or(-1))
    }
}

fn command_error(request: &CommandRequest, detail: impl Into<String>) -> BoundaryError {
    BoundaryError::CommandFailed {
        program: request.program.clone(),
        args: request.args.clone(),
        detail: detail.into(),
    }
}
