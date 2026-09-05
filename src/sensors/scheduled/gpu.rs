use super::*;
use crate::sensors::attempts::{
    attempt_intel_frequency, attempt_intel_usage, attempt_nvidia_fallback, attempt_nvml_nvidia,
    nvidia_result,
};
use crate::sensors::gpu_history::DecoderOutcome;

#[allow(clippy::too_many_arguments)]
pub(super) fn execute(
    job: &JobId,
    owners: OwnerRefs<'_>,
    hw: &HardwareInventory,
    cfg: &Config,
    ctx: &mut CollectCtx<'_, '_>,
    readings: &mut DisplaySnapshot,
    notifications: &mut DisplaySnapshot,
    timings: &mut Option<&mut Timings>,
    history_deadline: Option<HistoryDeadline>,
) -> CompletionKind {
    match job.kind {
        JobKind::NvidiaNvml => {
            let status = if let Some(nvml) = ctx.nvml.as_deref_mut() {
                attempt_nvml_nvidia(owners.nvidia, nvml, (ctx.clock)().monotonic)
            } else {
                AttemptStatus::Absent
            };
            merge_nvidia(owners.nvidia, status, readings, notifications)
        }
        JobKind::NvidiaFallback => {
            let fallback_needed = ctx.nvml.is_none()
                || owners.nvidia.cache.nvml_init_failed
                || owners.nvidia.cache.nvml_latest_attempt_failed;
            let decision_at = (ctx.clock)().monotonic;
            let fallback_due = owners
                .nvidia
                .cache
                .fallback_attempted_at
                .is_none_or(|attempted| {
                    decision_at.saturating_sub(attempted)
                        >= crate::sensors::gpu_nvidia::GPU_CACHE_TTL
                });
            if fallback_needed && fallback_due {
                let status = attempt_nvidia_fallback(owners.nvidia, ctx.commands, decision_at);
                merge_nvidia(owners.nvidia, status, readings, notifications)
            } else if fallback_needed {
                merge_nvidia(
                    owners.nvidia,
                    AttemptStatus::Cached,
                    readings,
                    notifications,
                )
            } else {
                CompletionKind::ConfirmedAbsent
            }
        }
        JobKind::IntelFrequency => {
            let SourceIdentity::Path(path) = &job.source else {
                return CompletionKind::ConfirmedAbsent;
            };
            let result = attempt_intel_frequency(owners.intel_gpu, path, (ctx.clock)(), timings);
            let completion = completion(result.reading.status);
            readings.gpu_intel_freq = sample_value(result.reading);
            completion
        }
        JobKind::IntelUsage => {
            let Some(pci) = source_device(job) else {
                return CompletionKind::ConfirmedAbsent;
            };
            let result =
                attempt_intel_usage(owners.intel_gpu, ctx.proc_root, pci, (ctx.clock)(), timings);
            let completion = completion(result.reading.status);
            if let Some(sample) = result.reading.sample {
                readings.gpu_intel_usage = sample.value.get("render").copied();
                readings.gpu_intel_dec_usage = sample.value.get("video").copied();
            }
            completion
        }
        JobKind::GpuHistory => {
            let (history_readings, decoder) =
                gpu_values_at_history_deadline(job, &owners, readings, history_deadline);
            let captured_at = history_deadline.map_or_else(
                || (ctx.clock)(),
                |deadline| ClockSnapshot {
                    monotonic: deadline.at().duration(),
                    ..ClockSnapshot::default()
                },
            );
            let result =
                owners
                    .gpu_history
                    .sample(cfg, hw, &history_readings, decoder, captured_at, true);
            if let Some(result) = result {
                if let Some(history) = result.usage {
                    readings.gpu_usage_history = history.value;
                }
                if let Some(history) = result.decoder {
                    readings.gpu_dec_history = history.value;
                }
                CompletionKind::Captured
            } else {
                CompletionKind::ConfirmedAbsent
            }
        }
        _ => unreachable!("GPU execution requires a GPU job"),
    }
}

fn merge_nvidia(
    state: &crate::sensors::gpu_nvidia::NvidiaState,
    status: AttemptStatus,
    readings: &mut DisplaySnapshot,
    notifications: &mut DisplaySnapshot,
) -> CompletionKind {
    let result = nvidia_result(state, status);
    if let Some(sample) = result.reading.sample {
        if status == AttemptStatus::Captured {
            notifications.gpu_temp = sample.value.temp_celsius;
        }
        readings.gpu_temp = sample.value.temp_celsius;
        readings.gpu_usage = sample.value.usage_percent;
        readings.gpu_mem = sample.value.memory_percent;
        readings.gpu_dec = sample.value.decoder_percent;
        readings.gpu_fan = sample.value.fan_percent;
    }
    completion(status)
}

