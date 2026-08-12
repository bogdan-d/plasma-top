//! Optional scheduler profiling session and truthful timed report.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fmt::{Display, Formatter, Write as _};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::adapters::{
    ProductionCommandRunner, ProductionDbusFacade, ProductionNotificationFacade,
};
use crate::domain::boundary::{
    BoundaryError, CommandOutput, CommandRunner, DbusFacade, DbusOutput, DbusRequest,
    NotificationError, NotificationFacade, NotificationPayload,
};
use crate::scheduler::{
    CompletionKind, ConfigGeneration, JobId, JobKind, PageId, PublicationId, PublishReason, RunId,
    SchedulerAction, Transition,
};

#[path = "profiling/stimulus.rs"]
mod stimulus;
pub(crate) use stimulus::{ProfileStimuli, ProfileStimulusConfig};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptDisposition {
    Accepted(CompletionKind),
    Rejected,
    Cancelled,
}

#[derive(Debug)]
struct Attempt {
    job: String,
    capture_identity: JobId,
    queued_at: Duration,
    started_at: Option<Duration>,
    finished_at: Option<Duration>,
    captured_at: Option<Duration>,
    disposition: Option<AttemptDisposition>,
}

#[derive(Debug, Default)]
struct JobStats {
    attempts: u64,
    captured: u64,
    baseline: u64,
    absent: u64,
    failed: u64,
    rejected: u64,
    cancelled: u64,
    in_flight: u64,
    queue: Vec<Duration>,
    run: Vec<Duration>,
}

#[derive(Debug, Default)]
struct PublicationStats {
    render_by_reason: BTreeMap<&'static str, Vec<Duration>>,
    scheduled_lateness: Vec<Duration>,
    first_paint_latency: Vec<Duration>,
    skipped_display_deadlines: u64,
    over_50ms_publications: u64,
    first_paint_over_250ms: u64,
    maximum_sample_age: Option<Duration>,
    event_latency: BTreeMap<ProfileEventKind, EventLatency>,
}

#[derive(Debug, Default)]
struct EventLatency {
    completed: Vec<Duration>,
    over_100ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct StimulusId(pub(crate) u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StimulusExpectation {
    Presentation {
        presented: bool,
        html_publication: bool,
    },
    Page {
        index: usize,
        page: PageId,
    },
    Config {
        generation: ConfigGeneration,
        overlay: bool,
    },
}

#[derive(Debug)]
struct StimulusRecord {
    kind: ProfileEventKind,
    expected: StimulusExpectation,
    started_at: Duration,
    observed: bool,
    publication: Option<PublicationId>,
    completed: bool,
    timed_out: bool,
}

#[derive(Debug, Default)]
struct StimulusStats {
    enabled: bool,
    planned: usize,
    records: BTreeMap<StimulusId, StimulusRecord>,
    publications: BTreeMap<PublicationId, StimulusId>,
    skipped: usize,
    requested_state: Option<ProfileStimulusState>,
    observed_state: Option<ProfileStimulusState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProfileStimulusState {
    pub(crate) presented: bool,
    pub(crate) page_index: usize,
    pub(crate) page: PageId,
    pub(crate) config_generation: ConfigGeneration,
    pub(crate) overlay: bool,
}

impl Display for ProfileStimulusState {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "presented={} page_index={} page={} config_generation={} overlay={}",
            self.presented,
            self.page_index,
            self.page.as_id(),
            self.config_generation.0,
            self.overlay
        )
    }
}

/// External profiling stimuli whose publication latency is measured separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ProfileEventKind {
    Page,
    Control,
    Config,
}

#[derive(Debug, Default)]
struct ShutdownStats {
    started: Option<Instant>,
    duration: Option<Duration>,
}

