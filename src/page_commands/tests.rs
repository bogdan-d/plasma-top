#![allow(clippy::expect_used)]

use super::*;
use crate::domain::boundary::{CommandOutput, CommandStatus};
use crate::test_support::FakeCommandRunner;
use std::cell::Cell;
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_dir(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "plasma-top-page-tests-{label}-{}-{unique}",
        process::id()
    ));
    fs::create_dir_all(&path).expect("create temp dir");
    path
}

fn ok_output(program: &str, stdout: &[u8]) -> CommandOutput {
    CommandOutput {
        program: PathBuf::from(program),
        args: Vec::new(),
        status: CommandStatus::Exit(0),
        stdout: stdout.to_vec(),
        stderr: Vec::new(),
    }
}

fn output(program: &str, status: CommandStatus, stdout: &[u8], stderr: &[u8]) -> CommandOutput {
    CommandOutput {
        program: PathBuf::from(program),
        args: Vec::new(),
        status,
        stdout: stdout.to_vec(),
        stderr: stderr.to_vec(),
    }
}

#[test]
fn build_pages_keeps_full_and_skips_unknown_ids() {
    let pages = build_pages(&[
        String::from("processes"),
        String::from("nope"),
        String::from("graphs"),
        String::from("processes"),
    ]);

    assert_eq!(pages[0], FULL_PAGE);
    assert_eq!(pages[1].id, "processes");
    assert_eq!(pages[2].id, "graphs");
    assert_eq!(pages[3].id, "processes");
    assert_eq!(pages.len(), 4);
}

#[test]
fn registry_matches_python_page_metadata() {
    let pages = build_pages(
        &[
            "processes",
            "connections",
            "fastfetch",
            "cpu_cores",
            "graphs",
        ]
        .map(String::from),
    );

    assert_eq!(
        pages.iter().map(|page| page.id).collect::<Vec<_>>(),
        [
            "full",
            "processes",
            "connections",
            "fastfetch",
            "cpu_cores",
            "graphs",
        ]
    );
    let connections = pages[2].command().expect("connections command");
    assert_eq!(connections.argv, &["ss", "-4tlnp"]);
    assert_eq!(connections.max_lines, 20);
    assert_eq!(connections.colorize, Some(PageColorizer::Connections));
    let fastfetch = pages[3].command().expect("fastfetch command");
    assert_eq!(fastfetch.ttl, Duration::from_secs(30));
    assert!(fastfetch.pty);
    assert_eq!(pages[1].render(), Some(PageRenderKind::TopProcess));
    assert_eq!(pages[4].render(), Some(PageRenderKind::CpuCores));
    assert_eq!(pages[5].render(), Some(PageRenderKind::Graphs));
}

#[test]
fn run_command_returns_not_found_without_lookup_hit() {
    let mut runner = FakeCommandRunner::new();
    let mut cache = PageCommandCache::new();

    let text = run_command(
        &CONNECTIONS_PAGE,
        &mut runner,
        &CommandLookup::new(),
        &mut cache,
        Duration::from_secs(5),
    );

    assert_eq!(text, "ss: not found");
    assert!(runner.call_trace().is_empty());
}

#[test]
fn run_command_uses_script_when_pty_and_script_available() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/script",
        [
            "-qec",
            "fastfetch --logo none --structure OS:Kernel:Loadavg:Uptime:Separator:Chassis:Board:Bios:CPU:GPU:Display:BluetoothRadio:Separator:Memory:Disk:Battery:PowerAdapter:Wifi:LocalIP:DNS:Separator:InitSystem:Shell:LM:DE:WM",
            "/dev/null",
        ],
        ok_output("/usr/bin/script", b"hello\n"),
    );
    let mut commands = CommandLookup::new();
    commands
        .insert("fastfetch", "/usr/bin/fastfetch")
        .insert("script", "/usr/bin/script");
    let mut cache = PageCommandCache::new();

    let text = run_command(
        &FASTFETCH_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(1),
    );

    assert_eq!(text, "hello");
    assert_eq!(runner.call_trace().len(), 1);
    assert_eq!(
        runner.call_trace()[0].program,
        PathBuf::from("/usr/bin/script")
    );
    assert_eq!(runner.call_trace()[0].timeout, COMMAND_TIMEOUT);
}

