//! NVIDIA GPU discovery, metrics, fallback caching, and graph history.
//!
//! NVML itself stays behind [`NvmlFacade`]. The Phase 5 collector owns loading
//! the optional library and GPU-0 handle; this module owns all observable
//! selection, fallback, clamp, cache, and history behavior.

use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::config::Config;
use crate::domain::boundary::{ClockSnapshot, CommandRunner, CommandStatus};
use crate::domain::readings::{HardwareSnapshot, ReadingsSnapshot};
use crate::domain::state::{DaemonStateSnapshot, GpuCache};

/// `nvidia-smi` executable token used by the Python backend.
pub const NVIDIA_SMI_PROGRAM: &str = "nvidia-smi";
/// Timeout for the `nvidia-smi` fallback.
pub const NVIDIA_SMI_TIMEOUT: Duration = Duration::from_secs(5);
/// Forking fallback cache TTL.
pub const GPU_CACHE_TTL: Duration = Duration::from_secs(3);
/// NVML reads are cheap enough to run every poll.
pub const GPU_CACHE_TTL_NVML: Duration = Duration::ZERO;

const NVIDIA_VENDOR: &str = "0x10de";
const DISPLAY_CLASS_PREFIX: &str = "0x03";
const NVIDIA_SMI_QUERY: &str =
    "--query-gpu=temperature.gpu,utilization.gpu,utilization.memory,fan.speed,utilization.decoder";
const NVIDIA_SMI_FORMAT: &str = "--format=csv,noheader,nounits";

/// One NVIDIA reading in formatter order: temp, usage, memory, decoder, fan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NvidiaMetrics {
    /// GPU temperature in °C.
    pub temp_celsius: Option<i32>,
    /// GPU utilization percentage.
    pub usage_percent: Option<i32>,
    /// GPU memory-controller utilization percentage.
    pub memory_percent: Option<i32>,
    /// Decoder utilization percentage.
    pub decoder_percent: Option<i32>,
    /// Fan-speed percentage.
    pub fan_percent: Option<i32>,
}

/// NVML failure class needed by the fallback/cache state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmlError {
    /// Library initialization or GPU-0 handle lookup failed permanently.
    Init,
    /// A metric read failed; retry NVML next poll.
    Read,
}

/// Narrow NVML boundary consumed by NVIDIA orchestration.
pub trait NvmlFacade {
    /// Reads GPU 0. Optional fan/decoder values remain `None` when unsupported.
    ///
    /// # Errors
    ///
    /// Returns [`NvmlError::Init`] for initialization/handle failure and
    /// [`NvmlError::Read`] when mandatory metric reads fail.
    fn read_device_zero(&mut self) -> Result<NvidiaMetrics, NvmlError>;
}

/// Detects an NVIDIA display-class PCI device below `sys_root`.
#[must_use]
pub fn detect_nvidia(sys_root: &Path) -> bool {
    let Ok(devices) = fs::read_dir(sys_root.join("bus/pci/devices")) else {
        return false;
    };
    devices.flatten().any(|entry| {
        let device = entry.path();
        let Ok(vendor) = fs::read_to_string(device.join("vendor")) else {
            return false;
        };
        if vendor.trim() != NVIDIA_VENDOR {
            return false;
        }
        fs::read_to_string(device.join("class"))
            .is_ok_and(|class| class.trim().starts_with(DISPLAY_CLASS_PREFIX))
    })
}

/// Returns the active cache TTL for the current NVML state.
#[must_use]
pub const fn gpu_cache_ttl(nvml_available: bool, nvml_init_failed: bool) -> Duration {
    if nvml_available && !nvml_init_failed {
        GPU_CACHE_TTL_NVML
    } else {
        GPU_CACHE_TTL
    }
}

/// Caps a metric at 99 while preserving absence and negative values.
#[must_use]
pub fn nvidia_cap(value: Option<i32>) -> Option<i32> {
    value.map(|value| value.min(99))
}

