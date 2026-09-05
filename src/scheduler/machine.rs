use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use super::model::{
    CancelReason, CompletionKind, ConfigGeneration, DemandPlan, EventDisposition, HistoryDeadline,
    InventoryGeneration, JobId, JobSpec, JobTicket, OwnerId, PublicationId, PublishReason,
    RescanKind, ResumeReconciliationId, RunId, SchedulerAction, SchedulerConfig, SchedulerEvent,
    SchedulerTime, TimingClass, Transition,
};

#[path = "machine/policy.rs"]
mod policy;
#[path = "machine/publication.rs"]
mod publication;

#[path = "machine/demand.rs"]
mod demand;

use policy::{
    advance_past, due_intervals, latest_due_at_or_before, retry_delay, skipped_intervals,
    sort_actions,
};

const FAST_PREFETCH: Duration = Duration::from_millis(50);
const FIRST_PAINT_TIMEOUT: Duration = Duration::from_millis(200);
const ACTIVATION_REFRESH_TIMEOUT: Duration = Duration::from_millis(100);
const DEACTIVATION_GRACE: Duration = Duration::from_secs(1);
const RETRY_BASE: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    Unstarted,
    Running,
    Suspended,
    Stopped,
}

#[derive(Debug, Clone)]
struct JobRuntime {
    spec: JobSpec,
    nominal_due: Option<SchedulerTime>,
    retry_due: Option<SchedulerTime>,
    pending_since: Option<SchedulerTime>,
    pending_history_deadline: Option<HistoryDeadline>,
    in_flight: Option<JobTicket>,
    has_sample: bool,
    failure_count: u32,
    needs_baseline: bool,
}

impl JobRuntime {
    fn new(spec: JobSpec, anchor: SchedulerTime, display_deadline: SchedulerTime) -> Self {
        let nominal_due = match spec.timing {
            TimingClass::FastDisplay => Some(display_deadline.saturating_sub(FAST_PREFETCH)),
            TimingClass::Periodic | TimingClass::History => {
                Some(anchor.saturating_add(spec.freshness))
            }
            TimingClass::Triggered => None,
        };
        Self {
            spec,
            nominal_due,
            retry_due: None,
            pending_since: None,
            pending_history_deadline: None,
            in_flight: None,
            has_sample: false,
            failure_count: 0,
            needs_baseline: false,
        }
    }

    fn mark_pending(&mut self, at: SchedulerTime) {
        debug_assert_ne!(self.spec.timing, TimingClass::History);
        self.pending_since.get_or_insert(at);
    }

    fn mark_history_pending(&mut self, deadline: SchedulerTime) {
        debug_assert_eq!(self.spec.timing, TimingClass::History);
        self.pending_since.get_or_insert(deadline);
        self.pending_history_deadline = Some(HistoryDeadline::new(deadline));
    }
}

#[derive(Debug)]
pub(crate) struct Scheduler {
    lifecycle: Lifecycle,
    now: SchedulerTime,
    config_generation: ConfigGeneration,
    inventory_generation: InventoryGeneration,
    display_interval: Duration,
    next_display: Option<SchedulerTime>,
    jobs: BTreeMap<JobId, JobRuntime>,
    demand: DemandPlan,
    presented: bool,
    effective_presented: bool,
    selected_page: super::model::PageId,
    deactivation_deadline: Option<SchedulerTime>,
    activation_deadline: Option<SchedulerTime>,
    activation_waiting: BTreeSet<JobId>,
    first_paint_deadline: Option<SchedulerTime>,
    first_paint_waiting: BTreeSet<JobId>,
    first_paint_issued: bool,
    notifications_armed: bool,
    panel_publications: BTreeSet<PublicationId>,
    in_flight_owners: BTreeMap<OwnerId, RunId>,
    cancelling: BTreeMap<RunId, JobTicket>,
    awaiting_resume_inventory: Option<ResumeReconciliationId>,
    next_run_id: u64,
    next_publication_id: u64,
    next_resume_reconciliation_id: u64,
}