#[test]
fn run_command_falls_back_to_plain_execution_without_script() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/fastfetch",
        ["--logo", "none", "--structure", "OS:Kernel:Loadavg:Uptime:Separator:Chassis:Board:Bios:CPU:GPU:Display:BluetoothRadio:Separator:Memory:Disk:Battery:PowerAdapter:Wifi:LocalIP:DNS:Separator:InitSystem:Shell:LM:DE:WM"],
        ok_output("/usr/bin/fastfetch", b"plain\n"),
    );
    let mut commands = CommandLookup::new();
    commands.insert("fastfetch", "/usr/bin/fastfetch");
    let mut cache = PageCommandCache::new();

    let text = run_command(
        &FASTFETCH_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(1),
    );

    assert_eq!(text, "plain");
    assert_eq!(
        runner.call_trace()[0].program,
        PathBuf::from("/usr/bin/fastfetch")
    );
    assert_eq!(runner.call_trace()[0].timeout, COMMAND_TIMEOUT);
}

#[test]
fn attempt_command_runs_once_per_call_without_applying_page_ttl() {
    let spec = FASTFETCH_PAGE.command().expect("fastfetch command");
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/fastfetch",
        spec.argv[1..].iter().copied(),
        ok_output("/usr/bin/fastfetch", b"first\n"),
    );
    runner.enqueue(
        "/usr/bin/fastfetch",
        spec.argv[1..].iter().copied(),
        ok_output("/usr/bin/fastfetch", b"second\n"),
    );
    let mut commands = CommandLookup::new();
    commands.insert("fastfetch", "/usr/bin/fastfetch");

    let first = attempt_command(&FASTFETCH_PAGE, &mut runner, &commands);
    let second = attempt_command(&FASTFETCH_PAGE, &mut runner, &commands);

    assert_eq!(first, PageCommandAttempt::Completed(String::from("first")));
    assert_eq!(
        second,
        PageCommandAttempt::Completed(String::from("second"))
    );
    assert_eq!(runner.call_trace().len(), 2);
}

#[test]
fn run_command_ttl_cache_skips_second_invocation() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/fastfetch",
        ["--logo", "none", "--structure", "OS:Kernel:Loadavg:Uptime:Separator:Chassis:Board:Bios:CPU:GPU:Display:BluetoothRadio:Separator:Memory:Disk:Battery:PowerAdapter:Wifi:LocalIP:DNS:Separator:InitSystem:Shell:LM:DE:WM"],
        ok_output("/usr/bin/fastfetch", b"cached\n"),
    );
    let mut commands = CommandLookup::new();
    commands.insert("fastfetch", "/usr/bin/fastfetch");
    let mut cache = PageCommandCache::new();

    let first = run_command(
        &FASTFETCH_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(5),
    );
    let second = run_command(
        &FASTFETCH_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(10),
    );

    assert_eq!(first, "cached");
    assert_eq!(second, "cached");
    assert_eq!(runner.call_trace().len(), 1);
}

#[test]
fn run_command_refreshes_at_ttl_boundary() {
    let mut runner = FakeCommandRunner::new();
    let spec = FASTFETCH_PAGE.command().expect("fastfetch command");
    runner.enqueue(
        "/usr/bin/fastfetch",
        spec.argv[1..].iter().copied(),
        ok_output("/usr/bin/fastfetch", b"first\n"),
    );
    runner.enqueue(
        "/usr/bin/fastfetch",
        spec.argv[1..].iter().copied(),
        ok_output("/usr/bin/fastfetch", b"second\n"),
    );
    let mut commands = CommandLookup::new();
    commands.insert("fastfetch", "/usr/bin/fastfetch");
    let mut cache = PageCommandCache::new();

    let first = run_command(
        &FASTFETCH_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::ZERO,
    );
    let second = run_command(
        &FASTFETCH_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(30),
    );

    assert_eq!((first.as_str(), second.as_str()), ("first", "second"));
    assert_eq!(runner.call_trace().len(), 2);
}

struct AdvancingCommandRunner<'a> {
    now: &'a Cell<Duration>,
}

