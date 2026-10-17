use super::*;
use crate::sensors::npu_amd::AmdNpuState;

pub(super) fn reconcile(
    state: &mut AmdNpuState,
    hw: &HardwareInventory,
    caps: &BTreeSet<Capability>,
) {
    let demanded = [
        Capability::NpuAmdUsage,
        Capability::NpuAmdPower,
        Capability::NpuAmdFrequency,
    ]
    .iter()
    .any(|capability| caps.contains(capability));
    state.reconcile_source(hw.amd_npu.as_ref().filter(|_| demanded));
}

// One table read updates every NPU field, whichever item demanded the job.
pub(super) fn execute(
    job: &JobId,
    state: &mut AmdNpuState,
    hw: &HardwareInventory,
    ctx: &mut CollectCtx<'_, '_>,
    readings: &mut DisplaySnapshot,
) -> CompletionKind {
    let Some(source) = hw.amd_npu.as_ref() else {
        return CompletionKind::ConfirmedAbsent;
    };
    if source_device(job) != Some(source.identity().as_str()) {
        return CompletionKind::ConfirmedAbsent;
    }
    state.sample((ctx.clock)().monotonic);
    let samples = state.samples();
    readings.npu_amd_usage = value(&samples.usage);
    readings.npu_amd_power_mw = value(&samples.power_mw);
    readings.npu_amd_freq = value(&samples.frequency);
    let statuses = [
        attempt_status(&samples.usage),
        attempt_status(&samples.power_mw),
        attempt_status(&samples.frequency),
    ];
    if statuses.contains(&AttemptStatus::Failed) {
        CompletionKind::Failed
    } else if statuses.contains(&AttemptStatus::Captured) {
        CompletionKind::Captured
    } else {
        CompletionKind::ConfirmedAbsent
    }
}

pub(super) fn capture_time(state: &AmdNpuState) -> Option<Duration> {
    let samples = state.samples();
    [
        captured_at(&samples.usage),
        captured_at(&samples.power_mw),
        captured_at(&samples.frequency),
    ]
    .into_iter()
    .flatten()
    .min()
}

fn value<T: Copy>(sample: &RetainedMetricSample<T>) -> Option<T> {
    sample.latest.as_ref().map(|sample| sample.value)
}

fn captured_at<T>(sample: &RetainedMetricSample<T>) -> Option<Duration> {
    sample.latest.as_ref().map(|sample| sample.captured_at)
}

fn attempt_status<T>(sample: &RetainedMetricSample<T>) -> AttemptStatus {
    if sample.latest_attempt_failed {
        AttemptStatus::Failed
    } else if sample.latest.is_some() {
        AttemptStatus::Captured
    } else {
        AttemptStatus::Absent
    }
}
