use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn publish(
    publication: PublicationId,
    reason: crate::scheduler::PublishReason,
    panel: bool,
    tooltip: bool,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
    control: &mut dyn LoopControl,
    boot: ClockSnapshot,
) -> Result<Option<Transition>> {
    let snapshot = control.snapshot();
    state.readings.assembled_at = snapshot;
    if matches!(
        reason,
        crate::scheduler::PublishReason::FirstPaintReady
            | crate::scheduler::PublishReason::FirstPaintTimeout
    ) {
        state.first_paint_published = true;
    }
    if panel {
        if reason == crate::scheduler::PublishReason::DisplayDeadline {
            state.display_publication_count = state.display_publication_count.saturating_add(1);
        }
        let width = PanelFormatter::new(&state.cfg, &state.hw).canonical_width(&state.readings);
        apply_canonical_width(&mut state.cfg, i32::try_from(width).unwrap_or(i32::MAX));
        let html = PanelFormatter::with_now_unix(&state.cfg, &state.hw, clock_unix(snapshot))
            .format_panel(&state.readings, &state.css);
        if state.panel_html.as_ref() != Some(&html) {
            write_atomic(&paths.panel, &html)?;
            state.panel_html = Some(html);
        }
        log_boot_ready(
            &state.readings,
            &mut state.boot_pending,
            boot.monotonic,
            snapshot.monotonic,
        );
        let acknowledgement = scheduler.handle(SchedulerEvent::PanelPublished {
            at: scheduler_time(control.snapshot()),
            publication,
        });
        publish_tooltip(state, roots, paths)?;
        return Ok(Some(acknowledgement));
    }
    if tooltip || panel {
        publish_tooltip(state, roots, paths)?;
    }
    Ok(None)
}

fn publish_tooltip(
    state: &mut RuntimeState,
    roots: &FilesystemRoots,
    paths: &DaemonPaths,
) -> Result<()> {
    let index = page_index(paths, state.active.len());
    let html = render_page_cached(
        &state.cfg,
        &state.hw,
        &state.readings,
        &state.css,
        &state.active,
        index,
        &state.command_cache,
        &roots.proc_root,
    );
    if state.tooltip_html.as_ref() != Some(&html) {
        write_atomic(&paths.tooltip, &html)?;
        state.tooltip_html = Some(html);
    }
    Ok(())
}