impl CommandRunner for AdvancingCommandRunner<'_> {
    fn run(
        &mut self,
        program: &Path,
        _args: &[OsString],
        _timeout: Duration,
    ) -> Result<CommandOutput, BoundaryError> {
        self.now.set(self.now.get() + Duration::from_secs(7));
        Ok(ok_output(&program.to_string_lossy(), b"completed\n"))
    }
}

#[test]
fn command_capture_time_is_sampled_after_slow_execution() {
    let now = Cell::new(Duration::from_secs(2));
    let mut runner = AdvancingCommandRunner { now: &now };
    let mut commands = CommandLookup::new();
    commands.insert("ss", "/usr/bin/ss");
    let mut cache = PageCommandCache::new();

    let text = run_command_with_clock(
        &CONNECTIONS_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(2),
        &mut || now.get(),
    );

    assert_eq!(text, "completed");
    let state = cache.state("connections").expect("owner state");
    assert_eq!(state.attempted_at, Some(Duration::from_secs(9)));
    assert_eq!(
        state.latest.as_ref().map(|sample| sample.captured_at),
        Some(Duration::from_secs(9))
    );
}

#[test]
fn run_command_retains_success_when_due_refresh_fails() {
    let spec = FASTFETCH_PAGE.command().expect("fastfetch command");
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/fastfetch",
        spec.argv[1..].iter().copied(),
        ok_output("/usr/bin/fastfetch", b"retained\n"),
    );
    runner.enqueue_error(
        "/usr/bin/fastfetch",
        spec.argv[1..].iter().copied(),
        BoundaryError::CommandFailed {
            program: PathBuf::from("/usr/bin/fastfetch"),
            args: spec.argv[1..].iter().map(OsString::from).collect(),
            detail: String::from("timed out"),
        },
    );
    let mut commands = CommandLookup::new();
    commands.insert("fastfetch", "/usr/bin/fastfetch");
    let mut cache = PageCommandCache::new();

    assert_eq!(
        run_command(
            &FASTFETCH_PAGE,
            &mut runner,
            &commands,
            &mut cache,
            Duration::from_secs(2),
        ),
        "retained"
    );
    assert_eq!(
        run_command(
            &FASTFETCH_PAGE,
            &mut runner,
            &commands,
            &mut cache,
            Duration::from_secs(32),
        ),
        "retained"
    );

    let state = cache.state("fastfetch").expect("owner state");
    assert_eq!(state.status, PageCommandStatus::Failed);
    assert_eq!(
        state.latest.as_ref().expect("success").captured_at,
        Duration::from_secs(2)
    );
    assert_eq!(state.attempted_at, Some(Duration::from_secs(32)));
    assert_eq!(state.failed_at, Some(Duration::from_secs(32)));
    assert_eq!(state.last_failure.as_deref(), Some("fastfetch: timed out"));
    assert_eq!(runner.call_trace().len(), 2);
}

#[test]
fn run_command_retains_success_when_due_refresh_exits_nonzero() {
    let spec = FASTFETCH_PAGE.command().expect("fastfetch command");
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/fastfetch",
        spec.argv[1..].iter().copied(),
        ok_output("/usr/bin/fastfetch", b"retained\n"),
    );
    runner.enqueue(
        "/usr/bin/fastfetch",
        spec.argv[1..].iter().copied(),
        output(
            "/usr/bin/fastfetch",
            CommandStatus::Exit(7),
            b"invalid stdout\n",
            b"invalid stderr\n",
        ),
    );
    let mut commands = CommandLookup::new();
    commands.insert("fastfetch", "/usr/bin/fastfetch");
    let mut cache = PageCommandCache::new();

    assert_eq!(
        run_command(
            &FASTFETCH_PAGE,
            &mut runner,
            &commands,
            &mut cache,
            Duration::from_secs(2),
        ),
        "retained"
    );
    assert_eq!(
        run_command(
            &FASTFETCH_PAGE,
            &mut runner,
            &commands,
            &mut cache,
            Duration::from_secs(32),
        ),
        "retained"
    );

    let state = cache.state("fastfetch").expect("owner state");
    let latest = state.latest.as_ref().expect("retained success");
    assert_eq!(latest.value, "retained");
    assert_eq!(latest.captured_at, Duration::from_secs(2));
    assert_eq!(state.attempted_at, Some(Duration::from_secs(32)));
    assert_eq!(state.failed_at, Some(Duration::from_secs(32)));
    assert_eq!(
        state.last_failure.as_deref(),
        Some("fastfetch: exited with status 7")
    );
    assert_eq!(state.status, PageCommandStatus::Failed);
}

