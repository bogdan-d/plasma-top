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