impl Default for Scheduler {
    fn default() -> Self {
        Self {
            lifecycle: Lifecycle::Unstarted,
            now: SchedulerTime::ZERO,
            config_generation: ConfigGeneration::default(),
            inventory_generation: InventoryGeneration::default(),
            display_interval: Duration::from_secs(1),
            next_display: None,
            jobs: BTreeMap::new(),
            demand: DemandPlan::default(),
            presented: false,
            effective_presented: false,
            selected_page: super::model::PageId::Main,
            deactivation_deadline: None,
            activation_deadline: None,
            activation_waiting: BTreeSet::new(),
            first_paint_deadline: None,
            first_paint_waiting: BTreeSet::new(),
            first_paint_issued: false,
            notifications_armed: false,
            panel_publications: BTreeSet::new(),
            in_flight_owners: BTreeMap::new(),
            cancelling: BTreeMap::new(),
            awaiting_resume_inventory: None,
            next_run_id: 1,
            next_publication_id: 1,
            next_resume_reconciliation_id: 1,
        }
    }
}

impl Scheduler {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn handle(&mut self, event: SchedulerEvent) -> Transition {
        let at = event.at();
        if self.lifecycle != Lifecycle::Unstarted && at < self.now {
            return Transition::rejected(EventDisposition::RejectedTimeRegression);
        }
        if self.lifecycle == Lifecycle::Stopped {
            return Transition::rejected(EventDisposition::RejectedLifecycle);
        }
        if self.lifecycle == Lifecycle::Unstarted
            && !matches!(&event, SchedulerEvent::Startup { .. })
        {
            return Transition::rejected(EventDisposition::RejectedLifecycle);
        }
        if self.lifecycle != Lifecycle::Unstarted
            && matches!(&event, SchedulerEvent::Startup { .. })
        {
            return Transition::rejected(EventDisposition::RejectedLifecycle);
        }
        let stale = match &event {
            SchedulerEvent::ConfigChanged { config, .. } => {
                config.generation <= self.config_generation
            }
            SchedulerEvent::InventoryChanged { update, .. } => {
                update.generation <= self.inventory_generation
            }
            SchedulerEvent::JobFinished { ticket, .. } => {
                !self.is_current_ticket(ticket) && !self.is_cancelling_ticket(ticket)
            }
            SchedulerEvent::JobCancelled { ticket, .. } => {
                self.cancelling.get(&ticket.run_id) != Some(ticket)
            }
            SchedulerEvent::PanelPublished { publication, .. } => {
                !self.panel_publications.contains(publication)
            }
            _ => false,
        };
        if stale {
            return Transition::rejected(EventDisposition::RejectedStale);
        }
        self.now = at;
        let mut actions = Vec::new();
        let disposition = match event {
            SchedulerEvent::Startup {
                config,
                inventory_generation,
                ..
            } => self.startup(config, inventory_generation, &mut actions),
            SchedulerEvent::TimeAdvanced { .. } => {
                if self.lifecycle == Lifecycle::Running {
                    self.process_time(&mut actions);
                }
                EventDisposition::Accepted
            }
            SchedulerEvent::ConfigChanged { config, .. } => {
                self.reconfigure(config, CancelReason::ConfigChanged, &mut actions);
                EventDisposition::Accepted
            }
            SchedulerEvent::InventoryChanged { update, .. } => {
                self.change_inventory(update, &mut actions);
                EventDisposition::Accepted
            }
            SchedulerEvent::DemandChanged { demand, .. } => {
                self.change_demand(demand, &mut actions);
                EventDisposition::Accepted
            }
            SchedulerEvent::TooltipPresented { presented, .. } => {
                self.change_presentation(presented, &mut actions);
                EventDisposition::Accepted
            }
            SchedulerEvent::SelectedPageChanged { page, .. } => {
                self.change_page(page, &mut actions);
                EventDisposition::Accepted
            }
            SchedulerEvent::DisplayRefreshRequested { .. } => {
                self.issue_publish(
                    PublishReason::ConfigChanged,
                    None,
                    0,
                    true,
                    self.effective_presented,
                    &mut actions,
                );
                EventDisposition::Accepted
            }
            SchedulerEvent::TooltipRefreshRequested { .. } => {
                self.issue_publish(
                    PublishReason::TooltipRefresh,
                    None,
                    0,
                    false,
                    true,
                    &mut actions,
                );
                EventDisposition::Accepted
            }
            SchedulerEvent::JobFinished {
                ticket,
                completion,
                notification_ready,
                ..
            } => {
                if self.is_cancelling_ticket(&ticket) {
                    if self.lifecycle == Lifecycle::Running {
                        self.process_time(&mut actions);
                    }
                    self.finish_cancellation(&ticket);
                    EventDisposition::Accepted
                } else {
                    if self.lifecycle == Lifecycle::Running {
                        self.process_time(&mut actions);
                    }
                    if self.finish_job(&ticket, completion, notification_ready, &mut actions) {
                        EventDisposition::Accepted
                    } else {
                        EventDisposition::RejectedStale
                    }
                }
            }
            SchedulerEvent::JobCancelled { ticket, .. } => {
                if self.lifecycle == Lifecycle::Running {
                    self.process_time(&mut actions);
                }
                if self.finish_cancellation(&ticket) {
                    EventDisposition::Accepted
                } else {
                    EventDisposition::RejectedStale
                }
            }
            SchedulerEvent::RefreshTriggered { job, .. } => {
                if self.lifecycle == Lifecycle::Running {
                    self.process_time(&mut actions);
                }
                self.trigger(job);
                EventDisposition::Accepted
            }
            SchedulerEvent::PanelPublished { publication, .. } => {
                if self.panel_publications.remove(&publication) {
                    self.notifications_armed = true;
                    EventDisposition::Accepted
                } else {
                    EventDisposition::RejectedStale
                }
            }
            SchedulerEvent::Suspend { .. } => {
                self.suspend(&mut actions);
                EventDisposition::Accepted
            }
            SchedulerEvent::Resume { .. } => {
                self.resume(&mut actions);
                EventDisposition::Accepted
            }
            SchedulerEvent::Shutdown { .. } => {
                self.stop(CancelReason::Shutdown, None, &mut actions);
                EventDisposition::Accepted
            }
            SchedulerEvent::CriticalFailure { component, .. } => {
                self.stop(CancelReason::CriticalFailure, Some(component), &mut actions);
                EventDisposition::Accepted
            }
        };