#[test]
fn zero_ttl_page_retains_success_across_failed_real_attempt() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/ss",
        ["-4tlnp"],
        ok_output("/usr/bin/ss", b"first\n"),
    );
    runner.enqueue_error(
        "/usr/bin/ss",
        ["-4tlnp"],
        BoundaryError::CommandFailed {
            program: PathBuf::from("/usr/bin/ss"),
            args: vec![OsString::from("-4tlnp")],
            detail: String::from("transport error"),
        },
    );
    let mut commands = CommandLookup::new();
    commands.insert("ss", "/usr/bin/ss");
    let mut cache = PageCommandCache::new();

    let first = run_command(
        &CONNECTIONS_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(4),
    );
    let retained = run_command(
        &CONNECTIONS_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(5),
    );

    assert_eq!((first.as_str(), retained.as_str()), ("first", "first"));
    assert_eq!(
        cache
            .state("connections")
            .and_then(|state| state.latest.as_ref())
            .map(|sample| sample.captured_at),
        Some(Duration::from_secs(4))
    );
    assert_eq!(runner.call_trace().len(), 2);
}

#[test]
fn zero_ttl_page_retains_success_when_refresh_is_signaled() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/ss",
        ["-4tlnp"],
        ok_output("/usr/bin/ss", b"retained\n"),
    );
    runner.enqueue(
        "/usr/bin/ss",
        ["-4tlnp"],
        output(
            "/usr/bin/ss",
            CommandStatus::Signal(9),
            b"invalid stdout\n",
            b"invalid stderr\n",
        ),
    );
    let mut commands = CommandLookup::new();
    commands.insert("ss", "/usr/bin/ss");
    let mut cache = PageCommandCache::new();

    assert_eq!(
        run_command(
            &CONNECTIONS_PAGE,
            &mut runner,
            &commands,
            &mut cache,
            Duration::from_secs(4),
        ),
        "retained"
    );
    assert_eq!(
        run_command(
            &CONNECTIONS_PAGE,
            &mut runner,
            &commands,
            &mut cache,
            Duration::from_secs(5),
        ),
        "retained"
    );

    let state = cache.state("connections").expect("owner state");
    let latest = state.latest.as_ref().expect("retained success");
    assert_eq!(latest.value, "retained");
    assert_eq!(latest.captured_at, Duration::from_secs(4));
    assert_eq!(state.attempted_at, Some(Duration::from_secs(5)));
    assert_eq!(state.failed_at, Some(Duration::from_secs(5)));
    assert_eq!(
        state.last_failure.as_deref(),
        Some("ss: terminated by signal 9")
    );
    assert_eq!(state.status, PageCommandStatus::Failed);
}

#[test]
fn ttl_free_attempt_records_success_and_failure_timestamps() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/ss",
        ["-4tlnp"],
        ok_output("/usr/bin/ss", b"captured\n"),
    );
    runner.enqueue_error(
        "/usr/bin/ss",
        ["-4tlnp"],
        BoundaryError::CommandFailed {
            program: PathBuf::from("/usr/bin/ss"),
            args: vec![OsString::from("-4tlnp")],
            detail: String::from("failed"),
        },
    );
    let mut commands = CommandLookup::new();
    commands.insert("ss", "/usr/bin/ss");
    let mut cache = PageCommandCache::new();

    let captured = attempt_command_with_state(
        &CONNECTIONS_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(7),
    );
    let failed = attempt_command_with_state(
        &CONNECTIONS_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(9),
    );

    assert!(matches!(captured, PageCommandAttempt::Completed(_)));
    assert!(matches!(failed, PageCommandAttempt::Unavailable(_)));
    let state = cache.state("connections").expect("owner state");
    assert_eq!(
        state.latest.as_ref().expect("success").captured_at,
        Duration::from_secs(7)
    );
    assert_eq!(state.attempted_at, Some(Duration::from_secs(9)));
    assert_eq!(state.failed_at, Some(Duration::from_secs(9)));
    assert_eq!(state.status, PageCommandStatus::Failed);
    assert_eq!(runner.call_trace().len(), 2);
}

