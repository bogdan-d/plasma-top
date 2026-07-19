//! Command-runner boundary and its in-memory fake.
//!
//! [`CommandRunner`] is the trait the production adapter (`sensors/source.rs`,
//! Wave 5 COLLECTOR lane) and the in-memory fake both implement. Sensor/daemon
//! code accepts `impl CommandRunner` (or `&mut dyn CommandRunner`) so tests
//! can inject [`FakeCommandRunner`] with argv-keyed replies and an ordered
//! call trace, with no child process ever spawned.

use std::collections::{HashMap, VecDeque};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::domain::boundary::{BoundaryError, CommandOutput, CommandRunner};

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
///     .run(Path::new("/usr/bin/ip"), &[OsString::from("-j"), OsString::from("route")])
///     .expect("enqueued reply must be returned");
/// assert_eq!(out.stdout, b"[]");
/// ```
#[derive(Debug, Default)]
pub struct FakeCommandRunner {
    /// Argv-keyed FIFO of pending replies.
    outputs: HashMap<(PathBuf, Vec<OsString>), VecDeque<CommandOutput>>,
    /// Ordered signatures of every invocation seen by this fake.
    call_trace: Vec<(PathBuf, Vec<OsString>)>,
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
        self.outputs.entry(key).or_default().push_back(output);
        self
    }

    /// Peeks the head of the call trace without consuming it.
    ///
    /// Returns `None` when the trace is empty. Useful for compact
    /// `assert_eq!`-style checks against the first recorded call; for full
    /// ordering assertions use [`call_trace`](Self::call_trace) instead.
    #[must_use]
    pub fn next_call(&self) -> Option<&(PathBuf, Vec<OsString>)> {
        self.call_trace.first()
    }

    /// Returns the full ordered trace of argv signatures seen by this fake.
    ///
    /// Each entry is `(program, args)` in invocation order. Differential tests
    /// compare this against the Python capability/argv oracle to assert the
    /// Rust adapter issues exactly the expected commands.
    #[must_use]
    pub fn call_trace(&self) -> &[(PathBuf, Vec<OsString>)] {
        &self.call_trace
    }
}

impl CommandRunner for FakeCommandRunner {
    fn run(&mut self, program: &Path, args: &[OsString]) -> Result<CommandOutput, BoundaryError> {
        let key = (program.to_path_buf(), args.to_vec());
        self.call_trace.push(key.clone());
        match self.outputs.get_mut(&key).and_then(VecDeque::pop_front) {
            Some(output) => Ok(output),
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

        let got = match runner.run(Path::new("/bin/true"), &[]) {
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
        ) {
            Ok(out) => out,
            Err(error) => panic!("first reply: {error}"),
        };
        let second = match runner.run(
            Path::new("/bin/ping"),
            &[OsString::from("-c"), OsString::from("1")],
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
        let _ = runner.run(Path::new("/bin/true"), &[]);
        // Unmatched call (no enqueue) — still recorded in the trace.
        let _ = runner.run(Path::new("/bin/false"), &[]);

        let trace = runner.call_trace();
        assert_eq!(trace.len(), 2, "every invocation is recorded");
        assert_eq!(trace[0].0, PathBuf::from("/bin/true"));
        assert_eq!(trace[1].0, PathBuf::from("/bin/false"));
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

        let _ = runner.run(Path::new("/bin/true"), &[]);
        let Some(head) = runner.next_call() else {
            panic!("trace non-empty after one call");
        };
        assert_eq!(head.0, PathBuf::from("/bin/true"));
    }

    #[test]
    fn run_returns_command_not_queued_when_queue_empty() {
        let mut runner = FakeCommandRunner::new();

        let err = match runner.run(Path::new("/bin/missing"), &[OsString::from("--flag")]) {
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

        let first = runner.run(Path::new("/bin/once"), &[]);
        assert!(first.is_ok());

        let second = runner.run(Path::new("/bin/once"), &[]);
        assert!(matches!(
            second,
            Err(BoundaryError::CommandNotQueued { .. })
        ));
    }

    #[test]
    fn enqueue_is_chainable_via_mut_self() {
        let mut runner = FakeCommandRunner::new();
        runner
            .enqueue("/bin/a", Option::<&str>::None, ok_output("/bin/a", b"a"))
            .enqueue("/bin/b", Option::<&str>::None, ok_output("/bin/b", b"b"));

        let a = match runner.run(Path::new("/bin/a"), &[]) {
            Ok(out) => out,
            Err(error) => panic!("a: {error}"),
        };
        let b = match runner.run(Path::new("/bin/b"), &[]) {
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

        let route = match runner.run(Path::new("/bin/ip"), &[OsString::from("route")]) {
            Ok(out) => out,
            Err(error) => panic!("route: {error}"),
        };
        let addr = match runner.run(Path::new("/bin/ip"), &[OsString::from("addr")]) {
            Ok(out) => out,
            Err(error) => panic!("addr: {error}"),
        };

        assert_eq!(route.stdout, b"route");
        assert_eq!(addr.stdout, b"addr");
    }
}