        if disposition == EventDisposition::Accepted && self.lifecycle == Lifecycle::Running {
            self.dispatch(&mut actions);
            if let Some(deadline) = self.next_wake() {
                actions.push(SchedulerAction::ScheduleDeadline { at: deadline });
            }
        }
        sort_actions(&mut actions);
        Transition {
            disposition,
            actions,
        }
    }

    pub(crate) fn next_wake(&self) -> Option<SchedulerTime> {
        if self.lifecycle != Lifecycle::Running || self.awaiting_resume_inventory.is_some() {
            return None;
        }
        let demanded = self.current_demand();
        let job_deadlines = self.jobs.iter().filter_map(|(id, runtime)| {
            demanded.contains(id).then_some(
                runtime
                    .retry_due
                    .into_iter()
                    .chain(runtime.nominal_due)
                    .min(),
            )?
        });
        job_deadlines
            .chain(self.next_display)
            .chain(self.first_paint_deadline)
            .chain(self.deactivation_deadline)
            .chain(self.activation_deadline)
            .min()
    }

    pub(crate) fn is_current_ticket(&self, ticket: &JobTicket) -> bool {
        self.lifecycle == Lifecycle::Running
            && ticket.config_generation == self.config_generation
            && ticket.inventory_generation == self.inventory_generation
            && self
                .jobs
                .get(&ticket.job)
                .is_some_and(|runtime| runtime.in_flight.as_ref() == Some(ticket))
    }

    fn is_cancelling_ticket(&self, ticket: &JobTicket) -> bool {
        self.cancelling.get(&ticket.run_id) == Some(ticket)
    }

    #[cfg(test)]
    pub(crate) fn has_sample(&self, job: &JobId) -> bool {
        self.jobs.get(job).is_some_and(|runtime| runtime.has_sample)
    }

    #[cfg(test)]
    pub(crate) fn notifications_armed(&self) -> bool {
        self.notifications_armed
    }