#[test]
fn run_command_surfaces_adapter_failure_with_page_executable() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue_error(
        "/usr/bin/ss",
        ["-4tlnp"],
        BoundaryError::CommandFailed {
            program: PathBuf::from("/usr/bin/ss"),
            args: vec![OsString::from("-4tlnp")],
            detail: String::from("timed out"),
        },
    );
    let mut commands = CommandLookup::new();
    commands.insert("ss", "/usr/bin/ss");

    let text = run_command(
        &CONNECTIONS_PAGE,
        &mut runner,
        &commands,
        &mut PageCommandCache::new(),
        Duration::ZERO,
    );

    assert_eq!(text, "ss: timed out");
    assert_eq!(runner.call_trace()[0].timeout, Duration::from_secs(5));
}

#[test]
fn first_nonzero_attempt_is_unavailable_and_does_not_capture_output() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/ss",
        ["-4tlnp"],
        output(
            "/usr/bin/ss",
            CommandStatus::Exit(1),
            b"invalid stdout\n",
            b"invalid stderr\n",
        ),
    );
    let mut commands = CommandLookup::new();
    commands.insert("ss", "/usr/bin/ss");
    let mut cache = PageCommandCache::new();

    let text = run_command(
        &CONNECTIONS_PAGE,
        &mut runner,
        &commands,
        &mut cache,
        Duration::from_secs(3),
    );

    assert_eq!(text, "ss: exited with status 1");
    let state = cache.state("connections").expect("owner state");
    assert!(state.latest.is_none());
    assert_eq!(state.attempted_at, Some(Duration::from_secs(3)));
    assert_eq!(state.failed_at, Some(Duration::from_secs(3)));
    assert_eq!(
        state.last_failure.as_deref(),
        Some("ss: exited with status 1")
    );
    assert_eq!(state.status, PageCommandStatus::Failed);
}

#[test]
fn run_command_reports_empty_output_and_truncates_visible_lines() {
    let page = Page {
        id: "limited",
        label: "Limited",
        source: PageSource::Command(PageCommandSpec {
            argv: &["limited"],
            ttl: Duration::ZERO,
            max_lines: 2,
            pty: false,
            colorize: None,
        }),
        click: CLICK_SYSTEM_MONITOR,
    };
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/limited",
        Option::<&str>::None,
        ok_output("/usr/bin/limited", b"a\nb\nc\n"),
    );
    runner.enqueue(
        "/usr/bin/limited",
        Option::<&str>::None,
        ok_output("/usr/bin/limited", b"\n"),
    );
    let mut commands = CommandLookup::new();
    commands.insert("limited", "/usr/bin/limited");
    let mut cache = PageCommandCache::new();

    let limited = run_command(&page, &mut runner, &commands, &mut cache, Duration::ZERO);
    let empty = run_command(&page, &mut runner, &commands, &mut cache, Duration::ZERO);

    assert_eq!(limited, "a\nb");
    assert_eq!(empty, "limited: no output");
}

#[test]
fn text_to_mono_html_escapes_html_and_preserves_spaces() {
    assert_eq!(text_to_mono_html("a < b\n"), "a&nbsp;&lt;&nbsp;b<br>&nbsp;");
    assert_eq!(ellipsize("abcd", 0), "a…");
    assert_eq!(ellipsize("abcd", 3), "ab…");
}

#[test]
fn text_width_ignores_sgr_sequences() {
    assert_eq!(text_width("\u{1b}[31mred\u{1b}[0m\nwide"), 4);
}

