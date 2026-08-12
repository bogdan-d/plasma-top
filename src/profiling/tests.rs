#![allow(clippy::expect_used)]

use super::*;
use crate::scheduler::{JobKind, OwnerId};

fn publish_transition(publication: PublicationId, reason: PublishReason) -> Transition {
    Transition {
        disposition: crate::scheduler::EventDisposition::Accepted,
        actions: vec![SchedulerAction::PublishDisplay {
            publication,
            reason,
            display_deadline: None,
            skipped_display_deadlines: 0,
            panel: false,
            tooltip: true,
        }],
    }
}

fn cpu_job() -> JobId {
    JobId::singleton(OwnerId::Cpu, JobKind::Cpu)
}

fn publication(
    session: &ProfileSession,
    reason: PublishReason,
    boot: Duration,
    finished_at: Duration,
    render: Duration,
    deadline: Option<Duration>,
    skipped: u64,
) {
    session.record_publication(
        PublicationId(99),
        reason,
        true,
        boot,
        finished_at,
        render,
        deadline,
        skipped,
    );
}

#[test]
fn nearest_rank_percentiles_cover_tail() {
    let values = (1..=100).map(Duration::from_millis).collect::<Vec<_>>();
    assert_eq!(percentile(&values, 50), Duration::from_millis(50));
    assert_eq!(percentile(&values, 95), Duration::from_millis(95));
    assert_eq!(percentile(&values, 99), Duration::from_millis(99));
}

#[test]
fn accepted_capture_age_survives_failure_and_rejected_completion() {
    let session = ProfileSession::default();
    let job = cpu_job();
    for (run, captured_at, disposition) in [
        (
            RunId(1),
            Some(Duration::from_millis(8)),
            AttemptDisposition::Accepted(CompletionKind::Captured),
        ),
        (
            RunId(2),
            Some(Duration::from_millis(8)),
            AttemptDisposition::Accepted(CompletionKind::Failed),
        ),
        (
            RunId(3),
            Some(Duration::from_millis(19)),
            AttemptDisposition::Rejected,
        ),
    ] {
        session.record_job_queued(run, &job, Duration::from_millis(run.0));
        session.record_job_started(run, Duration::from_millis(run.0 + 1));
        session.record_job_finished(run, Duration::from_millis(run.0 + 2));
        session.record_capture_candidate(run, captured_at);
        session.resolve_attempt(run, disposition);
    }
    publication(
        &session,
        PublishReason::FirstPaintReady,
        Duration::ZERO,
        Duration::from_millis(20),
        Duration::ZERO,
        None,
        0,
    );

    let report = session.report(Duration::from_secs(1), "main");

    assert!(report.contains("captured=1"));
    assert!(report.contains("failed=1"));
    assert!(report.contains("rejected=1"));
    assert!(report.contains("maximum_sample_age: 12.000ms"));

    let invalidated = ProfileSession::default();
    invalidated.record_job_queued(RunId(1), &job, Duration::ZERO);
    invalidated.record_capture_candidate(RunId(1), Some(Duration::from_millis(5)));
    invalidated.resolve_attempt(
        RunId(1),
        AttemptDisposition::Accepted(CompletionKind::Captured),
    );
    invalidated.invalidate_capture(&job);
    publication(
        &invalidated,
        PublishReason::FirstPaintReady,
        Duration::ZERO,
        Duration::from_millis(20),
        Duration::ZERO,
        None,
        0,
    );
    let invalidated_report = invalidated.report(Duration::from_secs(1), "main");
    assert!(invalidated_report.contains("maximum_sample_age: unavailable"));
}

#[test]
fn accepted_composite_baseline_and_partial_failure_update_retained_capture() {
    let session = ProfileSession::default();
    let job = cpu_job();
    for (run, captured_at, completion) in [
        (RunId(1), Duration::from_millis(4), CompletionKind::Baseline),
        (RunId(2), Duration::from_millis(11), CompletionKind::Failed),
    ] {
        session.record_job_queued(run, &job, Duration::ZERO);
        session.record_capture_candidate(run, Some(captured_at));
        session.resolve_attempt(run, AttemptDisposition::Accepted(completion));
    }
    publication(
        &session,
        PublishReason::FirstPaintReady,
        Duration::ZERO,
        Duration::from_millis(20),
        Duration::ZERO,
        None,
        0,
    );

    let report = session.report(Duration::from_secs(1), "main");

    assert!(report.contains("baseline=1"));
    assert!(report.contains("failed=1"));
    assert!(report.contains("maximum_sample_age: 9.000ms"));
}

