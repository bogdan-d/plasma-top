use std::time::Duration;

use crate::scheduler::{JobId, JobKind, SchedulerAction, SchedulerTime};

use super::RETRY_BASE;

pub(super) fn retry_delay(failures: u32, freshness: Duration) -> Duration {
    let shift = failures.saturating_sub(1).min(31);
    let multiplier = 1_u32.checked_shl(shift).unwrap_or(u32::MAX);
    RETRY_BASE
        .checked_mul(multiplier)
        .unwrap_or(Duration::MAX)
        .min(freshness.max(RETRY_BASE))
}

pub(super) fn advance_past(
    deadline: SchedulerTime,
    cadence: Duration,
    now: SchedulerTime,
) -> SchedulerTime {
    if cadence.is_zero() {
        return now;
    }
    let deadline_nanos = deadline.duration().as_nanos();
    let cadence_nanos = cadence.as_nanos();
    let elapsed = now.duration().as_nanos().saturating_sub(deadline_nanos);
    let steps = elapsed / cadence_nanos + 1;
    let target = deadline_nanos.saturating_add(steps.saturating_mul(cadence_nanos));
    let seconds = target / 1_000_000_000;
    if seconds > u128::from(u64::MAX) {
        return SchedulerTime::from_duration(Duration::MAX);
    }
    let nanos = u32::try_from(target % 1_000_000_000).unwrap_or(u32::MAX);
    SchedulerTime::from_duration(Duration::new(
        u64::try_from(seconds).unwrap_or(u64::MAX),
        nanos,
    ))
}

pub(super) fn latest_due_at_or_before(
    deadline: SchedulerTime,
    cadence: Duration,
    now: SchedulerTime,
) -> SchedulerTime {
    if cadence.is_zero() || deadline >= now {
        return deadline;
    }
    let elapsed = now.duration().as_nanos() - deadline.duration().as_nanos();
    let steps = elapsed / cadence.as_nanos();
    let target = deadline
        .duration()
        .as_nanos()
        .saturating_add(steps.saturating_mul(cadence.as_nanos()));
    let seconds = target / 1_000_000_000;
    if seconds > u128::from(u64::MAX) {
        return SchedulerTime::from_duration(Duration::MAX);
    }
    SchedulerTime::from_duration(Duration::new(
        u64::try_from(seconds).unwrap_or(u64::MAX),
        u32::try_from(target % 1_000_000_000).unwrap_or(u32::MAX),
    ))
}

pub(super) fn is_cancelled_page_work(job: &JobId) -> bool {
    matches!(
        job.kind,
        JobKind::PageCommand | JobKind::PageRender | JobKind::PageProcesses
    )
}

fn action_priority(action: &SchedulerAction) -> u8 {
    match action {
        SchedulerAction::InvalidateJob { .. } => 0,
        SchedulerAction::CancelJob { .. } => 1,
        SchedulerAction::ResetCounterBaseline { .. } => 2,
        SchedulerAction::PublishDisplay { .. } => 3,
        SchedulerAction::EvaluateNotifications { .. } => 4,
        SchedulerAction::RescanHardware { .. } => 5,
        SchedulerAction::ApplyBackoff { .. } => 6,
        SchedulerAction::StartJob { .. } => 7,
        SchedulerAction::ScheduleDeadline { .. } => 8,
        SchedulerAction::Terminate { .. } => 9,
    }
}

pub(super) fn sort_actions(actions: &mut [SchedulerAction]) {
    actions.sort_by_key(action_priority);
}

pub(super) fn skipped_intervals(
    deadline: SchedulerTime,
    cadence: Duration,
    now: SchedulerTime,
) -> u64 {
    if cadence.is_zero() || now <= deadline {
        return 0;
    }
    u64::try_from(
        now.duration()
            .saturating_sub(deadline.duration())
            .as_nanos()
            / cadence.as_nanos(),
    )
    .unwrap_or(u64::MAX)
}

pub(super) fn due_intervals(deadline: SchedulerTime, cadence: Duration, now: SchedulerTime) -> u64 {
    if cadence.is_zero() || now < deadline {
        return 0;
    }
    skipped_intervals(deadline, cadence, now).saturating_add(1)
}
