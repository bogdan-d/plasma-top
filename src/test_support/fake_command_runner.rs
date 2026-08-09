//! Command-runner boundary and its in-memory fake.
//!
//! [`CommandRunner`] is implemented by the production adapter in
//! `crate::adapters` and this in-memory fake. Sensor/daemon
//! code accepts `impl CommandRunner` (or `&mut dyn CommandRunner`) so tests
//! can inject [`FakeCommandRunner`] with argv-keyed replies and an ordered
//! call trace, with no child process ever spawned.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::domain::boundary::{BoundaryError, CommandOutput, CommandRunner};

type CommandKey = (PathBuf, Vec<OsString>);
type QueuedCommandResult = Result<CommandOutput, BoundaryError>;

/// In-memory `CommandRunner` fake keyed by `(program, args)`.
///
/// Each enqueued reply is popped FIFO when the matching argv is invoked, so
/// repeated calls to the same command return distinct replies in registration
/// order. Every invocation is appended to a call trace for differential
/// assertions against the Python oracle.
///
/// # Examples
///
/// ```
/// use std::ffi::OsString;
/// use std::path::Path;
///
/// use plasma_top::domain::boundary::{CommandOutput, CommandRunner, CommandStatus};
/// use plasma_top::test_support::FakeCommandRunner;
///
/// let mut runner = FakeCommandRunner::new();
/// runner.enqueue(
///     "/usr/bin/ip",
///     ["-j", "route"],
///     CommandOutput {
///         program: "/usr/bin/ip".into(),
///         args: [OsString::from("-j"), OsString::from("route")].to_vec(),
///         status: CommandStatus::Exit(0),
///         stdout: b"[]".to_vec(),
///         stderr: Vec::new(),
///     },
/// );
///
/// let out = runner
///     .run(
///         Path::new("/usr/bin/ip"),
///         &[OsString::from("-j"), OsString::from("route")],
///         std::time::Duration::from_secs(3),
///     )
///     .expect("enqueued reply must be returned");
/// assert_eq!(out.stdout, b"[]");
/// ```
#[derive(Debug, Default)]
pub struct FakeCommandRunner {
    /// Argv-keyed FIFO of pending replies.
    outputs: HashMap<CommandKey, VecDeque<QueuedCommandResult>>,
    /// Ordered signatures of every invocation seen by this fake.
    call_trace: Vec<CommandCall>,
}

/// One exact command invocation recorded by [`FakeCommandRunner`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandCall {
    /// Resolved executable path.
    pub program: PathBuf,
    /// Exact child arguments.
    pub args: Vec<OsString>,
    /// Requested execution timeout.
    pub timeout: std::time::Duration,
}

impl FakeCommandRunner {
    /// Creates an empty fake with no queued replies and an empty call trace.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueues a reply for the given argv, returning `&mut self` for chaining.
    ///
    /// Multiple [`enqueue`](Self::enqueue) calls for the same argv return in
    /// FIFO order: the first matching invocation gets the first reply, the
    /// second invocation gets the second reply, and so on.
    ///
    /// Note: this is a mutating `&mut self` builder, not a consuming one, so
    /// it is intentionally *not* marked `#[must_use]` — the side effect
    /// (recording the reply) happens regardless of whether the caller chains.
    /// The `api-builder-must-use` rule targets consuming `self -> Self`
    /// builders where dropping the return loses the work.
    pub fn enqueue(
        &mut self,
        program: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl Into<OsString>>,
        output: CommandOutput,
    ) -> &mut Self {
        let key = (
            program.into(),
            args.into_iter().map(|a| a.into()).collect::<Vec<_>>(),
        );
        self.outputs.entry(key).or_default().push_back(Ok(output));
        self
    }

    /// Enqueues an adapter failure for the given argv.
    pub fn enqueue_error(
        &mut self,
        program: impl Into<PathBuf>,
        args: impl IntoIterator<Item = impl Into<OsString>>,
        error: BoundaryError,
    ) -> &mut Self {
        let key = (
            program.into(),
            args.into_iter().map(Into::into).collect::<Vec<_>>(),
        );
        self.outputs.entry(key).or_default().push_back(Err(error));
        self
    }

    /// Peeks the head of the call trace without consuming it.
    ///
    /// Returns `None` when the trace is empty. Useful for compact
    /// `assert_eq!`-style checks against the first recorded call; for full
    /// ordering assertions use [`call_trace`](Self::call_trace) instead.
    #[must_use]
    pub fn next_call(&self) -> Option<&CommandCall> {
        self.call_trace.first()
    }

    /// Returns the full ordered trace of argv signatures seen by this fake.
    ///
    /// Each entry records program, args, and timeout in invocation order.
    /// Differential tests compare these against the Python boundary oracle.
    #[must_use]
    pub fn call_trace(&self) -> &[CommandCall] {
        &self.call_trace
    }
}

impl CommandRunner for FakeCommandRunner {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        timeout: std::time::Duration,
    ) -> Result<CommandOutput, BoundaryError> {
        let key = (program.to_path_buf(), args.to_vec());
        self.call_trace.push(CommandCall {
            program: key.0.clone(),
            args: key.1.clone(),
            timeout,
        });
        match self.outputs.get_mut(&key).and_then(VecDeque::pop_front) {
            Some(output) => output,
            None => Err(BoundaryError::CommandNotQueued {
                program: key.0,
                args: key.1,
            }),
        }
    }
}

#[cfg(test)]
mod tests;
