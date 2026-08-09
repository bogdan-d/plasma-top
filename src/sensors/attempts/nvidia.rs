use std::time::Duration;

use crate::domain::boundary::CommandRunner;
use crate::domain::readings::MetricSample;

use super::{AttemptResult, AttemptStatus};
use crate::sensors::gpu_history::DecoderOutcome;
use crate::sensors::gpu_nvidia::{self, NvidiaState, NvmlFacade};

/// Typed result produced by one NVIDIA-owner attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NvidiaResult {
    pub(crate) reading: AttemptResult<gpu_nvidia::NvidiaMetrics>,
    pub(crate) decoder: DecoderOutcome,
}

/// Performs one NVML NVIDIA source attempt with no cadence policy.
pub(crate) fn attempt_nvml_nvidia(
    state: &mut NvidiaState,
    nvml: &mut dyn NvmlFacade,
    captured_at: Duration,
) -> AttemptStatus {
    match gpu_nvidia::attempt_nvml(&mut state.cache, nvml, captured_at) {
        gpu_nvidia::NvidiaSourceOutcome::Captured => AttemptStatus::Captured,
        gpu_nvidia::NvidiaSourceOutcome::Failed => AttemptStatus::Failed,
    }
}

/// Performs one command-fallback NVIDIA source attempt with no cadence policy.
pub(crate) fn attempt_nvidia_fallback(
    state: &mut NvidiaState,
    runner: &mut dyn CommandRunner,
    captured_at: Duration,
) -> AttemptStatus {
    match gpu_nvidia::attempt_fallback(&mut state.cache, runner, captured_at) {
        gpu_nvidia::NvidiaSourceOutcome::Captured => AttemptStatus::Captured,
        gpu_nvidia::NvidiaSourceOutcome::Failed => AttemptStatus::Failed,
    }
}

pub(crate) fn nvidia_result(state: &NvidiaState, status: AttemptStatus) -> NvidiaResult {
    let latest_attempt_failed = match status {
        AttemptStatus::Failed => true,
        AttemptStatus::Captured => false,
        AttemptStatus::Cached => {
            let cache = &state.cache;
            let fallback_active = cache.nvml_init_failed
                || cache.nvml_attempted_at.is_none()
                || cache.nvml_latest_attempt_failed;
            if fallback_active {
                cache.fallback_latest_attempt_failed
            } else {
                cache.nvml_latest_attempt_failed
            }
        }
        AttemptStatus::Baseline | AttemptStatus::Absent => false,
    };
    NvidiaResult {
        reading: AttemptResult {
            sample: state.cache.sampled_at.map(|captured_at| {
                MetricSample::new(gpu_nvidia::metrics_from_cache(&state.cache), captured_at)
            }),
            attempted_at: state.cache.attempted_at,
            failed_at: state.cache.failed_at,
            latest_attempt_failed,
            status,
        },
        decoder: decoder_outcome(state, status, latest_attempt_failed),
    }
}

fn decoder_outcome(
    state: &NvidiaState,
    status: AttemptStatus,
    latest_attempt_failed: bool,
) -> DecoderOutcome {
    let cache = &state.cache;
    let fallback_captured_latest = cache.fallback_attempted_at == cache.attempted_at
        && cache.fallback_attempted_at == cache.sampled_at
        && !cache.fallback_latest_attempt_failed;
    if fallback_captured_latest {
        return cache
            .decoder_percent
            .map_or(DecoderOutcome::ConfirmedAbsent, DecoderOutcome::Value);
    }

    let nvml_captured_latest = cache.nvml_attempted_at == cache.attempted_at
        && cache.nvml_attempted_at == cache.sampled_at
        && !cache.nvml_latest_attempt_failed;
    if nvml_captured_latest {
        if cache.decoder.latest_attempt_failed {
            return DecoderOutcome::TransientFailure;
        }
        return cache
            .decoder
            .latest
            .as_ref()
            .map_or(DecoderOutcome::ConfirmedAbsent, |sample| {
                DecoderOutcome::Value(sample.value)
            });
    }

    if latest_attempt_failed
        || status == AttemptStatus::Failed
        || cache.decoder.latest_attempt_failed
    {
        DecoderOutcome::TransientFailure
    } else {
        cache
            .decoder_percent
            .map_or(DecoderOutcome::Unmeasured, DecoderOutcome::Value)
    }
}
