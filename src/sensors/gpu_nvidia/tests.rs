use super::*;

use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::domain::boundary::{BoundaryError, CommandOutput};
use crate::test_support::FakeCommandRunner;

#[derive(Default)]
struct FakeNvml {
    replies: VecDeque<Result<NvmlMetrics, NvmlError>>,
    calls: usize,
}

impl FakeNvml {
    fn with(replies: impl IntoIterator<Item = Result<NvidiaMetrics, NvmlError>>) -> Self {
        Self {
            replies: replies
                .into_iter()
                .map(|reply| reply.map(Into::into))
                .collect(),
            calls: 0,
        }
    }
}

impl NvmlFacade for FakeNvml {
    fn read_device_zero(&mut self) -> Result<NvmlMetrics, NvmlError> {
        self.calls += 1;
        self.replies.pop_front().unwrap_or(Err(NvmlError::Read))
    }
}

struct TempTree(PathBuf);

impl TempTree {
    fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "plasma-top-gpu-nvidia-{}-{unique}",
            std::process::id()
        ));
        if let Err(error) = fs::create_dir_all(&root) {
            panic!("failed to create {}: {error}", root.display());
        }
        Self(root)
    }

    fn write(&self, relative: &str, value: &str) {
        let path = self.0.join(relative);
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                panic!("failed to create {}: {error}", parent.display());
            }
        }
        if let Err(error) = fs::write(&path, value) {
            panic!("failed to write {}: {error}", path.display());
        }
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn clock(seconds: u64) -> ClockSnapshot {
    ClockSnapshot {
        monotonic: Duration::from_secs(seconds),
        wall: UNIX_EPOCH,
    }
}

fn metrics(values: [Option<i32>; 5]) -> NvidiaMetrics {
    NvidiaMetrics {
        temp_celsius: values[0],
        usage_percent: values[1],
        memory_percent: values[2],
        decoder_percent: values[3],
        fan_percent: values[4],
    }
}

fn smi_output(status: CommandStatus, stdout: &[u8]) -> CommandOutput {
    CommandOutput {
        program: PathBuf::from(NVIDIA_SMI_PROGRAM),
        args: vec![
            OsString::from(NVIDIA_SMI_QUERY),
            OsString::from(NVIDIA_SMI_FORMAT),
        ],
        status,
        stdout: stdout.to_vec(),
        stderr: Vec::new(),
        truncation: Default::default(),
    }
}

fn enqueue_smi(runner: &mut FakeCommandRunner, status: CommandStatus, stdout: &[u8]) {
    runner.enqueue(
        NVIDIA_SMI_PROGRAM,
        [NVIDIA_SMI_QUERY, NVIDIA_SMI_FORMAT],
        smi_output(status, stdout),
    );
}

#[test]
fn detects_only_nvidia_display_class_devices() {
    let tree = TempTree::new();
    tree.write("bus/pci/devices/0000:01:00.0/vendor", "0x10de\n");
    tree.write("bus/pci/devices/0000:01:00.0/class", "0x030000\n");
    tree.write("bus/pci/devices/0000:02:00.0/vendor", "0x10de\n");
    tree.write("bus/pci/devices/0000:02:00.0/class", "malformed\n");

    assert!(detect_nvidia(&tree.0));
    assert!(!detect_nvidia(&tree.0.join("missing")));
}

#[test]
fn incomplete_pci_enumeration_does_not_confirm_nvidia_absence() {
    let tree = TempTree::new();
    tree.write("bus/pci/devices/0000:01:00.0/vendor", "malformed\n");

    assert!(detect_nvidia_outcome(&tree.0).is_err());
}

#[test]
fn caps_at_99_and_preserves_none_and_negative_values() {
    assert_eq!(nvidia_cap(Some(100)), Some(99));
    assert_eq!(nvidia_cap(Some(42)), Some(42));
    assert_eq!(nvidia_cap(Some(-1)), Some(-1));
    assert_eq!(nvidia_cap(None), None);
}

#[test]
fn nvml_success_clamps_and_runs_every_poll_without_smi() {
    let expected = metrics([Some(99), Some(80), Some(70), None, Some(30)]);
    let mut nvml = FakeNvml::with([
        Ok(metrics([Some(100), Some(80), Some(70), None, Some(30)])),
        Ok(expected),
    ]);
    let mut runner = FakeCommandRunner::new();
    let mut state = GpuCache::default();

    assert_eq!(
        read_nvidia(&mut state, Some(&mut nvml), &mut runner, clock(10)),
        expected
    );
    assert_eq!(
        read_nvidia(&mut state, Some(&mut nvml), &mut runner, clock(10)),
        expected
    );
    assert_eq!(nvml.calls, 2);
    assert!(runner.call_trace().is_empty());
}