/// Reads NVIDIA metrics, preferring NVML and falling back to `nvidia-smi`.
///
/// NVML initialization failure is permanent and enables the three-second
/// fallback cache. Ordinary NVML read failure falls back for this call but is
/// retried next poll, matching Python's cached-handle behavior.
pub fn read_nvidia(
    state: &mut GpuCache,
    mut nvml: Option<&mut dyn NvmlFacade>,
    runner: &mut impl CommandRunner,
    clock: ClockSnapshot,
) -> NvidiaMetrics {
    let ttl = gpu_cache_ttl(nvml.is_some(), state.nvml_init_failed);
    if let Some(sampled_at) = state.sampled_at
        && clock.monotonic.saturating_sub(sampled_at) < ttl
    {
        return metrics_from_cache(state);
    }

    let nvml_result = if state.nvml_init_failed {
        None
    } else {
        nvml.as_mut().map(|facade| facade.read_device_zero())
    };
    let metrics = match nvml_result {
        Some(Ok(metrics)) => cap_metrics(metrics),
        Some(Err(NvmlError::Init)) => {
            state.nvml_init_failed = true;
            read_nvidia_smi(runner)
        }
        Some(Err(NvmlError::Read)) | None => read_nvidia_smi(runner),
    };
    store_metrics(state, metrics, clock.monotonic);
    metrics
}

/// Reads and parses the `nvidia-smi` CSV fallback.
#[must_use]
pub fn read_nvidia_smi(runner: &mut impl CommandRunner) -> NvidiaMetrics {
    let args = [
        OsString::from(NVIDIA_SMI_QUERY),
        OsString::from(NVIDIA_SMI_FORMAT),
    ];
    let Ok(output) = runner.run(Path::new(NVIDIA_SMI_PROGRAM), &args, NVIDIA_SMI_TIMEOUT) else {
        return NvidiaMetrics::default();
    };
    if output.status != CommandStatus::Exit(0) {
        return NvidiaMetrics::default();
    }
    let Ok(stdout) = std::str::from_utf8(&output.stdout) else {
        return NvidiaMetrics::default();
    };
    parse_nvidia_smi(stdout).unwrap_or_default()
}

/// Samples the preferred GPU into graphs-page history and exposes the buffers.
///
/// NVIDIA wins on hybrid machines based on hardware presence, even when its
/// current reading is absent. A missing usage sample preserves and re-exposes
/// existing history without inserting a gap.
pub fn sample_gpu_history(
    state: &mut DaemonStateSnapshot,
    cfg: &Config,
    hw: &HardwareSnapshot,
    readings: &mut ReadingsSnapshot,
    clock: ClockSnapshot,
) {
    if !cfg.pages.order.iter().any(|page| page == "graphs") {
        return;
    }
    let (usage, decoder) = if hw.has_nvidia {
        (readings.gpu_usage, readings.gpu_dec)
    } else if hw.intel_gpu_pci.is_some() {
        (readings.gpu_intel_usage, readings.gpu_intel_dec_usage)
    } else {
        return;
    };

    if let Some(usage) = usage
        && history_due(
            state.gpu_history_sample_at,
            clock.monotonic,
            cfg.display.history_interval,
        )
    {
        state.gpu_history_sample_at = Some(clock.monotonic);
        state.gpu_usage_history.push(usage);
        state.gpu_dec_history.push(decoder.unwrap_or(0));
        let max_len = cfg.pages.graph_history_length.max(0) as usize;
        trim_to_len(&mut state.gpu_usage_history, max_len);
        trim_to_len(&mut state.gpu_dec_history, max_len);
    }
    readings
        .gpu_usage_history
        .clone_from(&state.gpu_usage_history);
    readings.gpu_dec_history.clone_from(&state.gpu_dec_history);
}

fn parse_nvidia_smi(stdout: &str) -> Option<NvidiaMetrics> {
    let parts: Vec<&str> = stdout.split(',').map(str::trim).collect();
    Some(NvidiaMetrics {
        temp_celsius: parse_metric(parts.first()?),
        usage_percent: parse_metric(parts.get(1)?),
        memory_percent: parse_metric(parts.get(2)?),
        decoder_percent: parse_metric(parts.get(4)?),
        fan_percent: parse_metric(parts.get(3)?),
    })
}