    fn startup(
        &mut self,
        config: SchedulerConfig,
        inventory_generation: InventoryGeneration,
        actions: &mut Vec<SchedulerAction>,
    ) -> EventDisposition {
        if self.lifecycle != Lifecycle::Unstarted {
            return EventDisposition::RejectedLifecycle;
        }
        self.lifecycle = Lifecycle::Running;
        self.inventory_generation = inventory_generation;
        self.config_generation = config.generation;
        self.display_interval = config.display_interval;
        self.demand = config.demand;
        let first_display = self.now.saturating_add(self.display_interval);
        self.next_display = Some(first_display);
        self.first_paint_deadline = Some(self.now.saturating_add(FIRST_PAINT_TIMEOUT));
        self.jobs = config
            .jobs
            .into_iter()
            .map(|spec| {
                let id = spec.id.clone();
                (id, JobRuntime::new(spec, self.now, first_display))
            })
            .collect();
        let demanded = self.current_demand();
        for (id, runtime) in &mut self.jobs {
            if demanded.contains(id) {
                if runtime.spec.timing != TimingClass::History {
                    runtime.mark_pending(self.now);
                }
                if runtime.spec.startup_panel && runtime.spec.timing != TimingClass::History {
                    self.first_paint_waiting.insert(id.clone());
                }
            }
        }
        if self.first_paint_waiting.is_empty() {
            self.issue_first_paint(PublishReason::FirstPaintReady, actions);
        }
        EventDisposition::Accepted
    }

    fn reconfigure(
        &mut self,
        config: SchedulerConfig,
        reason: CancelReason,
        actions: &mut Vec<SchedulerAction>,
    ) {
        self.config_generation = config.generation;
        self.display_interval = config.display_interval;
        self.demand = config.demand;
        self.next_display = Some(self.now.saturating_add(self.display_interval));
        self.replace_jobs(config.jobs, reason, actions);
        self.reanchor_fast_jobs();
        self.refresh_demand(false, actions);
        let demanded = self.current_demand();
        for (id, runtime) in &mut self.jobs {
            if demanded.contains(id) && id.kind == super::model::JobKind::HardwareDiscovery {
                runtime.mark_pending(self.now);
            }
        }
        if self.first_paint_issued {
            self.issue_publish(
                PublishReason::ConfigChanged,
                None,
                0,
                true,
                self.effective_presented,
                actions,
            );
        }
    }

    fn replace_jobs(
        &mut self,
        specs: Vec<JobSpec>,
        reason: CancelReason,
        actions: &mut Vec<SchedulerAction>,
    ) {
        self.cancel_all(reason, actions);
        let display_deadline = self
            .next_display
            .unwrap_or_else(|| self.now.saturating_add(self.display_interval));
        let mut old = std::mem::take(&mut self.jobs);
        let mut replacement = BTreeMap::new();
        for spec in specs {
            let id = spec.id.clone();
            let (runtime, source_changed) = if let Some(previous) = old.remove(&id) {
                let source_changed = previous.spec.history_source != spec.history_source
                    || previous.spec.amd != spec.amd;
                if previous.spec == spec {
                    (JobRuntime { spec, ..previous }, false)
                } else {
                    let mut runtime = JobRuntime::new(spec, self.now, display_deadline);
                    runtime.has_sample = previous.has_sample && !source_changed;
                    (runtime, source_changed)
                }
            } else {
                (JobRuntime::new(spec, self.now, display_deadline), false)
            };
            if source_changed {
                actions.push(SchedulerAction::InvalidateJob {
                    job: id.clone(),
                    reason,
                });
            }
            replacement.insert(id, runtime);
        }
        for (id, _) in old {
            actions.push(SchedulerAction::InvalidateJob { job: id, reason });
        }
        self.jobs = replacement;
        if self.first_paint_issued {
            self.first_paint_waiting.clear();
        } else {
            self.first_paint_waiting.clear();
            let demanded = self.current_demand();
            for (id, runtime) in &mut self.jobs {
                if demanded.contains(id)
                    && runtime.spec.startup_panel
                    && runtime.spec.timing != TimingClass::History
                    && !runtime.has_sample
                {
                    runtime.mark_pending(self.now);
                    self.first_paint_waiting.insert(id.clone());
                }
            }
        }
        self.activation_waiting
            .retain(|job| self.jobs.contains_key(job));
    }

