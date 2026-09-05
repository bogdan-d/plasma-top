use super::*;

#[cfg(feature = "test-support")]
use std::cell::Cell;
#[cfg(feature = "test-support")]
use std::ffi::OsString;
#[cfg(feature = "test-support")]
use std::time::UNIX_EPOCH;

#[cfg(feature = "test-support")]
use crate::config::NotificationConfig;
#[cfg(feature = "test-support")]
use crate::domain::boundary::{ClockSnapshot, CommandOutput, CommandStatus};
#[cfg(feature = "test-support")]
use crate::page_commands::{CommandLookup, PageCommandStatus};
#[cfg(feature = "test-support")]
use crate::scheduler::{JobId, JobKind};
#[cfg(feature = "test-support")]
use crate::test_support::{FakeCommandRunner, FakeDbus, FixtureRoot};

#[test]
fn strip_html_preserves_rows_and_spacing() {
    assert_eq!(
        strip_html("<style>.x{}</style><div>a&nbsp;b<br>c</div>"),
        "a b\nc\n"
    );
}

#[cfg(feature = "test-support")]
struct DiagnosticRenderResult {
    html: String,
    command_cache: PageCommandCache,
    command_calls: usize,
    command_job_starts: usize,
    captures: usize,
}

#[cfg(feature = "test-support")]
fn render_command_page_after_cold_and_warm(
    page_id: &str,
    status: CommandStatus,
    stdout: &[u8],
) -> DiagnosticRenderResult {
    let fixture = FixtureRoot::from_env();
    let roots = FilesystemRoots {
        runtime_root: None,
        cache_root: None,
        config_root: None,
        proc_root: fixture.proc(),
        sys_root: fixture.sys(),
    };
    let mut cfg = Config::default();
    cfg.pages.order = vec![page_id.to_owned()];
    cfg.notifications = NotificationConfig {
        disk_usage: false,
        disk_smart: false,
        cpu_temp: false,
        gpu_nvidia_temp: false,
        gpu_amd_temp: false,
        hd_temp: false,
        battery_sys: false,
        battery_mouse: false,
        battery_kbd: false,
        server_check: false,
        load_avg: false,
    };
    let mut hw = HardwareInventory::default();
    let active = build_pages(&cfg.pages.order);
    let page = active[1];
    let Some(spec) = page.command() else {
        panic!("command-backed test page");
    };
    let program = PathBuf::from(format!("/fixture/bin/{}", spec.argv[0]));
    let args = spec.argv[1..]
        .iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
    let mut runner = FakeCommandRunner::new();
    runner.enqueue(
        &program,
        args,
        CommandOutput {
            program: program.clone(),
            args: Vec::new(),
            status,
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
            truncation: Default::default(),
        },
    );
    let mut lookup = CommandLookup::new();
    lookup.insert(spec.argv[0], &program);

    let mut cpu_owner = cpu::CpuState::default();
    let mut memory_owner = memory::MemoryState::default();
    let mut network_owner = network::NetworkState::default();
    let mut disk_owner = disk::DiskState::default();
    let mut process_owner = ProcessState::default();
    let mut intel_gpu_owner = gpu_intel::IntelGpuState::default();
    let mut power_owner = power::PowerState::default();
    let mut nvidia_owner = gpu_nvidia::NvidiaState::default();
    let mut gpu_history_owner = gpu_history::GpuHistoryState::default();
    let mut external_owner = external::ExternalState::default();
    let mut schedule = SerialSchedule::new(
        &cfg,
        &hw,
        &roots.proc_root,
        SchedulerTime::ZERO,
        diagnostic_acquisition_page(Some(page_id)),
    );
    let now = Cell::new(Duration::ZERO);
    let mut dbus = FakeDbus::new();
    let mut capture_clock = || ClockSnapshot {
        monotonic: now.get(),
        wall: UNIX_EPOCH + now.get(),
    };
    let mut command_job_starts = 0_usize;
    let mut captures = 0_usize;
    let readings = {
        let mut ctx = CollectCtx::new(&roots, &mut runner, &mut dbus, &mut capture_clock);
        let mut observe_start = |job: &JobId| {
            if job.kind == JobKind::PageCommand {
                command_job_starts = command_job_starts.saturating_add(1);
            }
        };
        capture_diagnostic_baseline_and_warm(
            || {
                captures = captures.saturating_add(1);
                schedule.sample(
                    OwnerRefs {
                        cpu: &mut cpu_owner,
                        memory: &mut memory_owner,
                        network: &mut network_owner,
                        disk: &mut disk_owner,
                        process: &mut process_owner,
                        intel_gpu: &mut intel_gpu_owner,
                        power: &mut power_owner,
                        nvidia: &mut nvidia_owner,
                        gpu_history: &mut gpu_history_owner,
                        external: &mut external_owner,
                    },
                    &mut hw,
                    &cfg,
                    &mut ctx,
                    None,
                    Some(&mut observe_start),
                )
            },
            || now.set(Duration::from_secs(1)),
        )
    };

    let mut command_cache = PageCommandCache::new();
    let html = render_page_with_clock(
        &cfg,
        &hw,
        &readings,
        "",
        &active,
        1,
        &mut runner,
        &lookup,
        &mut command_cache,
        now.get(),
        &roots.proc_root,
        &mut || now.get(),
    );
    let command_calls = runner
        .call_trace()
        .iter()
        .filter(|call| call.program == program)
        .count();
    DiagnosticRenderResult {
        html,
        command_cache,
        command_calls,
        command_job_starts,
        captures,
    }
}

#[cfg(feature = "test-support")]
#[test]
fn connections_diagnostic_runs_command_once_after_cold_and_warm_acquisition() {
    let result = render_command_page_after_cold_and_warm(
        "connections",
        CommandStatus::Exit(0),
        b"LISTEN 0 128 127.0.0.1:22 0.0.0.0:* users:((\"sshd\",pid=1,fd=3))\n",
    );

    assert_eq!(result.captures, 2);
    assert_eq!(result.command_job_starts, 0);
    assert_eq!(result.command_calls, 1);
    assert!(result.html.contains("sshd"));
    let Some(state) = result.command_cache.state("connections") else {
        panic!("connections owner state");
    };
    assert_eq!(state.status, PageCommandStatus::Captured);
    assert!(state.latest.is_some());
}

#[cfg(feature = "test-support")]
#[test]
fn fastfetch_diagnostic_runs_failed_command_once_after_cold_and_warm_acquisition() {
    let result = render_command_page_after_cold_and_warm(
        "fastfetch",
        CommandStatus::Exit(7),
        b"ignored output\n",
    );

    assert_eq!(result.captures, 2);
    assert_eq!(result.command_job_starts, 0);
    assert_eq!(result.command_calls, 1);
    assert!(
        result
            .html
            .contains("fastfetch:&nbsp;exited&nbsp;with&nbsp;status&nbsp;7")
    );
    let Some(state) = result.command_cache.state("fastfetch") else {
        panic!("fastfetch owner state");
    };
    assert_eq!(state.status, PageCommandStatus::Failed);
    assert_eq!(
        state.last_failure.as_deref(),
        Some("fastfetch: exited with status 7")
    );
}