fn parse_metric(value: &str) -> Option<i32> {
    nvidia_cap(value.parse::<i32>().ok())
}

fn cap_metrics(metrics: NvidiaMetrics) -> NvidiaMetrics {
    NvidiaMetrics {
        temp_celsius: nvidia_cap(metrics.temp_celsius),
        usage_percent: nvidia_cap(metrics.usage_percent),
        memory_percent: nvidia_cap(metrics.memory_percent),
        decoder_percent: nvidia_cap(metrics.decoder_percent),
        fan_percent: nvidia_cap(metrics.fan_percent),
    }
}

fn metrics_from_cache(cache: &GpuCache) -> NvidiaMetrics {
    NvidiaMetrics {
        temp_celsius: cache.temp_celsius,
        usage_percent: cache.usage_percent,
        memory_percent: cache.memory_percent,
        decoder_percent: cache.decoder_percent,
        fan_percent: cache.fan_percent,
    }
}

fn store_metrics(cache: &mut GpuCache, metrics: NvidiaMetrics, sampled_at: Duration) {
    cache.temp_celsius = metrics.temp_celsius;
    cache.usage_percent = metrics.usage_percent;
    cache.memory_percent = metrics.memory_percent;
    cache.decoder_percent = metrics.decoder_percent;
    cache.fan_percent = metrics.fan_percent;
    cache.sampled_at = Some(sampled_at);
}

fn history_due(sampled_at: Option<Duration>, now: Duration, interval_secs: f64) -> bool {
    let interval = if interval_secs.is_finite() && interval_secs > 0.0 {
        Duration::from_secs_f64(interval_secs)
    } else {
        Duration::ZERO
    };
    sampled_at.is_none_or(|previous| now.saturating_sub(previous) >= interval)
}

fn trim_to_len<T>(values: &mut Vec<T>, max_len: usize) {
    if values.len() > max_len {
        values.drain(..values.len() - max_len);
    }
}

#[cfg(all(test, feature = "test-support"))]
mod tests {
    use super::*;

    use std::collections::VecDeque;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::domain::boundary::{BoundaryError, CommandOutput};
    use crate::test_support::FakeCommandRunner;

    #[derive(Default)]
    struct FakeNvml {
        replies: VecDeque<Result<NvidiaMetrics, NvmlError>>,
        calls: usize,
    }

    impl FakeNvml {
        fn with(replies: impl IntoIterator<Item = Result<NvidiaMetrics, NvmlError>>) -> Self {
            Self {
                replies: replies.into_iter().collect(),
                calls: 0,
            }
        }
    }

