use super::*;
use crate::domain::Metric;
use crate::domain::readings::RetainedMetricSample;
use crate::sensors::gpu_amd::{AmdGpuSamples, AmdGpuState};

pub(crate) fn attempt_amd(
    state: &mut AmdGpuState,
    metrics: &BTreeSet<Metric>,
    attempted_at: Duration,
) -> BTreeMap<Metric, AttemptStatus> {
    state.sample(metrics, attempted_at);
    metrics
        .iter()
        .map(|metric| (*metric, metadata(state.samples(), *metric).0))
        .collect()
}

pub(crate) fn amd_capture_time(samples: &AmdGpuSamples, metrics: &[Metric]) -> Option<Duration> {
    metrics
        .iter()
        .filter_map(|metric| metadata(samples, *metric).1)
        .min()
}

fn metadata(samples: &AmdGpuSamples, metric: Metric) -> (AttemptStatus, Option<Duration>) {
    match metric {
        Metric::GpuAmdUsage => retained(&samples.usage),
        Metric::GpuAmdCodecUsage => retained(&samples.codec_usage),
        Metric::GpuAmdMemUsage => retained(&samples.memory),
        Metric::GpuAmdFreq => retained(&samples.frequency),
        Metric::GpuAmdTemp => retained(&samples.temperature),
        Metric::GpuAmdPower => retained(&samples.power),
        Metric::GpuAmdFanSpeed => retained(&samples.fan_speed),
        _ => (AttemptStatus::Absent, None),
    }
}

fn retained<T>(sample: &RetainedMetricSample<T>) -> (AttemptStatus, Option<Duration>) {
    let capture = sample.latest.as_ref().map(|sample| sample.captured_at);
    let status = if sample.latest_attempt_failed {
        AttemptStatus::Failed
    } else if capture.is_some() {
        AttemptStatus::Captured
    } else {
        AttemptStatus::Absent
    };
    (status, capture)
}
