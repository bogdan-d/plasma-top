use std::collections::BTreeSet;

use crate::scheduler::{
    CancelReason, DemandPlan, InventoryUpdate, JobId, PublishReason, SchedulerAction, TimingClass,
};

use super::policy::is_cancelled_page_work;
use super::{ACTIVATION_REFRESH_TIMEOUT, DEACTIVATION_GRACE, Scheduler};

impl Scheduler {
    pub(super) fn change_inventory(
        &mut self,
        update: InventoryUpdate,
        actions: &mut Vec<SchedulerAction>,
    ) {
        let resumes_dispatch = update.resume_acknowledgement == self.awaiting_resume_inventory
            && self.awaiting_resume_inventory.is_some();
        let demand_before_inventory = self.current_demand();
        self.inventory_generation = update.generation;
        self.demand = update.demand;
        self.replace_jobs(update.jobs, CancelReason::SourceReplaced, actions);
        if resumes_dispatch {
            self.awaiting_resume_inventory = None;
        }
        let demand_after_inventory = self.current_demand();
        self.activate_jobs(
            &demand_before_inventory,
            &demand_after_inventory,
            self.effective_presented,
        );
        self.refresh_demand(false, actions);
        if resumes_dispatch {
            self.refresh_all_demanded();
        }
    }

    pub(super) fn change_demand(&mut self, demand: DemandPlan, actions: &mut Vec<SchedulerAction>) {
        let before = self.current_demand();
        self.demand = demand;
        let after = self.current_demand();
        self.cancel_demand_ended(&before, &after, actions);
        self.activate_jobs(&before, &after, self.effective_presented);
        self.refresh_demand(false, actions);
        self.rebuild_first_paint_blockers(actions);
    }

    pub(super) fn change_presentation(
        &mut self,
        presented: bool,
        actions: &mut Vec<SchedulerAction>,
    ) {
        let was_presented = self.presented;
        self.presented = presented;
        if presented {
            self.deactivation_deadline = None;
            let was_effective = self.effective_presented;
            self.effective_presented = true;
            if !was_effective {
                let before = self.current_demand_for(false, &self.selected_page);
                let after = self.current_demand();
                self.activate_jobs(&before, &after, true);
            }
            if !was_presented {
                self.issue_publish(
                    PublishReason::TooltipActivated,
                    None,
                    0,
                    false,
                    true,
                    actions,
                );
            }
        } else if was_presented && self.effective_presented {
            self.deactivation_deadline = Some(self.now.saturating_add(DEACTIVATION_GRACE));
        }
    }

    pub(super) fn change_page(
        &mut self,
        page: crate::scheduler::PageId,
        actions: &mut Vec<SchedulerAction>,
    ) {
        if page == self.selected_page {
            return;
        }
        let before = self.current_demand();
        self.selected_page = page;
        if self.effective_presented {
            let after = self.current_demand();
            self.cancel_demand_ended(&before, &after, actions);
            self.activate_jobs(&before, &after, true);
            if self.presented {
                self.issue_publish(PublishReason::PageChanged, None, 0, false, true, actions);
            }
        }
    }

    pub(super) fn activate_jobs(
        &mut self,
        before: &BTreeSet<JobId>,
        after: &BTreeSet<JobId>,
        track_tooltip_refresh: bool,
    ) {
        self.activation_waiting.retain(|job| after.contains(job));
        for job in after.difference(before) {
            if let Some(runtime) = self.jobs.get_mut(job)
                && runtime.spec.timing != TimingClass::History
            {
                runtime.mark_pending(self.now);
                if track_tooltip_refresh {
                    self.activation_waiting.insert(job.clone());
                }
            }
        }
        if self.activation_waiting.is_empty() {
            self.activation_deadline = None;
        } else if track_tooltip_refresh && self.activation_deadline.is_none() {
            self.activation_deadline = Some(self.now.saturating_add(ACTIVATION_REFRESH_TIMEOUT));
        }
    }

    fn cancel_demand_ended(
        &mut self,
        before: &BTreeSet<JobId>,
        after: &BTreeSet<JobId>,
        actions: &mut Vec<SchedulerAction>,
    ) {
        for id in before
            .difference(after)
            .filter(|id| is_cancelled_page_work(id))
        {
            let Some(runtime) = self.jobs.get_mut(id) else {
                continue;
            };
            runtime.pending_since = None;
            runtime.pending_history_deadline = None;
            runtime.retry_due = None;
            if let Some(ticket) = runtime.in_flight.take() {
                self.cancelling.insert(ticket.run_id, ticket.clone());
                actions.push(SchedulerAction::CancelJob {
                    ticket,
                    reason: CancelReason::DemandEnded,
                });
            }
        }
    }

    pub(super) fn refresh_demand(
        &mut self,
        cancel_page_jobs: bool,
        actions: &mut Vec<SchedulerAction>,
    ) {
        let demanded = self.current_demand();
        for (id, runtime) in &mut self.jobs {
            if demanded.contains(id) {
                if runtime.spec.timing != TimingClass::History
                    && runtime.in_flight.is_none()
                    && !runtime.has_sample
                {
                    runtime.mark_pending(self.now);
                }
            } else {
                runtime.pending_since = None;
                runtime.pending_history_deadline = None;
                runtime.retry_due = None;
                if cancel_page_jobs
                    && is_cancelled_page_work(id)
                    && let Some(ticket) = runtime.in_flight.take()
                {
                    self.cancelling.insert(ticket.run_id, ticket.clone());
                    actions.push(SchedulerAction::CancelJob {
                        ticket,
                        reason: CancelReason::DemandEnded,
                    });
                }
            }
        }
    }

    fn refresh_all_demanded(&mut self) {
        let demanded = self.current_demand();
        for (id, runtime) in &mut self.jobs {
            if demanded.contains(id) && runtime.spec.timing != TimingClass::History {
                runtime.mark_pending(self.now);
                runtime.retry_due = None;
            }
        }
    }

    fn rebuild_first_paint_blockers(&mut self, actions: &mut Vec<SchedulerAction>) {
        if self.first_paint_issued {
            return;
        }
        let demanded = self.current_demand();
        self.first_paint_waiting = self
            .jobs
            .iter()
            .filter(|(id, runtime)| {
                demanded.contains(*id)
                    && runtime.spec.startup_panel
                    && runtime.spec.timing != TimingClass::History
                    && !runtime.has_sample
            })
            .map(|(id, _)| id.clone())
            .collect();
        if self.first_paint_waiting.is_empty() {
            self.issue_first_paint(PublishReason::FirstPaintReady, actions);
        }
    }
}