fn gpu_values_at_history_deadline(
    job: &JobId,
    owners: &OwnerRefs<'_>,
    readings: &DisplaySnapshot,
    deadline: Option<HistoryDeadline>,
) -> (DisplaySnapshot, DecoderOutcome) {
    let Some(deadline) = deadline else {
        let decoder = readings
            .gpu_dec
            .or(readings.gpu_intel_dec_usage)
            .map_or(DecoderOutcome::Unmeasured, DecoderOutcome::Value);
        return (readings.clone(), decoder);
    };
    let mut eligible = readings.clone();
    match &job.source {
        SourceIdentity::Device(source) if source == "nvidia" => {
            let cache = &owners.nvidia.cache;
            if let Some(sample) = sample_at_history_deadline(&cache.history_samples, Some(deadline))
            {
                eligible.gpu_usage = sample.value.usage_percent;
                eligible.gpu_dec = sample.value.decoder_percent;
                let decoder = sample
                    .value
                    .decoder_percent
                    .map_or(DecoderOutcome::Unmeasured, DecoderOutcome::Value);
                (eligible, decoder)
            } else {
                eligible.gpu_usage = None;
                eligible.gpu_dec = None;
                (eligible, DecoderOutcome::Unmeasured)
            }
        }
        SourceIdentity::Device(source) if source.starts_with("intel:") => {
            if let Some(sample) =
                sample_at_history_deadline(&owners.intel_gpu.usage, Some(deadline))
            {
                eligible.gpu_intel_usage = sample.value.get("render").copied();
                eligible.gpu_intel_dec_usage = sample.value.get("video").copied();
                let decoder = eligible
                    .gpu_intel_dec_usage
                    .map_or(DecoderOutcome::Unmeasured, DecoderOutcome::Value);
                (eligible, decoder)
            } else {
                eligible.gpu_intel_usage = None;
                eligible.gpu_intel_dec_usage = None;
                (eligible, DecoderOutcome::Unmeasured)
            }
        }
        _ => (eligible, DecoderOutcome::Unmeasured),
    }
}

pub(super) fn reconcile_amd(
    state: &mut crate::sensors::gpu_amd::AmdGpuState,
    hw: &HardwareInventory,
    caps: &BTreeSet<Capability>,
) {
    let amd_metrics = crate::sensors::gpu_amd::FAST_METRICS
        .iter()
        .chain(crate::sensors::gpu_amd::SLOW_METRICS)
        .copied()
        .filter(|metric| metric.capabilities().iter().any(|cap| caps.contains(cap)))
        .collect();
    let amd_source = hw
        .amd_gpu
        .as_ref()
        .map(|source| crate::sensors::gpu_amd::source_for_metrics(source, &amd_metrics));
    state.reconcile_source(amd_source.as_ref());
}

#[allow(clippy::too_many_arguments)]
pub(super) fn execute_amd(
    job: &JobId,
    metrics: Option<&BTreeSet<crate::domain::Metric>>,
    state: &mut crate::sensors::gpu_amd::AmdGpuState,
    hw: &HardwareInventory,
    cfg: &Config,
    ctx: &mut CollectCtx<'_, '_>,
    readings: &mut DisplaySnapshot,
    notifications: &mut DisplaySnapshot,
) -> CompletionKind {
    use crate::domain::Metric;
    use crate::sensors::gpu_amd::{FAST_METRICS, SLOW_METRICS};
    let Some(source) = hw.amd_gpu.as_ref() else {
        return CompletionKind::ConfirmedAbsent;
    };
    if source_device(job) != Some(format!("amd:{}", source.pci_identity).as_str()) {
        return CompletionKind::ConfirmedAbsent;
    }
    let group = if job.kind == JobKind::AmdFast {
        FAST_METRICS
    } else {
        SLOW_METRICS
    };
    let metrics = metrics.map_or_else(
        || {
            let configured = super::super::catalog::configured_capabilities(cfg);
            group
                .iter()
                .copied()
                .filter(|metric| {
                    metric
                        .capabilities()
                        .iter()
                        .any(|cap| configured.contains(cap))
                })
                .collect()
        },
        |metrics| {
            metrics
                .iter()
                .copied()
                .filter(|metric| group.contains(metric))
                .collect()
        },
    );
    let outcomes = crate::sensors::attempts::attempt_amd(state, &metrics, (ctx.clock)().monotonic);
    let samples = state.samples();
    readings.gpu_amd_usage = value(&samples.usage);
    readings.gpu_amd_codec_usage = value(&samples.codec_usage);
    readings.gpu_amd_mem_usage = value(&samples.memory);
    readings.gpu_amd_freq = value(&samples.frequency);
    readings.gpu_amd_temp = value(&samples.temperature);
    readings.gpu_amd_power = value(&samples.power);
    readings.gpu_amd_fan_speed = value(&samples.fan_speed);
    if outcomes.get(&Metric::GpuAmdTemp) == Some(&AttemptStatus::Captured) {
        notifications.gpu_amd_temp = readings.gpu_amd_temp;
    }
    if outcomes
        .values()
        .any(|status| *status == AttemptStatus::Failed)
    {
        CompletionKind::Failed
    } else if outcomes
        .values()
        .any(|status| *status == AttemptStatus::Captured)
    {
        CompletionKind::Captured
    } else {
        CompletionKind::ConfirmedAbsent
    }
}

fn value<T: Copy>(sample: &RetainedMetricSample<T>) -> Option<T> {
    sample.latest.as_ref().map(|sample| sample.value)
}