#[test]
fn nvidia_backends_share_retained_capture_identity() {
    let session = ProfileSession::default();
    let nvml = JobId::singleton(OwnerId::Nvidia, JobKind::NvidiaNvml);
    let fallback = JobId::singleton(OwnerId::Nvidia, JobKind::NvidiaFallback);
    for (run, job, captured_at) in [
        (RunId(1), &nvml, Duration::from_millis(3)),
        (RunId(2), &fallback, Duration::from_millis(12)),
    ] {
        session.record_job_queued(run, job, Duration::ZERO);
        session.record_capture_candidate(run, Some(captured_at));
        session.resolve_attempt(run, AttemptDisposition::Accepted(CompletionKind::Captured));
    }
    session.record_job_queued(RunId(3), &fallback, Duration::ZERO);
    session.record_capture_candidate(RunId(3), Some(Duration::from_millis(12)));
    session.resolve_attempt(
        RunId(3),
        AttemptDisposition::Accepted(CompletionKind::ConfirmedAbsent),
    );
    publication(
        &session,
        PublishReason::FirstPaintReady,
        Duration::ZERO,
        Duration::from_millis(20),
        Duration::ZERO,
        None,
        0,
    );

    let report = session.report(Duration::from_secs(1), "graphs");

    assert!(report.contains("absent=1"));
    assert!(report.contains("maximum_sample_age: 8.000ms"));

    let invalidated = ProfileSession::default();
    invalidated.record_job_queued(RunId(1), &fallback, Duration::ZERO);
    invalidated.record_capture_candidate(RunId(1), Some(Duration::from_millis(12)));
    invalidated.resolve_attempt(
        RunId(1),
        AttemptDisposition::Accepted(CompletionKind::Captured),
    );
    invalidated.invalidate_capture(&nvml);
    publication(
        &invalidated,
        PublishReason::FirstPaintReady,
        Duration::ZERO,
        Duration::from_millis(21),
        Duration::ZERO,
        None,
        0,
    );
    let invalidated_report = invalidated.report(Duration::from_secs(1), "graphs");
    assert!(invalidated_report.contains("maximum_sample_age: unavailable"));
}

#[test]
fn publication_uses_render_completion_and_typed_deadline() {
    let session = ProfileSession::default();
    session.record_wake(Duration::from_millis(9), Duration::from_millis(10));
    publication(
        &session,
        PublishReason::FirstPaintReady,
        Duration::from_millis(1),
        Duration::from_millis(11),
        Duration::from_millis(2),
        None,
        0,
    );
    publication(
        &session,
        PublishReason::DisplayDeadline,
        Duration::from_millis(1),
        Duration::from_millis(18),
        Duration::from_millis(3),
        Some(Duration::from_millis(15)),
        2,
    );

    let report = session.report(Duration::from_secs(1), "main");

    assert!(report.contains("wake_lateness: p50=1.000ms"));
    assert!(report.contains("first_paint_latency: p50=10.000ms"));
    assert!(report.contains("publication_lateness: p50=3.000ms"));
    assert!(report.contains("skipped_display_deadlines: 2"));
    assert!(report.contains("render_first_paint_ready: p50=2.000ms"));
    assert!(report.contains("render_display_deadline: p50=3.000ms"));
}

#[test]
fn stimulus_latency_requires_the_correlated_in_memory_html_publication() {
    let session = ProfileSession::default();
    session.enable_stimuli(1);
    session.record_stimulus_started(
        StimulusId(1),
        ProfileEventKind::Page,
        StimulusExpectation::Page {
            index: 0,
            page: PageId::Main,
        },
        Duration::from_millis(5),
    );
    let transition = publish_transition(PublicationId(7), PublishReason::PageChanged);
    session.observe_page(0, PageId::Main, &transition);

    // An unrelated config/style publication has the same broad reason class but no stimulus id.
    session.record_publication(
        PublicationId(8),
        PublishReason::ConfigChanged,
        true,
        Duration::ZERO,
        Duration::from_millis(90),
        Duration::ZERO,
        None,
        0,
    );
    assert!(!session.stimulus_completed(StimulusId(1)));

    // Scheduler correlation alone is insufficient until tooltip HTML is rendered in memory.
    session.record_publication(
        PublicationId(7),
        PublishReason::PageChanged,
        false,
        Duration::ZERO,
        Duration::from_millis(110),
        Duration::ZERO,
        None,
        0,
    );
    assert!(!session.stimulus_completed(StimulusId(1)));
    session.record_publication(
        PublicationId(7),
        PublishReason::PageChanged,
        true,
        Duration::ZERO,
        Duration::from_millis(110),
        Duration::ZERO,
        None,
        0,
    );

    let report = session.report(Duration::from_secs(1), "main");

    assert!(report.contains(
        "event_page_latency: p50=105.000ms p95=105.000ms p99=105.000ms samples=1 over_100ms=1"
    ));
    assert!(report.contains("stimulus_1: kind=page"));
    assert!(report.contains("completed=true timed_out=false"));
}

