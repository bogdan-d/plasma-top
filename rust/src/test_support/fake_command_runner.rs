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
/// use pirostats::domain::boundary::{CommandOutput, CommandRunner, CommandStatus};
/// use pirostats::test_support::FakeCommandRunner;
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
mod tests {
    use super::*;
    use crate::domain::boundary::CommandStatus;
    use std::time::Duration;

    fn ok_output(program: &str, payload: &[u8]) -> CommandOutput {
        CommandOutput {
            program: PathBuf::from(program),
            args: Vec::new(),
            status: CommandStatus::Exit(0),
            stdout: payload.to_vec(),
            stderr: Vec::new(),
        }
    }

    #[test]
    fn new_starts_empty() {
        let runner = FakeCommandRunner::new();

        assert!(runner.call_trace().is_empty());
        assert!(runner.next_call().is_none());
    }

    #[test]
    fn enqueue_then_run_returns_reply_in_order() {
        let mut runner = FakeCommandRunner::new();
        let output = ok_output("/bin/true", b"hello");
        runner.enqueue("/bin/true", Option::<&str>::None, output.clone());

        let got = match runner.run(Path::new("/bin/true"), &[], Duration::ZERO) {
            Ok(out) => out,
            Err(error) => panic!("enqueued reply must be returned: {error}"),
        };

        assert_eq!(got, output);
    }

    #[test]
    fn repeated_calls_for_same_argv_return_replies_in_fifo_order() {
        let mut runner = FakeCommandRunner::new();
        runner.enqueue("/bin/ping", ["-c", "1"], ok_output("/bin/ping", b"one"));
        runner.enqueue("/bin/ping", ["-c", "1"], ok_output("/bin/ping", b"two"));

        let first = match runner.run(
            Path::new("/bin/ping"),
            &[OsString::from("-c"), OsString::from("1")],
            Duration::from_secs(1),
        ) {
            Ok(out) => out,
            Err(error) => panic!("first reply: {error}"),
        };
        let second = match runner.run(
            Path::new("/bin/ping"),
            &[OsString::from("-c"), OsString::from("1")],
            Duration::from_secs(1),
        ) {
            Ok(out) => out,
            Err(error) => panic!("second reply: {error}"),
        };

        assert_eq!(first.stdout, b"one");
        assert_eq!(second.stdout, b"two");
    }

    #[test]
    fn run_records_call_in_trace_regardless_of_match() {
        let mut runner = FakeCommandRunner::new();
        runner.enqueue(
            "/bin/true",
            Option::<&str>::None,
            ok_output("/bin/true", b""),
        );

        // Matched call.
        let _ = runner.run(Path::new("/bin/true"), &[], Duration::from_secs(1));
        // Unmatched call (no enqueue) — still recorded in the trace.
        let _ = runner.run(Path::new("/bin/false"), &[], Duration::from_secs(2));

        let trace = runner.call_trace();
        assert_eq!(trace.len(), 2, "every invocation is recorded");
        assert_eq!(trace[0].program, PathBuf::from("/bin/true"));
        assert_eq!(trace[0].timeout, Duration::from_secs(1));
        assert_eq!(trace[1].program, PathBuf::from("/bin/false"));
        assert_eq!(trace[1].timeout, Duration::from_secs(2));
    }

    #[test]
    fn next_call_peeks_head_of_trace() {
        let mut runner = FakeCommandRunner::new();
        runner.enqueue(
            "/bin/true",
            Option::<&str>::None,
            ok_output("/bin/true", b""),
        );

        assert!(runner.next_call().is_none(), "trace empty before any call");

        let _ = runner.run(Path::new("/bin/true"), &[], Duration::ZERO);
        let Some(head) = runner.next_call() else {
            panic!("trace non-empty after one call");
        };
        assert_eq!(head.program, PathBuf::from("/bin/true"));
    }

    #[test]
    fn run_returns_command_not_queued_when_queue_empty() {
        let mut runner = FakeCommandRunner::new();

        let err = match runner.run(
            Path::new("/bin/missing"),
            &[OsString::from("--flag")],
            Duration::ZERO,
        ) {
            Ok(out) => panic!("expected error, got {out:?}"),
            Err(error) => error,
        };

        match err {
            BoundaryError::CommandNotQueued { program, args } => {
                assert_eq!(program, PathBuf::from("/bin/missing"));
                assert_eq!(args, vec![OsString::from("--flag")]);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn run_returns_command_not_queued_when_queue_for_argv_exhausted() {
        let mut runner = FakeCommandRunner::new();
        runner.enqueue(
            "/bin/once",
            Option::<&str>::None,
            ok_output("/bin/once", b""),
        );

        let first = runner.run(Path::new("/bin/once"), &[], Duration::ZERO);
        assert!(first.is_ok());

        let second = runner.run(Path::new("/bin/once"), &[], Duration::ZERO);
        assert!(matches!(
            second,
            Err(BoundaryError::CommandNotQueued { .. })
        ));
    }

    #[test]
    fn enqueue_error_returns_requested_adapter_failure() {
        let mut runner = FakeCommandRunner::new();
        let error = BoundaryError::CommandFailed {
            program: PathBuf::from("/bin/slow"),
            args: vec![OsString::from("--wait")],
            detail: String::from("timed out"),
        };
        runner.enqueue_error("/bin/slow", ["--wait"], error.clone());

        let result = runner.run(
            Path::new("/bin/slow"),
            &[OsString::from("--wait")],
            Duration::from_secs(5),
        );

        assert_eq!(result, Err(error));
        assert_eq!(runner.call_trace()[0].timeout, Duration::from_secs(5));
    }

    #[test]
    fn enqueue_is_chainable_via_mut_self() {
        let mut runner = FakeCommandRunner::new();
        runner
            .enqueue("/bin/a", Option::<&str>::None, ok_output("/bin/a", b"a"))
            .enqueue("/bin/b", Option::<&str>::None, ok_output("/bin/b", b"b"));

        let a = match runner.run(Path::new("/bin/a"), &[], Duration::ZERO) {
            Ok(out) => out,
            Err(error) => panic!("a: {error}"),
        };
        let b = match runner.run(Path::new("/bin/b"), &[], Duration::ZERO) {
            Ok(out) => out,
            Err(error) => panic!("b: {error}"),
        };
        assert_eq!(a.stdout, b"a");
        assert_eq!(b.stdout, b"b");
    }

    #[test]
    fn distinct_argv_do_not_consume_each_other_queues() {
        let mut runner = FakeCommandRunner::new();
        runner.enqueue("/bin/ip", ["route"], ok_output("/bin/ip", b"route"));
        runner.enqueue("/bin/ip", ["addr"], ok_output("/bin/ip", b"addr"));

        let route = match runner.run(
            Path::new("/bin/ip"),
            &[OsString::from("route")],
            Duration::ZERO,
        ) {
            Ok(out) => out,
            Err(error) => panic!("route: {error}"),
        };
        let addr = match runner.run(
            Path::new("/bin/ip"),
            &[OsString::from("addr")],
            Duration::ZERO,
        ) {
            Ok(out) => out,
            Err(error) => panic!("addr: {error}"),
        };

        assert_eq!(route.stdout, b"route");
        assert_eq!(addr.stdout, b"addr");
    }
}
