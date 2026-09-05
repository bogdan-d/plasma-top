use super::*;

impl Scheduler {
    pub(super) fn dispatch(&mut self, actions: &mut Vec<SchedulerAction>) {
        if self.lifecycle != Lifecycle::Running || self.awaiting_resume_inventory.is_some() {
            return;
        }
        let demanded = self.current_demand();
        let source_samples = self
            .jobs
            .iter()
            .map(|(id, runtime)| (id.clone(), runtime.has_sample))
            .collect::<BTreeMap<_, _>>();
        let candidates =
            self.jobs
                .iter()
                .filter(|(id, runtime)| {
                    demanded.contains(*id)
                        && runtime.pending_since.is_some()
                        && runtime.in_flight.is_none()
                        && runtime
                            .retry_due
                            .is_none_or(|deadline| deadline <= self.now)
                        && (runtime.spec.timing != TimingClass::History
                            || runtime.spec.history_source.as_ref().is_none_or(|source| {
                                source_samples.get(source).copied() == Some(true)
                            }))
                })
                .map(|(id, runtime)| {
                    (
                        self.first_paint_issued || !runtime.spec.startup_panel,
                        runtime.pending_since.unwrap_or(self.now),
                        id.clone(),
                    )
                })
                .collect::<BTreeSet<_>>();

        for (_, _, id) in candidates {
            if self.in_flight_owners.contains_key(&id.owner) {
                continue;
            }
            let run_id = RunId(self.next_run_id);
            self.next_run_id = self.next_run_id.saturating_add(1);
            let ticket = JobTicket {
                metrics: self
                    .jobs
                    .get(&id)
                    .and_then(|runtime| runtime.spec.amd.as_ref())
                    .map(|amd| amd.metrics(self.presented))
                    .unwrap_or_default(),
                run_id,
                job: id.clone(),
                config_generation: self.config_generation,
                inventory_generation: self.inventory_generation,
                history_deadline: self
                    .jobs
                    .get_mut(&id)
                    .and_then(|runtime| runtime.pending_history_deadline.take()),
            };
            if let Some(runtime) = self.jobs.get_mut(&id) {
                runtime.pending_since = None;
                runtime.in_flight = Some(ticket.clone());
                self.in_flight_owners.insert(id.owner, run_id);
                actions.push(SchedulerAction::StartJob { ticket });
            }
        }
    }

    pub(super) fn reanchor_fast_jobs(&mut self) {
        let Some(display_deadline) = self.next_display else {
            return;
        };
        let due = display_deadline.saturating_sub(FAST_PREFETCH);
        for runtime in self.jobs.values_mut() {
            if runtime.spec.timing == TimingClass::FastDisplay {
                runtime.nominal_due = Some(due);
            }
        }
    }

    pub(super) fn issue_first_paint(
        &mut self,
        reason: PublishReason,
        actions: &mut Vec<SchedulerAction>,
    ) {
        if self.first_paint_issued {
            return;
        }
        self.first_paint_issued = true;
        self.first_paint_deadline = None;
        self.first_paint_waiting.clear();
        let skipped = self.next_display.map_or(0, |deadline| {
            due_intervals(deadline, self.display_interval, self.now)
        });
        if self
            .next_display
            .is_some_and(|deadline| deadline <= self.now)
            && let Some(deadline) = self.next_display
        {
            self.next_display = Some(advance_past(deadline, self.display_interval, self.now));
            self.reanchor_fast_jobs();
        }
        self.issue_publish(
            reason,
            None,
            skipped,
            true,
            self.effective_presented,
            actions,
        );
    }

    #[expect(
        clippy::collapsible_if,
        reason = "the publication-surface gate is separate from tooltip-refresh coalescing"
    )]
    pub(super) fn issue_publish(
        &mut self,
        reason: PublishReason,
        display_deadline: Option<SchedulerTime>,
        skipped_display_deadlines: u64,
        panel: bool,
        tooltip: bool,
        actions: &mut Vec<SchedulerAction>,
    ) {
        let tooltip = tooltip && self.presented;
        if !panel && !tooltip {
            return;
        }
        if reason == PublishReason::TooltipRefresh
            && actions.iter().any(|action| {
                matches!(
                    action,
                    SchedulerAction::PublishDisplay { tooltip: true, .. }
                )
            })
        {
            return;
        }
        if panel && tooltip {
            if let Some(SchedulerAction::PublishDisplay {
                publication,
                reason: existing_reason,
                display_deadline: existing_deadline,
                skipped_display_deadlines: existing_skipped,
                panel: existing_panel,
                tooltip: existing_tooltip,
            }) = actions.iter_mut().find(|action| {
                matches!(
                    action,
                    SchedulerAction::PublishDisplay {
                        reason: PublishReason::TooltipRefresh,
                        tooltip: true,
                        ..
                    }
                )
            }) {
                *existing_reason = reason;
                *existing_deadline = display_deadline;
                *existing_skipped = skipped_display_deadlines;
                *existing_panel = true;
                *existing_tooltip = true;
                self.panel_publications.insert(*publication);
                return;
            }
        }
        let publication = PublicationId(self.next_publication_id);
        self.next_publication_id = self.next_publication_id.saturating_add(1);
        if panel {
            self.panel_publications.insert(publication);
        }
        actions.push(SchedulerAction::PublishDisplay {
            publication,
            reason,
            display_deadline,
            skipped_display_deadlines,
            panel,
            tooltip,
        });
    }
}