    fn process_time(&mut self, actions: &mut Vec<SchedulerAction>) {
        if self
            .deactivation_deadline
            .is_some_and(|deadline| deadline <= self.now)
        {
            self.deactivation_deadline = None;
            if !self.presented {
                self.effective_presented = false;
                self.refresh_demand(true, actions);
            }
        }

        if self
            .first_paint_deadline
            .is_some_and(|deadline| deadline <= self.now)
            && !self.first_paint_issued
        {
            self.issue_first_paint(PublishReason::FirstPaintTimeout, actions);
        }

        if self
            .activation_deadline
            .is_some_and(|deadline| deadline <= self.now)
        {
            self.activation_deadline = None;
            self.activation_waiting.clear();
            self.issue_publish(PublishReason::TooltipRefresh, None, 0, false, true, actions);
        }

        if self.first_paint_issued
            && self
                .next_display
                .is_some_and(|deadline| deadline <= self.now)
        {
            let deadline = self.next_display;
            let skipped = deadline.map_or(0, |deadline| {
                skipped_intervals(deadline, self.display_interval, self.now)
            });
            self.issue_publish(
                PublishReason::DisplayDeadline,
                deadline,
                skipped,
                true,
                self.effective_presented,
                actions,
            );
            if let Some(deadline) = self.next_display {
                self.next_display = Some(advance_past(deadline, self.display_interval, self.now));
            }
        }

        let demanded = self.current_demand();
        let source_samples = self
            .jobs
            .iter()
            .map(|(id, runtime)| (id.clone(), runtime.has_sample))
            .collect::<BTreeMap<_, _>>();
        for (id, runtime) in &mut self.jobs {
            if !demanded.contains(id) {
                continue;
            }
            if runtime
                .retry_due
                .is_some_and(|deadline| deadline <= self.now)
            {
                runtime.retry_due = None;
                runtime.mark_pending(self.now);
            }
            if runtime
                .nominal_due
                .is_some_and(|deadline| deadline <= self.now)
            {
                if runtime.spec.timing == TimingClass::History {
                    let history_ready = runtime
                        .spec
                        .history_source
                        .as_ref()
                        .is_none_or(|source| source_samples.get(source).copied() == Some(true));
                    if history_ready && let Some(deadline) = runtime.nominal_due {
                        runtime.mark_history_pending(latest_due_at_or_before(
                            deadline,
                            runtime.spec.freshness,
                            self.now,
                        ));
                    }
                } else {
                    runtime.mark_pending(self.now);
                }
                if let Some(deadline) = runtime.nominal_due {
                    runtime.nominal_due =
                        Some(advance_past(deadline, runtime.spec.freshness, self.now));
                }
            }
        }
    }

    fn finish_job(
        &mut self,
        ticket: &JobTicket,
        completion: CompletionKind,
        notification_ready: bool,
        actions: &mut Vec<SchedulerAction>,
    ) -> bool {
        if ticket.config_generation != self.config_generation
            || ticket.inventory_generation != self.inventory_generation
        {
            return false;
        }
        let Some(runtime) = self.jobs.get_mut(&ticket.job) else {
            return false;
        };
        if runtime.in_flight.as_ref() != Some(ticket) {
            return false;
        }
        runtime.in_flight = None;
        self.in_flight_owners.remove(&ticket.job.owner);
        self.first_paint_waiting.remove(&ticket.job);
        self.activation_waiting.remove(&ticket.job);

        match completion {
            CompletionKind::Captured => {
                runtime.has_sample = true;
                runtime.failure_count = 0;
                runtime.needs_baseline = false;
                runtime.retry_due = None;
            }
            CompletionKind::Baseline => {
                runtime.failure_count = 0;
                runtime.needs_baseline = true;
                runtime.retry_due = (runtime.spec.timing != TimingClass::History)
                    .then(|| self.now.saturating_add(RETRY_BASE));
            }
            CompletionKind::ConfirmedAbsent => {
                runtime.has_sample = false;
                runtime.failure_count = 0;
                runtime.needs_baseline = false;
                runtime.retry_due = None;
            }
            CompletionKind::Failed => {
                runtime.failure_count = runtime.failure_count.saturating_add(1);
                if runtime.spec.timing != TimingClass::History {
                    let delay = retry_delay(runtime.failure_count, runtime.spec.freshness);
                    let retry_at = self.now.saturating_add(delay);
                    runtime.retry_due = Some(retry_at);
                    actions.push(SchedulerAction::ApplyBackoff {
                        job: ticket.job.clone(),
                        failures: runtime.failure_count,
                        retry_at,
                    });
                }
            }
        }

        if notification_ready
            && self.notifications_armed
            && runtime.spec.timing != TimingClass::History
        {
            actions.push(SchedulerAction::EvaluateNotifications {
                ticket: ticket.clone(),
            });
        }

        if completion == CompletionKind::ConfirmedAbsent {
            let dependent_histories = self
                .jobs
                .iter_mut()
                .filter(|(_, candidate)| {
                    candidate.spec.history_source.as_ref() == Some(&ticket.job)
                })
                .map(|(id, candidate)| {
                    candidate.has_sample = false;
                    candidate.pending_since = None;
                    candidate.pending_history_deadline = None;
                    id.clone()
                })
                .collect::<Vec<_>>();
            actions.extend(dependent_histories.into_iter().map(|job| {
                SchedulerAction::InvalidateJob {
                    job,
                    reason: CancelReason::SourceReplaced,
                }
            }));
        }

        if !self.first_paint_issued && self.first_paint_waiting.is_empty() {
            self.issue_first_paint(PublishReason::FirstPaintReady, actions);
        }
        if self.activation_deadline.is_some() && self.activation_waiting.is_empty() {
            self.activation_deadline = None;
            self.issue_publish(PublishReason::TooltipRefresh, None, 0, false, true, actions);
        }
        true
    }