/// Detailed collection sink instantiated only by timed profiling.
#[derive(Debug, Default)]
pub(crate) struct ProfileSession {
    attempts: Mutex<BTreeMap<RunId, Attempt>>,
    accepted_captures: Mutex<BTreeMap<JobId, Duration>>,
    process_calls: AtomicU64,
    dbus_calls: AtomicU64,
    wake_lateness: Mutex<Vec<Duration>>,
    publications: Mutex<PublicationStats>,
    stimuli: Mutex<StimulusStats>,
    shutdown: Mutex<ShutdownStats>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProfilingCommandRunner {
    inner: ProductionCommandRunner,
    session: Arc<ProfileSession>,
}

impl ProfilingCommandRunner {
    pub(crate) fn new(inner: ProductionCommandRunner, session: Arc<ProfileSession>) -> Self {
        Self { inner, session }
    }
}

impl CommandRunner for ProfilingCommandRunner {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        timeout: Duration,
    ) -> Result<CommandOutput, BoundaryError> {
        self.session.record_process_call();
        self.inner.run(program, args, timeout)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProfilingDbusFacade {
    inner: ProductionDbusFacade,
    session: Arc<ProfileSession>,
}

impl ProfilingDbusFacade {
    pub(crate) fn new(inner: ProductionDbusFacade, session: Arc<ProfileSession>) -> Self {
        Self { inner, session }
    }
}

impl DbusFacade for ProfilingDbusFacade {
    fn call(&mut self, request: DbusRequest) -> Result<DbusOutput, BoundaryError> {
        self.session.record_dbus_call();
        self.inner.call(request)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ProfilingNotificationFacade {
    inner: ProductionNotificationFacade,
    session: Arc<ProfileSession>,
}

impl ProfilingNotificationFacade {
    pub(crate) fn new(inner: ProductionNotificationFacade, session: Arc<ProfileSession>) -> Self {
        Self { inner, session }
    }
}

impl NotificationFacade for ProfilingNotificationFacade {
    fn send(&mut self, payload: &NotificationPayload) -> Result<(), NotificationError> {
        self.session.record_dbus_call();
        self.inner.send(payload)
    }
}

impl ProfileSession {
    pub(crate) fn record_job_queued(&self, run: RunId, job: &JobId, queued_at: Duration) {
        self.attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(
                run,
                Attempt {
                    job: job_name(job),
                    capture_identity: capture_identity(job),
                    queued_at,
                    started_at: None,
                    finished_at: None,
                    captured_at: None,
                    disposition: None,
                },
            );
    }

    pub(crate) fn record_job_started(&self, run: RunId, started_at: Duration) {
        if let Some(attempt) = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(&run)
        {
            attempt.started_at = Some(started_at);
        }
    }

    pub(crate) fn record_job_finished(&self, run: RunId, finished_at: Duration) {
        if let Some(attempt) = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(&run)
        {
            attempt.finished_at = Some(finished_at);
        }
    }

    pub(crate) fn record_capture_candidate(&self, run: RunId, captured_at: Option<Duration>) {
        if let Some(attempt) = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get_mut(&run)
        {
            attempt.captured_at = captured_at;
        }
    }

    pub(crate) fn resolve_attempt(&self, run: RunId, disposition: AttemptDisposition) {
        let (capture_identity, captured_at) = {
            let mut attempts = self
                .attempts
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(attempt) = attempts.get_mut(&run) else {
                return;
            };
            attempt.disposition = Some(disposition);
            (attempt.capture_identity.clone(), attempt.captured_at)
        };
        let mut captures = self
            .accepted_captures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if matches!(disposition, AttemptDisposition::Accepted(_)) {
            if let Some(captured_at) = captured_at {
                captures.insert(capture_identity, captured_at);
            } else {
                captures.remove(&capture_identity);
            }
        }
    }

    pub(crate) fn record_process_call(&self) {
        self.process_calls.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn invalidate_capture(&self, job: &JobId) {
        self.accepted_captures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&capture_identity(job));
    }

    pub(crate) fn record_dbus_call(&self) {
        self.dbus_calls.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn record_wake(&self, due: Duration, at: Duration) {
        self.wake_lateness
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(at.saturating_sub(due));
    }

    pub(crate) fn enable_stimuli(&self, planned: usize) {
        let mut stimuli = self
            .stimuli
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        stimuli.enabled = true;
        stimuli.planned = planned;
    }

    pub(crate) fn record_stimulus_started(
        &self,
        id: StimulusId,
        kind: ProfileEventKind,
        expected: StimulusExpectation,
        at: Duration,
    ) {
        self.stimuli
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .records
            .insert(
                id,
                StimulusRecord {
                    kind,
                    expected,
                    started_at: at,
                    observed: false,
                    publication: None,
                    completed: false,
                    timed_out: false,
                },
            );
    }

    pub(crate) fn stimulus_completed(&self, id: StimulusId) -> bool {
        self.stimuli
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .records
            .get(&id)
            .is_some_and(|record| record.completed)
    }

    pub(crate) fn observe_presentation(&self, presented: bool, transition: &Transition) {
        self.observe_stimulus(
            |expected| {
                matches!(
                    expected,
                    StimulusExpectation::Presentation {
                        presented: expected,
                        ..
                    } if *expected == presented
                )
            },
            transition,
        );
    }

    pub(crate) fn observe_page(&self, index: usize, page: PageId, transition: &Transition) {
        self.observe_stimulus(
            |expected| {
                matches!(
                    expected,
                    StimulusExpectation::Page {
                        index: expected_index,
                        page: expected_page,
                    } if *expected_index == index && *expected_page == page
                )
            },
            transition,
        );
    }

    pub(crate) fn observe_config(
        &self,
        generation: ConfigGeneration,
        overlay: bool,
        transition: &Transition,
    ) {
        self.observe_stimulus(
            |expected| {
                matches!(
                    expected,
                    StimulusExpectation::Config {
                        generation: expected_generation,
                        overlay: expected_overlay,
                    } if *expected_generation == generation && *expected_overlay == overlay
                )
            },
            transition,
        );
    }

    fn observe_stimulus(
        &self,
        matches_expected: impl Fn(&StimulusExpectation) -> bool,
        transition: &Transition,
    ) {
        let publication = transition.actions.iter().find_map(|action| match action {
            SchedulerAction::PublishDisplay {
                publication,
                tooltip: true,
                ..
            } => Some(*publication),
            _ => None,
        });
        let mut stimuli = self
            .stimuli
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some((&id, record)) = stimuli
            .records
            .iter_mut()
            .find(|(_, record)| !record.completed && matches_expected(&record.expected))
        else {
            return;
        };
        record.observed = true;
        let needs_publication = !matches!(
            record.expected,
            StimulusExpectation::Presentation {
                html_publication: false,
                ..
            }
        );
        if needs_publication {
            if let Some(publication) = publication {
                record.publication = Some(publication);
                stimuli.publications.insert(publication, id);
            }
        } else {
            record.completed = true;
        }
    }

    pub(crate) fn time_out_stimuli(
        &self,
        pending: Option<StimulusId>,
        skipped: usize,
        requested_state: ProfileStimulusState,
        observed_state: ProfileStimulusState,
    ) {
        let mut stimuli = self
            .stimuli
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(record) = pending.and_then(|id| stimuli.records.get_mut(&id)) {
            record.timed_out = !record.completed;
        }
        stimuli.skipped = skipped;
        stimuli.requested_state = Some(requested_state);
        stimuli.observed_state = Some(observed_state);
    }

    pub(crate) fn record_shutdown_started(&self) {
        let mut shutdown = self
            .shutdown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        shutdown.started.get_or_insert_with(Instant::now);
    }

    pub(crate) fn record_shutdown_completed(&self) {
        let mut shutdown = self
            .shutdown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(started) = shutdown.started {
            shutdown.duration = Some(started.elapsed());
        }
    }

    pub(crate) fn shutdown_budget_remaining(&self, budget: Duration) -> Duration {
        self.shutdown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .started
            .map_or(budget, |started| budget.saturating_sub(started.elapsed()))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_publication(
        &self,
        publication: PublicationId,
        reason: PublishReason,
        tooltip: bool,
        boot: Duration,
        finished_at: Duration,
        render: Duration,
        display_deadline: Option<Duration>,
        skipped_display_deadlines: u64,
    ) {
        let oldest_age = self
            .accepted_captures
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .map(|captured| finished_at.saturating_sub(*captured))
            .max();
        let mut stats = self
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        stats.maximum_sample_age = stats.maximum_sample_age.max(oldest_age);
        stats.skipped_display_deadlines = stats
            .skipped_display_deadlines
            .saturating_add(skipped_display_deadlines);
        stats
            .render_by_reason
            .entry(reason_name(reason))
            .or_default()
            .push(render);
        if matches!(
            reason,
            PublishReason::FirstPaintReady | PublishReason::FirstPaintTimeout
        ) {
            let latency = finished_at.saturating_sub(boot);
            stats.first_paint_latency.push(latency);
            stats.first_paint_over_250ms = stats
                .first_paint_over_250ms
                .saturating_add(u64::from(latency > Duration::from_millis(250)));
        }
        if reason == PublishReason::DisplayDeadline
            && let Some(deadline) = display_deadline
        {
            let lateness = finished_at.saturating_sub(deadline);
            stats.scheduled_lateness.push(lateness);
            stats.over_50ms_publications = stats
                .over_50ms_publications
                .saturating_add(u64::from(lateness > Duration::from_millis(50)));
        }
        drop(stats);
        if tooltip {
            self.complete_stimulus_publication(publication, finished_at);
        }
    }

    fn complete_stimulus_publication(&self, publication: PublicationId, finished_at: Duration) {
        let mut stimuli = self
            .stimuli
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(id) = stimuli.publications.remove(&publication) else {
            return;
        };
        let Some(record) = stimuli.records.get_mut(&id) else {
            return;
        };
        let kind = record.kind;
        let latency = finished_at.saturating_sub(record.started_at);
        record.completed = true;
        drop(stimuli);
        let mut publications = self
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let event = publications.event_latency.entry(kind).or_default();
        event.completed.push(latency);
        event.over_100ms = event
            .over_100ms
            .saturating_add(u64::from(latency > Duration::from_millis(100)));
    }

    pub(crate) fn report(&self, duration: Duration, scenario: &str) -> String {
        let attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut jobs = BTreeMap::<String, JobStats>::new();
        for attempt in attempts.values() {
            let stats = jobs.entry(attempt.job.clone()).or_default();
            stats.attempts = stats.attempts.saturating_add(1);
            if let Some(started) = attempt.started_at {
                stats.queue.push(started.saturating_sub(attempt.queued_at));
                if let Some(finished) = attempt.finished_at {
                    stats.run.push(finished.saturating_sub(started));
                }
            }
            match attempt.disposition {
                Some(AttemptDisposition::Accepted(CompletionKind::Captured)) => {
                    stats.captured = stats.captured.saturating_add(1);
                }
                Some(AttemptDisposition::Accepted(CompletionKind::Baseline)) => {
                    stats.baseline = stats.baseline.saturating_add(1);
                }
                Some(AttemptDisposition::Accepted(CompletionKind::ConfirmedAbsent)) => {
                    stats.absent = stats.absent.saturating_add(1);
                }
                Some(AttemptDisposition::Accepted(CompletionKind::Failed)) => {
                    stats.failed = stats.failed.saturating_add(1);
                }
                Some(AttemptDisposition::Rejected) => {
                    stats.rejected = stats.rejected.saturating_add(1);
                }
                Some(AttemptDisposition::Cancelled) => {
                    stats.cancelled = stats.cancelled.saturating_add(1);
                }
                None => stats.in_flight = stats.in_flight.saturating_add(1),
            }
        }
        drop(attempts);

        let mut output = String::new();
        let _ = writeln!(output, "profile:");
        let _ = writeln!(output, "  duration_seconds: {:.3}", duration.as_secs_f64());
        let _ = writeln!(output, "  scenario: {scenario}");
        let stimuli_enabled = self
            .stimuli
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .enabled;
        let _ = writeln!(
            output,
            "  stimuli: {}",
            if stimuli_enabled {
                "enabled"
            } else {
                "disabled"
            }
        );
        if stimuli_enabled {
            let _ = writeln!(output, "  aggregate_scope: scenario plus profiling stimuli");
        }
        let _ = writeln!(
            output,
            "  process_calls: {}",
            self.process_calls.load(Ordering::Relaxed)
        );
        let _ = writeln!(
            output,
            "  dbus_calls: {}",
            self.dbus_calls.load(Ordering::Relaxed)
        );
        let _ = writeln!(output, "jobs:");
        if jobs.is_empty() {
            let _ = writeln!(output, "  unavailable: no attempts dispatched");
        }
        for (name, stats) in jobs {
            let _ = writeln!(
                output,
                "  {name}: attempts={} captured={} baseline={} absent={} failed={} rejected={} cancelled={} in_flight={} queue={} run={}",
                stats.attempts,
                stats.captured,
                stats.baseline,
                stats.absent,
                stats.failed,
                stats.rejected,
                stats.cancelled,
                stats.in_flight,
                percentiles(&stats.queue),
                percentiles(&stats.run)
            );
        }
        let _ = writeln!(output, "timing:");
        let wakes = self
            .wake_lateness
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = writeln!(output, "  wake_lateness: {}", percentiles(&wakes));
        let publications = self
            .publications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = writeln!(
            output,
            "  maximum_sample_age: {}",
            optional_duration(publications.maximum_sample_age)
        );
        let _ = writeln!(
            output,
            "  first_paint_latency: {}",
            percentiles(&publications.first_paint_latency)
        );
        let _ = writeln!(
            output,
            "  publication_lateness: {}",
            percentiles(&publications.scheduled_lateness)
        );
        let _ = writeln!(
            output,
            "  skipped_display_deadlines: {}",
            publications.skipped_display_deadlines
        );
        let _ = writeln!(
            output,
            "  first_paint_over_250ms: {}",
            publications.first_paint_over_250ms
        );
        let _ = writeln!(
            output,
            "  publication_over_50ms: {}",
            publications.over_50ms_publications
        );
        if stimuli_enabled {
            for (kind, name) in [
                (ProfileEventKind::Page, "page"),
                (ProfileEventKind::Control, "control"),
                (ProfileEventKind::Config, "config"),
            ] {
                let event = publications.event_latency.get(&kind);
                let timings = event.map_or(&[][..], |event| event.completed.as_slice());
                let over = event.map_or(0, |event| event.over_100ms);
                let _ = writeln!(
                    output,
                    "  event_{name}_latency: {} samples={} over_100ms={over}",
                    percentiles(timings),
                    timings.len()
                );
            }
        }
        for (reason, timings) in &publications.render_by_reason {
            let _ = writeln!(output, "  render_{reason}: {}", percentiles(timings));
        }
        let _ = writeln!(
            output,
            "  write_time: not_applicable (HTML runtime writes disabled)"
        );
        drop(publications);
        if stimuli_enabled {
            let stimuli = self
                .stimuli
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let completed = stimuli
                .records
                .values()
                .filter(|record| record.completed)
                .count();
            let timed_out = stimuli
                .records
                .values()
                .filter(|record| record.timed_out)
                .count();
            let _ = writeln!(
                output,
                "  stimulus_actions: planned={} started={} completed={} timed_out={} not_started={}",
                stimuli.planned,
                stimuli.records.len(),
                completed,
                timed_out,
                stimuli.skipped
            );
            for (id, record) in &stimuli.records {
                let _ = writeln!(
                    output,
                    "  stimulus_{}: kind={} expected={:?} observed={} publication={:?} completed={} timed_out={}",
                    id.0,
                    event_name(record.kind),
                    record.expected,
                    record.observed,
                    record.publication,
                    record.completed,
                    record.timed_out
                );
            }
            if let Some(requested_state) = &stimuli.requested_state {
                let _ = writeln!(output, "  stimulus_requested_state: {requested_state}");
            }
            if let Some(observed_state) = &stimuli.observed_state {
                let _ = writeln!(output, "  stimulus_observed_state: {observed_state}");
            }
        }
        let shutdown = self
            .shutdown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = writeln!(
            output,
            "  shutdown_duration: {}",
            optional_duration(shutdown.duration)
        );
        let _ = writeln!(
            output,
            "  shutdown_over_500ms: {}",
            u64::from(
                shutdown
                    .duration
                    .is_some_and(|duration| duration > Duration::from_millis(500))
            )
        );
        output
    }
}

const fn event_name(kind: ProfileEventKind) -> &'static str {
    match kind {
        ProfileEventKind::Page => "page",
        ProfileEventKind::Control => "control",
        ProfileEventKind::Config => "config",
    }
}

fn job_name(job: &JobId) -> String {
    format!("{:?}:{:?}:{:?}", job.owner, job.kind, job.source)
}

fn capture_identity(job: &JobId) -> JobId {
    if matches!(job.kind, JobKind::NvidiaNvml | JobKind::NvidiaFallback) {
        return JobId::with_source(job.owner, JobKind::NvidiaNvml, job.source.clone());
    }
    job.clone()
}

const fn reason_name(reason: PublishReason) -> &'static str {
    match reason {
        PublishReason::FirstPaintReady => "first_paint_ready",
        PublishReason::FirstPaintTimeout => "first_paint_timeout",
        PublishReason::DisplayDeadline => "display_deadline",
        PublishReason::TooltipActivated => "tooltip_activated",
        PublishReason::TooltipRefresh => "tooltip_refresh",
        PublishReason::PageChanged => "page_changed",
        PublishReason::ConfigChanged => "config_changed",
    }
}

fn percentiles(values: &[Duration]) -> String {
    if values.is_empty() {
        return String::from("unavailable");
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    format!(
        "p50={} p95={} p99={}",
        format_duration(percentile(&sorted, 50)),
        format_duration(percentile(&sorted, 95)),
        format_duration(percentile(&sorted, 99))
    )
}

fn percentile(sorted: &[Duration], percentile: usize) -> Duration {
    let rank = sorted.len().saturating_mul(percentile).div_ceil(100).max(1);
    sorted[rank.saturating_sub(1)]
}

fn optional_duration(duration: Option<Duration>) -> String {
    duration.map_or_else(|| String::from("unavailable"), format_duration)
}

fn format_duration(duration: Duration) -> String {
    format!("{:.3}ms", duration.as_secs_f64() * 1_000.0)
}

#[cfg(test)]
mod tests;