    impl NvmlFacade for FakeNvml {
        fn read_device_zero(&mut self) -> Result<NvidiaMetrics, NvmlError> {
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
                "pirostats-gpu-nvidia-{}-{unique}",
                std::process::id()
            ));
            if let Err(error) = fs::create_dir_all(&root) {
                panic!("failed to create {}: {error}", root.display());
            }
            Self(root)
        }

        fn write(&self, relative: &str, value: &str) {
            let path = self.0.join(relative);
            if let Some(parent) = path.parent()
                && let Err(error) = fs::create_dir_all(parent)
            {
                panic!("failed to create {}: {error}", parent.display());
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
    fn nvml_init_failure_permanently_selects_cached_smi_fallback() {
        let mut nvml = FakeNvml::with([Err(NvmlError::Init)]);
        let mut runner = FakeCommandRunner::new();
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
        assert_eq!(runner.call_trace().len(), 1);
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
    fn smi_cache_expires_after_three_seconds() {
        let mut runner = FakeCommandRunner::new();
        enqueue_smi(&mut runner, CommandStatus::Exit(0), b"60, 40, 20, 10, 5\n");
        enqueue_smi(&mut runner, CommandStatus::Exit(0), b"61, 41, 21, 11, 6\n");
        let mut state = GpuCache::default();

        let first = read_nvidia(&mut state, None, &mut runner, clock(10));
        assert_eq!(read_nvidia(&mut state, None, &mut runner, clock(12)), first);
        assert_eq!(
            read_nvidia(&mut state, None, &mut runner, clock(13)),
            metrics([Some(61), Some(41), Some(21), Some(6), Some(11)])
        );
        assert_eq!(runner.call_trace().len(), 2);
    }

    #[test]
    fn all_absent_smi_result_is_cached() {
        let mut runner = FakeCommandRunner::new();
        enqueue_smi(&mut runner, CommandStatus::Exit(1), b"");
        let mut state = GpuCache::default();

        assert_eq!(
            read_nvidia(&mut state, None, &mut runner, clock(10)),
            NvidiaMetrics::default()
        );
        assert_eq!(
            read_nvidia(&mut state, None, &mut runner, clock(12)),
            NvidiaMetrics::default()
        );
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
    fn history_prefers_nvidia_and_uses_zero_for_missing_decoder() {
        let mut cfg = Config::default();
        cfg.pages.order = vec![String::from("graphs")];
        cfg.pages.graph_history_length = 2;
        cfg.display.history_interval = 2.0;
        let mut hw = HardwareSnapshot {
            has_nvidia: true,
            intel_gpu_pci: Some(String::from("0000:00:02.0")),
            ..HardwareSnapshot::default()
        };
        let mut state = DaemonStateSnapshot::default();
        let mut readings = ReadingsSnapshot {
            gpu_usage: Some(70),
            gpu_dec: None,
            gpu_intel_usage: Some(20),
            gpu_intel_dec_usage: Some(10),
            ..ReadingsSnapshot::default()
        };

        sample_gpu_history(&mut state, &cfg, &hw, &mut readings, clock(10));
        assert_eq!(readings.gpu_usage_history, vec![70]);
        assert_eq!(readings.gpu_dec_history, vec![0]);

        readings.gpu_usage = Some(71);
        sample_gpu_history(&mut state, &cfg, &hw, &mut readings, clock(11));
        assert_eq!(readings.gpu_usage_history, vec![70]);
        readings.gpu_usage = Some(72);
        readings.gpu_dec = Some(4);
        sample_gpu_history(&mut state, &cfg, &hw, &mut readings, clock(12));
        readings.gpu_usage = Some(73);
        sample_gpu_history(&mut state, &cfg, &hw, &mut readings, clock(14));
        assert_eq!(readings.gpu_usage_history, vec![72, 73]);
        assert_eq!(readings.gpu_dec_history, vec![4, 4]);

        hw.has_nvidia = false;
        readings.gpu_intel_usage = Some(22);
        readings.gpu_intel_dec_usage = Some(8);
        sample_gpu_history(&mut state, &cfg, &hw, &mut readings, clock(16));
        assert_eq!(readings.gpu_usage_history, vec![73, 22]);
        assert_eq!(readings.gpu_dec_history, vec![4, 8]);
    }

    #[test]
    fn history_gap_reexposes_buffer_and_disabled_page_does_nothing() {
        let mut cfg = Config::default();
        cfg.pages.order = vec![String::from("graphs")];
        let hw = HardwareSnapshot {
            has_nvidia: true,
            ..HardwareSnapshot::default()
        };
        let mut state = DaemonStateSnapshot {
            gpu_usage_history: vec![10, 20],
            gpu_dec_history: vec![1, 2],
            ..DaemonStateSnapshot::default()
        };
        let mut readings = ReadingsSnapshot::default();

        sample_gpu_history(&mut state, &cfg, &hw, &mut readings, clock(10));
        assert_eq!(readings.gpu_usage_history, vec![10, 20]);
        assert_eq!(readings.gpu_dec_history, vec![1, 2]);

        cfg.pages.order.clear();
        readings.gpu_usage_history.clear();
        readings.gpu_dec_history.clear();
        sample_gpu_history(&mut state, &cfg, &hw, &mut readings, clock(20));
        assert!(readings.gpu_usage_history.is_empty());
        assert!(readings.gpu_dec_history.is_empty());
    }
}