#[test]
fn nvml_init_failure_selects_smi_until_state_invalidation() {
    let mut nvml = FakeNvml::with([Err(NvmlError::Init)]);
    let mut runner = FakeCommandRunner::new();
    enqueue_smi(&mut runner, CommandStatus::Exit(0), b"65, 70, 30, 40, 5\n");
    enqueue_smi(&mut runner, CommandStatus::Exit(0), b"65, 70, 30, 40, 5\n");
    let mut state = GpuCache::default();
    let expected = metrics([Some(65), Some(70), Some(30), Some(5), Some(40)]);

    assert_eq!(
        read_nvidia(&mut state, Some(&mut nvml), &mut runner, clock(10)),
        expected
    );
    assert!(state.nvml_init_failed);
    assert_eq!(
        read_nvidia(&mut state, Some(&mut nvml), &mut runner, clock(12)),
        expected
    );
    assert_eq!(nvml.calls, 1);
    assert_eq!(runner.call_trace().len(), 2);
    assert_eq!(runner.call_trace()[0].timeout, NVIDIA_SMI_TIMEOUT);
}

#[test]
fn nvml_read_failure_falls_back_but_retries_nvml_next_poll() {
    let recovered = metrics([Some(50), Some(20), Some(10), Some(3), None]);
    let mut nvml = FakeNvml::with([Err(NvmlError::Read), Ok(recovered)]);
    let mut runner = FakeCommandRunner::new();
    enqueue_smi(
        &mut runner,
        CommandStatus::Exit(0),
        b"60, 40, 20, N/A, N/A\n",
    );
    let mut state = GpuCache::default();

    assert_eq!(
        read_nvidia(&mut state, Some(&mut nvml), &mut runner, clock(10)),
        metrics([Some(60), Some(40), Some(20), None, None])
    );
    assert_eq!(
        read_nvidia(&mut state, Some(&mut nvml), &mut runner, clock(11)),
        recovered
    );
    assert_eq!(nvml.calls, 2);
    assert_eq!(runner.call_trace().len(), 1);
}

#[test]
fn smi_failure_and_malformed_results_degrade_to_absent_metrics() {
    let cases = [
        smi_output(CommandStatus::Exit(1), b"65, 70, 30, 40, 5\n"),
        smi_output(CommandStatus::Signal(9), b""),
        smi_output(CommandStatus::Exit(0), b"too,short"),
        smi_output(CommandStatus::Exit(0), &[0xff, 0xfe]),
    ];
    for output in cases {
        let mut runner = FakeCommandRunner::new();
        runner.enqueue(
            NVIDIA_SMI_PROGRAM,
            [NVIDIA_SMI_QUERY, NVIDIA_SMI_FORMAT],
            output,
        );
        assert_eq!(read_nvidia_smi(&mut runner), NvidiaMetrics::default());
    }

    let mut runner = FakeCommandRunner::new();
    runner.enqueue_error(
        NVIDIA_SMI_PROGRAM,
        [NVIDIA_SMI_QUERY, NVIDIA_SMI_FORMAT],
        BoundaryError::CommandFailed {
            program: PathBuf::from(NVIDIA_SMI_PROGRAM),
            args: Vec::new(),
            detail: String::from("timeout"),
        },
    );
    assert_eq!(read_nvidia_smi(&mut runner), NvidiaMetrics::default());
}

#[test]
fn cached_nvidia_result_keeps_latest_failure_timestamp() {
    let state = NvidiaState {
        cache: GpuCache {
            temp_celsius: Some(60),
            sampled_at: Some(Duration::from_secs(1)),
            attempted_at: Some(Duration::from_secs(2)),
            failed_at: Some(Duration::from_secs(2)),
            ..GpuCache::default()
        },
    };

    let result = crate::sensors::nvidia_result(&state, crate::sensors::AttemptStatus::Cached);

    assert_eq!(result.reading.status, crate::sensors::AttemptStatus::Cached);
    assert_eq!(result.reading.failed_at, Some(Duration::from_secs(2)));
    assert_eq!(
        result.reading.sample.map(|sample| sample.captured_at),
        Some(Duration::from_secs(1))
    );
}