#[test]
fn format_connections_resolves_interpreter_cmdline_and_services() {
    let proc_root = temp_dir("proc");
    let pid_dir = proc_root.join("1234");
    fs::create_dir_all(&pid_dir).expect("pid dir");
    fs::write(
        pid_dir.join("cmdline"),
        b"python3\0/home/user/app.py\0--flag\0",
    )
    .expect("cmdline");
    let env = PageEnvironment {
        proc_root,
        services_text: Some(String::from("http-alt 8080/tcp\n")),
    };
    let text = concat!(
        "State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n",
        "LISTEN 0 128 127.0.0.1:8080 0.0.0.0:* users:((\"python3\",pid=1234,fd=5))\n",
        "LISTEN 0 128 0.0.0.0:5432 0.0.0.0:* -\n",
        "LISTEN 0 128 127.0.0.1:8080 0.0.0.0:* -\n"
    );

    let (html, width) = format_connections(text, 24, &env);

    assert_eq!(
        (html.as_str(), width),
        (
            r#"<span class="active">app</span>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;127.0.0.1:8080&nbsp;<br><span class="warn">postgres</span>&nbsp;&nbsp;&nbsp;&nbsp;<span class="warn">0.0.0.0:5432&nbsp;</span><br><span class="label">http-alt</span>&nbsp;&nbsp;127.0.0.1:8080&nbsp;"#,
            25,
        )
    );
}

#[test]
fn connection_helpers_degrade_for_missing_process_and_unknown_service() {
    let env = PageEnvironment {
        proc_root: temp_dir("missing-proc"),
        services_text: Some(String::new()),
    };

    assert_eq!(proc_name("python3", 9999, &env.proc_root), "python3");
    assert_eq!(service_for_port("127.0.0.1:49152", &env), None);
    assert_eq!(
        format_connections("malformed\n", 30, &env),
        (text_to_mono_html("no listening sockets"), 30)
    );
}

#[test]
fn page_inner_wraps_connections_page_with_pager() {
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        "/usr/bin/ss",
        ["-4tlnp"],
        ok_output(
            "/usr/bin/ss",
            b"State Recv-Q Send-Q Local Address:Port Peer Address:Port Process\n",
        ),
    );
    let mut commands = CommandLookup::new();
    commands.insert("ss", "/usr/bin/ss");
    let mut cache = PageCommandCache::new();

    let html = page_inner(
        &CONNECTIONS_PAGE,
        1,
        3,
        20,
        &mut runner,
        PageCommandContext {
            commands: &commands,
            cache: &mut cache,
            now: Duration::ZERO,
            environment: &PageEnvironment::default(),
        },
    );

    assert!(html.starts_with(r#"<div class="page">"#));
    assert!(html.contains(r#"<div class="pager">"#));
}

#[test]
fn page_inner_fastfetch_matches_python_text_shell() {
    let mut runner = FakeCommandRunner::new();
    let spec = FASTFETCH_PAGE.command().expect("fastfetch command");
    runner.enqueue(
        "/usr/bin/fastfetch",
        spec.argv[1..].iter().copied(),
        ok_output("/usr/bin/fastfetch", b"OS:  Arch\nKernel: Linux\n"),
    );
    let mut commands = CommandLookup::new();
    commands.insert("fastfetch", "/usr/bin/fastfetch");
    let mut cache = PageCommandCache::new();

    let html = page_inner(
        &FASTFETCH_PAGE,
        2,
        4,
        30,
        &mut runner,
        PageCommandContext {
            commands: &commands,
            cache: &mut cache,
            now: Duration::ZERO,
            environment: &PageEnvironment::default(),
        },
    );

    assert_eq!(
        html,
        r#"<div class="page">OS:&nbsp;&nbsp;Arch<br>Kernel:&nbsp;Linux</div><div class="pager">&nbsp;&nbsp;&nbsp;<span class="off">●</span>&nbsp;<span class="off">●</span>&nbsp;<span class="on">●</span>&nbsp;<span class="off">●</span></div>"#
    );
}

#[test]
fn title_and_pager_match_python_shell() {
    assert_eq!(default_click(), &["plasma-systemmonitor"]);
    assert_eq!(top_process_page_rows(), 15);
    assert_eq!(
        title_html(&FASTFETCH_PAGE),
        r#"<div><span class="title">SYSTEM INFO</span></div><div width="100%" class="title-rule">&nbsp;</div>"#
    );
    assert_eq!(pager_html(0, 5, 1), "");
    assert_eq!(
        pager_html(1, 11, 3),
        r#"<div class="pager">&nbsp;&nbsp;&nbsp;<span class="off">●</span>&nbsp;<span class="on">●</span>&nbsp;<span class="off">●</span></div>"#
    );
}