#[test]
fn stimuli_wait_for_slow_publication_then_schedule_relative_to_completion() {
    let root = std::env::temp_dir().join(format!(
        "plasma-top-profile-stimulus-test-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let runtime = root.join("runtime");
    let state = runtime.join("state");
    std::fs::create_dir_all(state.join("presented")).expect("stimulus state");
    let paths = crate::daemon::DaemonPaths {
        panel: runtime.join("panel.html"),
        tooltip: runtime.join("tooltip.html"),
        page: state.join("page"),
        npages: state.join("npages"),
        geom: state.join("geom"),
        plasma_config: root.join("plasma-config"),
        kdeglobals: root.join("kdeglobals"),
        runtime,
        state,
    };
    let config_path = root.join("config.toml");
    let session = Arc::new(ProfileSession::default());
    let mut stimuli = ProfileStimuli::new(ProfileStimulusConfig {
        ready_at: Duration::ZERO,
        initially_presented: false,
        selected_page: 0,
        page_count: 2,
        pages: vec![PageId::Main, PageId::Graphs],
        config_path,
        config_original: b"[display]\noverlay = false\n".to_vec(),
        config_alternate: b"[display]\noverlay = true\n".to_vec(),
        config_generation: ConfigGeneration(1),
        overlay: false,
        session: Arc::clone(&session),
    });
    let clock = crate::adapters::ProductionClock::default();

    assert_eq!(
        stimuli.next_due(Duration::ZERO),
        Some(Duration::from_millis(100))
    );
    stimuli
        .inject_due(
            Duration::from_millis(100),
            &paths,
            &clock,
            ConfigGeneration(1),
            false,
        )
        .expect("inject presentation");
    assert_eq!(stimuli.next_due(Duration::from_secs(2)), None);

    let transition = publish_transition(PublicationId(1), PublishReason::TooltipActivated);
    session.observe_presentation(true, &transition);
    session.record_publication(
        PublicationId(1),
        PublishReason::TooltipActivated,
        true,
        Duration::ZERO,
        Duration::from_secs(2),
        Duration::from_secs(1),
        None,
        0,
    );
    assert_eq!(
        stimuli.next_due(Duration::from_secs(2)),
        Some(Duration::from_millis(2_100))
    );
    stimuli
        .inject_due(
            Duration::from_millis(2_099),
            &paths,
            &clock,
            ConfigGeneration(1),
            false,
        )
        .expect("action remains pending until relative due");
    assert!(!paths.page.exists());

    std::fs::remove_dir_all(root).expect("cleanup stimulus state");
}

#[test]
fn four_hundred_twenty_millisecond_stimulus_timeout_does_not_report_unobserved_page() {
    let session = ProfileSession::default();
    session.enable_stimuli(5);
    session.record_stimulus_started(
        StimulusId(3),
        ProfileEventKind::Page,
        StimulusExpectation::Page {
            index: 1,
            page: PageId::Graphs,
        },
        Duration::from_millis(410),
    );
    session.time_out_stimuli(
        Some(StimulusId(3)),
        2,
        ProfileStimulusState {
            presented: true,
            page_index: 1,
            page: PageId::Graphs,
            config_generation: ConfigGeneration(1),
            overlay: false,
        },
        ProfileStimulusState {
            presented: true,
            page_index: 0,
            page: PageId::Main,
            config_generation: ConfigGeneration(1),
            overlay: false,
        },
    );

    let report = session.report(Duration::from_millis(420), "main");
    assert!(report.contains("duration_seconds: 0.420"));
    assert!(report.contains("timed_out=1 not_started=2"));
    assert!(report.contains("stimulus_3: kind=page"));
    assert!(report.contains("observed=false publication=None completed=false timed_out=true"));
    assert!(report.contains("stimulus_requested_state: presented=true page_index=1 page=graphs"));
    assert!(report.contains("stimulus_observed_state: presented=true page_index=0 page=main"));
    assert!(!report.contains("stimulus_observed_state: presented=true page_index=1 page=graphs"));
}

#[test]
fn unresolved_and_cancelled_attempts_are_explicit() {
    let session = ProfileSession::default();
    let job = cpu_job();
    session.record_job_queued(RunId(1), &job, Duration::ZERO);
    session.record_job_queued(RunId(2), &job, Duration::ZERO);
    session.resolve_attempt(RunId(2), AttemptDisposition::Cancelled);

    let report = session.report(Duration::from_secs(1), "hidden");

    assert!(report.contains("cancelled=1 in_flight=1"));
    assert!(report.contains("write_time: not_applicable"));
}