    fn finish_cancellation(&mut self, ticket: &JobTicket) -> bool {
        if self.cancelling.get(&ticket.run_id) != Some(ticket) {
            return false;
        }
        self.cancelling.remove(&ticket.run_id);
        if self.in_flight_owners.get(&ticket.job.owner) == Some(&ticket.run_id) {
            self.in_flight_owners.remove(&ticket.job.owner);
        }
        true
    }

    fn trigger(&mut self, job: JobId) {
        if self.current_demand().contains(&job)
            && let Some(runtime) = self.jobs.get_mut(&job)
            && runtime.spec.timing != TimingClass::History
        {
            runtime.mark_pending(self.now);
        }
    }

    fn suspend(&mut self, actions: &mut Vec<SchedulerAction>) {
        if self.lifecycle != Lifecycle::Running {
            return;
        }
        self.cancel_all(CancelReason::Suspend, actions);
        self.lifecycle = Lifecycle::Suspended;
    }

    fn resume(&mut self, actions: &mut Vec<SchedulerAction>) {
        if self.lifecycle != Lifecycle::Suspended {
            return;
        }
        self.lifecycle = Lifecycle::Running;
        self.next_display = Some(self.now.saturating_add(self.display_interval));
        self.reanchor_fast_jobs();
        for runtime in self.jobs.values_mut() {
            if matches!(
                runtime.spec.timing,
                TimingClass::Periodic | TimingClass::History
            ) && runtime
                .nominal_due
                .is_some_and(|deadline| deadline <= self.now)
                && let Some(deadline) = runtime.nominal_due
            {
                runtime.nominal_due =
                    Some(advance_past(deadline, runtime.spec.freshness, self.now));
            }
        }
        let reconciliation = ResumeReconciliationId(self.next_resume_reconciliation_id);
        self.next_resume_reconciliation_id = self.next_resume_reconciliation_id.saturating_add(1);
        self.awaiting_resume_inventory = Some(reconciliation);
        actions.push(SchedulerAction::RescanHardware {
            kind: RescanKind::VolatileInventoryAndRoute,
            resume_reconciliation: Some(reconciliation),
        });
        let demanded = self.current_demand();
        for (id, runtime) in &mut self.jobs {
            if runtime.spec.counter {
                runtime.needs_baseline = true;
                actions.push(SchedulerAction::ResetCounterBaseline { job: id.clone() });
            }
            runtime.pending_since = (demanded.contains(id)
                && runtime.spec.timing != TimingClass::History)
                .then_some(self.now);
            runtime.pending_history_deadline = None;
            runtime.retry_due = None;
        }
    }

    fn stop(
        &mut self,
        reason: CancelReason,
        component: Option<&'static str>,
        actions: &mut Vec<SchedulerAction>,
    ) {
        self.cancel_all(reason, actions);
        self.lifecycle = Lifecycle::Stopped;
        actions.push(SchedulerAction::Terminate { component });
    }

    fn cancel_all(&mut self, reason: CancelReason, actions: &mut Vec<SchedulerAction>) {
        for runtime in self.jobs.values_mut() {
            if let Some(ticket) = runtime.in_flight.take() {
                self.cancelling.insert(ticket.run_id, ticket.clone());
                actions.push(SchedulerAction::CancelJob { ticket, reason });
            }
        }
    }

    fn current_demand(&self) -> BTreeSet<JobId> {
        self.current_demand_for(self.effective_presented, &self.selected_page)
    }

    fn current_demand_for(&self, presented: bool, page: &super::model::PageId) -> BTreeSet<JobId> {
        self.demand.demanded(presented, page)
    }
}
