use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::task::{JoinError, JoinHandle, JoinSet};

use crate::adapters::ProductionClock;
use crate::domain::boundary::{ClockSnapshot, IoEvent};
use crate::file_watch::{FileWatcher, WatchSource};
use crate::scheduler::{
    JobKind, OwnerId, RefreshTrigger, Scheduler, SchedulerAction, SchedulerEvent, SchedulerTime,
};

use super::state::RuntimeState;
use super::worker::{DispatchValidity, OwnerCompletion, OwnerMessage};
use super::{FIRST_PAINT_DEADLINE, enqueue, now};

pub(super) enum LoopWake {
    OwnerExit(Option<Result<OwnerId, JoinError>>),
    NotificationExit,
    Shutdown,
    Completion(Option<OwnerCompletion>),
    Files(std::io::Result<std::collections::BTreeSet<WatchSource>>),
    Sleep,
}

#[cfg(test)]
pub(super) async fn wait_for_wake(
    owner_tasks: &mut JoinSet<OwnerId>,
    notification_task: &mut JoinHandle<()>,
    shutdown: &mut watch::Receiver<bool>,
    completions: &mut mpsc::Receiver<OwnerCompletion>,
    sleep_for: Duration,
) -> LoopWake {
    tokio::select! {
        biased;
        exit = owner_tasks.join_next(), if !owner_tasks.is_empty() => LoopWake::OwnerExit(exit),
        _ = notification_task => LoopWake::NotificationExit,
        _ = shutdown.changed() => LoopWake::Shutdown,
        completion = completions.recv() => LoopWake::Completion(completion),
        () = tokio::time::sleep(sleep_for) => LoopWake::Sleep,
    }
}

pub(super) async fn wait_for_wake_with_files(
    owner_tasks: &mut JoinSet<OwnerId>,
    notification_task: &mut JoinHandle<()>,
    shutdown: &mut watch::Receiver<bool>,
    completions: &mut mpsc::Receiver<OwnerCompletion>,
    file_watcher: &mut FileWatcher,
    sleep_for: Duration,
) -> LoopWake {
    tokio::select! {
        biased;
        exit = owner_tasks.join_next(), if !owner_tasks.is_empty() => LoopWake::OwnerExit(exit),
        _ = notification_task => LoopWake::NotificationExit,
        _ = shutdown.changed() => LoopWake::Shutdown,
        completion = completions.recv() => LoopWake::Completion(completion),
        changed = file_watcher.changed() => LoopWake::Files(changed),
        () = tokio::time::sleep(sleep_for) => LoopWake::Sleep,
    }
}

pub(super) fn preempt_for_shutdown(
    requested: bool,
    at: SchedulerTime,
    scheduler: &mut Scheduler,
    pending_completion: &mut Option<(OwnerCompletion, SchedulerTime)>,
    actions: &mut VecDeque<SchedulerAction>,
    owner_messages: &mut VecDeque<(OwnerId, OwnerMessage)>,
) {
    if !requested
        || actions
            .iter()
            .any(|action| matches!(action, SchedulerAction::Terminate { component: Some(_) }))
    {
        return;
    }
    if let Some((OwnerCompletion::Job(mut completion), _)) = pending_completion.take()
        && let Some(decision) = completion.decision.take()
    {
        let _ = decision.send(false);
    }
    actions.clear();
    owner_messages.clear();
    actions.extend(scheduler.handle(SchedulerEvent::Shutdown { at }).actions);
}

pub(super) fn sleep_duration(
    scheduler: &Scheduler,
    state: &RuntimeState,
    boot: ClockSnapshot,
    now: ClockSnapshot,
) -> Duration {
    let scheduler_sleep = scheduler
        .next_wake()
        .map_or(Duration::from_secs(86_400), |wake| {
            wake.duration().saturating_sub(now.monotonic)
        });
    state
        .deferred_first_paint
        .as_ref()
        .map_or(scheduler_sleep, |_| {
            boot.monotonic
                .saturating_add(FIRST_PAINT_DEADLINE)
                .saturating_sub(now.monotonic)
                .min(scheduler_sleep)
        })
}

pub(super) fn process_io_events(
    events: Vec<IoEvent>,
    scheduler: &mut Scheduler,
    state: &RuntimeState,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
    validity: &DispatchValidity,
) {
    let mut lifecycle_actions = Vec::new();
    for event in events {
        match event {
            IoEvent::PrepareForSleep(true) => {
                validity.advance_lifecycle();
                actions.retain(|action| !is_resume_work(action));
                lifecycle_actions.retain(|action| !is_resume_work(action));
                lifecycle_actions.extend(
                    scheduler
                        .handle(SchedulerEvent::Suspend { at: now(clock) })
                        .actions,
                );
            }
            IoEvent::PrepareForSleep(false) => lifecycle_actions.extend(
                scheduler
                    .handle(SchedulerEvent::Resume { at: now(clock) })
                    .actions,
            ),
            IoEvent::UpowerChanged => {
                for job in state
                    .scheduler_config()
                    .jobs
                    .into_iter()
                    .filter(|spec| {
                        matches!(
                            spec.id.kind,
                            JobKind::SystemBattery | JobKind::PeripheralBattery
                        ) || matches!(
                            spec.id.source,
                            crate::scheduler::SourceIdentity::Inventory(
                                crate::domain::readings::InventoryFamily::SystemBattery
                                    | crate::domain::readings::InventoryFamily::Peripheral
                            )
                        )
                    })
                    .map(|spec| spec.id)
                {
                    enqueue(
                        actions,
                        scheduler.handle(SchedulerEvent::RefreshTriggered {
                            at: now(clock),
                            job,
                            trigger: RefreshTrigger::PeripheralChanged,
                        }),
                    );
                }
            }
        }
    }
    for action in lifecycle_actions.into_iter().rev() {
        actions.push_front(action);
    }
}

fn is_resume_work(action: &SchedulerAction) -> bool {
    matches!(
        action,
        SchedulerAction::RescanHardware { .. } | SchedulerAction::ResetCounterBaseline { .. }
    )
}
