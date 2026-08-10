#![allow(clippy::expect_used)]

use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

mod demand;
mod history;
mod identity;
mod lifecycle;
mod timing;

fn at_millis(milliseconds: u64) -> SchedulerTime {
    SchedulerTime::from_duration(Duration::from_millis(milliseconds))
}

fn at_seconds(seconds: u64) -> SchedulerTime {
    SchedulerTime::from_duration(Duration::from_secs(seconds))
}

fn job(owner: OwnerId, kind: JobKind) -> JobId {
    JobId::singleton(owner, kind)
}

fn sourced(owner: OwnerId, kind: JobKind, source: &str) -> JobId {
    JobId::with_source(owner, kind, SourceIdentity::Device(source.to_owned()))
}

fn config(
    display_interval: Duration,
    specs: Vec<JobSpec>,
    hidden: impl IntoIterator<Item = JobId>,
) -> SchedulerConfig {
    SchedulerConfig {
        generation: ConfigGeneration(1),
        display_interval,
        jobs: specs,
        demand: DemandPlan {
            hidden: hidden.into_iter().collect(),
            ..DemandPlan::default()
        },
    }
}

fn plan(
    hidden: impl IntoIterator<Item = JobId>,
    main: impl IntoIterator<Item = JobId>,
    pages: impl IntoIterator<Item = (PageId, BTreeSet<JobId>)>,
) -> DemandPlan {
    DemandPlan {
        hidden: hidden.into_iter().collect(),
        main_tooltip: main.into_iter().collect(),
        pages: pages.into_iter().collect::<BTreeMap<_, _>>(),
    }
}

fn startup(scheduler: &mut Scheduler, config: SchedulerConfig) -> Transition {
    scheduler.handle(SchedulerEvent::Startup {
        at: SchedulerTime::ZERO,
        config,
        inventory_generation: InventoryGeneration(1),
    })
}

fn starts(transition: &Transition) -> Vec<JobTicket> {
    transition
        .actions
        .iter()
        .filter_map(|action| match action {
            SchedulerAction::StartJob { ticket } => Some(ticket.clone()),
            SchedulerAction::CancelJob { .. }
            | SchedulerAction::InvalidateJob { .. }
            | SchedulerAction::ResetCounterBaseline { .. }
            | SchedulerAction::PublishDisplay { .. }
            | SchedulerAction::EvaluateNotifications { .. }
            | SchedulerAction::RescanHardware { .. }
            | SchedulerAction::ApplyBackoff { .. }
            | SchedulerAction::ScheduleDeadline { .. }
            | SchedulerAction::Terminate { .. } => None,
        })
        .collect()
}

fn first_start(transition: &Transition) -> JobTicket {
    starts(transition).into_iter().next().expect("start action")
}

fn finish(
    scheduler: &mut Scheduler,
    at: SchedulerTime,
    ticket: JobTicket,
    completion: CompletionKind,
) -> Transition {
    scheduler.handle(SchedulerEvent::JobFinished {
        at,
        ticket,
        completion,
        notification_ready: completion == CompletionKind::Captured,
    })
}

fn publications(transition: &Transition) -> Vec<(PublicationId, PublishReason, bool, bool)> {
    transition
        .actions
        .iter()
        .filter_map(|action| match action {
            SchedulerAction::PublishDisplay {
                publication,
                reason,
                panel,
                tooltip,
            } => Some((*publication, *reason, *panel, *tooltip)),
            SchedulerAction::CancelJob { .. }
            | SchedulerAction::InvalidateJob { .. }
            | SchedulerAction::ResetCounterBaseline { .. }
            | SchedulerAction::EvaluateNotifications { .. }
            | SchedulerAction::RescanHardware { .. }
            | SchedulerAction::ApplyBackoff { .. }
            | SchedulerAction::StartJob { .. }
            | SchedulerAction::ScheduleDeadline { .. }
            | SchedulerAction::Terminate { .. } => None,
        })
        .collect()
}

fn has_notification(transition: &Transition, ticket: &JobTicket) -> bool {
    transition.actions.iter().any(|action| {
        matches!(
            action,
            SchedulerAction::EvaluateNotifications { ticket: notified } if notified == ticket
        )
    })
}
