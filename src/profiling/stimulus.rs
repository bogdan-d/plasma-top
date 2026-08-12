use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use crate::adapters::ProductionClock;
use crate::daemon::DaemonPaths;
use crate::error::Result;
use crate::runtime::atomic::write_atomic;
use crate::scheduler::{ConfigGeneration, PageId};

use super::{
    ProfileEventKind, ProfileSession, ProfileStimulusState, StimulusExpectation, StimulusId,
};

const FIRST_DELAY: Duration = Duration::from_millis(100);
const NEXT_DELAY: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy)]
enum Action {
    Dismiss,
    Present,
    Page(usize),
    Config(bool),
}

/// Serial, bounded external protocol mutations used only by opt-in timed profiling.
pub(crate) struct ProfileStimuli {
    actions: Vec<Action>,
    next: usize,
    due: Duration,
    pending: Option<StimulusId>,
    config_path: PathBuf,
    config_original: Vec<u8>,
    config_alternate: Vec<u8>,
    pages: Vec<PageId>,
    requested_state: ProfileStimulusState,
    session: Arc<ProfileSession>,
}

pub(crate) struct ProfileStimulusConfig {
    pub(crate) ready_at: Duration,
    pub(crate) initially_presented: bool,
    pub(crate) selected_page: usize,
    pub(crate) page_count: usize,
    pub(crate) pages: Vec<PageId>,
    pub(crate) config_generation: ConfigGeneration,
    pub(crate) overlay: bool,
    pub(crate) config_path: PathBuf,
    pub(crate) config_original: Vec<u8>,
    pub(crate) config_alternate: Vec<u8>,
    pub(crate) session: Arc<ProfileSession>,
}

impl ProfileStimuli {
    pub(crate) fn new(config: ProfileStimulusConfig) -> Self {
        let mut measured = Vec::new();
        if config.page_count > 1 {
            measured.push(Action::Page((config.selected_page + 1) % config.page_count));
            measured.push(Action::Page(config.selected_page));
        }
        measured.extend([Action::Config(true), Action::Config(false)]);
        let actions = if config.initially_presented {
            let mut actions = vec![Action::Dismiss, Action::Present];
            actions.extend(measured);
            actions
        } else {
            let mut actions = vec![Action::Present];
            actions.extend(measured);
            actions.push(Action::Dismiss);
            actions
        };
        config.session.enable_stimuli(actions.len());
        let requested_state = ProfileStimulusState {
            presented: config.initially_presented,
            page_index: config.selected_page,
            page: config.pages[config.selected_page].clone(),
            config_generation: config.config_generation,
            overlay: config.overlay,
        };
        Self {
            actions,
            next: 0,
            due: config.ready_at.saturating_add(FIRST_DELAY),
            pending: None,
            config_path: config.config_path,
            config_original: config.config_original,
            config_alternate: config.config_alternate,
            pages: config.pages,
            requested_state,
            session: config.session,
        }
    }

    pub(crate) fn next_due(&mut self, now: Duration) -> Option<Duration> {
        if let Some(id) = self.pending {
            if !self.session.stimulus_completed(id) {
                return None;
            }
            self.pending = None;
            self.next = self.next.saturating_add(1);
            self.due = now.saturating_add(NEXT_DELAY);
        }
        self.actions.get(self.next).map(|_| self.due)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn inject_due(
        &mut self,
        now: Duration,
        paths: &DaemonPaths,
        clock: &ProductionClock,
        config_generation: ConfigGeneration,
        overlay: bool,
    ) -> Result<()> {
        if self.pending.is_some() || self.due > now {
            return Ok(());
        }
        let Some(&action) = self.actions.get(self.next) else {
            return Ok(());
        };
        let sequence = u64::try_from(self.next)
            .unwrap_or(u64::MAX)
            .saturating_add(1);
        let id = StimulusId(sequence);
        let expectation = match action {
            Action::Dismiss => {
                remove_if_present(&lease_path(paths))?;
                self.requested_state.presented = false;
                StimulusExpectation::Presentation {
                    presented: false,
                    html_publication: false,
                }
            }
            Action::Present => {
                write_atomic(&lease_path(paths), b"")?;
                self.requested_state.presented = true;
                StimulusExpectation::Presentation {
                    presented: true,
                    html_publication: true,
                }
            }
            Action::Page(page) => {
                write_atomic(&paths.page, page.to_string().as_bytes())?;
                self.requested_state.page_index = page;
                self.requested_state.page = self.pages[page].clone();
                StimulusExpectation::Page {
                    index: page,
                    page: self.pages[page].clone(),
                }
            }
            Action::Config(alternate) => {
                let contents = if alternate {
                    &self.config_alternate
                } else {
                    &self.config_original
                };
                write_atomic(&self.config_path, contents)?;
                self.requested_state.config_generation = config_generation.next();
                self.requested_state.overlay = !overlay;
                StimulusExpectation::Config {
                    generation: config_generation.next(),
                    overlay: !overlay,
                }
            }
        };
        self.session.record_stimulus_started(
            id,
            event_kind(action),
            expectation,
            clock.snapshot().monotonic,
        );
        self.pending = Some(id);
        Ok(())
    }

    pub(crate) fn time_out(&self, observed_state: ProfileStimulusState) {
        self.session.time_out_stimuli(
            self.pending,
            self.actions.len().saturating_sub(
                self.next
                    .saturating_add(usize::from(self.pending.is_some())),
            ),
            self.requested_state.clone(),
            observed_state,
        );
    }
}

const fn event_kind(action: Action) -> ProfileEventKind {
    match action {
        Action::Dismiss | Action::Present => ProfileEventKind::Control,
        Action::Page(_) => ProfileEventKind::Page,
        Action::Config(_) => ProfileEventKind::Config,
    }
}

fn lease_path(paths: &DaemonPaths) -> PathBuf {
    paths.state.join("presented").join("1")
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
