//! Applet requests applied to daemon state: page selection, presentation leases, and geometry.

use std::collections::{BTreeSet, VecDeque};
use std::path::Path;

use crate::adapters::ProductionClock;
use crate::config::{PanelGeometry, write_geom_cache};
use crate::domain::applet::{AppletEvent, AppletRequest, ClientId, InstanceId, PageStep};
use crate::domain::boundary::FilesystemRoots;
use crate::error::Result;
use crate::page_commands::Page;
use crate::scheduler::{OwnerId, Scheduler, SchedulerAction, SchedulerEvent};

use super::state::RuntimeState;
use super::worker::OwnerMessage;
use super::{DaemonPaths, critical_failure, enqueue, now, reload};

/// Component reported when the applet service stops delivering events.
pub(super) const APPLET_SERVICE: &str = "applet D-Bus service";

/// Presentation leases keyed by bus connection and applet instance.
#[derive(Debug, Default)]
pub(super) struct Leases {
    live: BTreeSet<(ClientId, InstanceId)>,
    gone: BTreeSet<ClientId>,
}

/// Effect of one lease operation on tooltip presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LeaseEffect {
    /// Aggregate presentation changed to the given value.
    Presented(bool),
    /// An existing lease was presented again.
    Reactivated,
    /// Nothing observable changed.
    Unchanged,
}

impl Leases {
    pub(super) fn presented(&self) -> bool {
        !self.live.is_empty()
    }

    pub(super) fn present(&mut self, client: ClientId, instance: InstanceId) -> LeaseEffect {
        // A request delayed behind its client's disconnect must not recreate a dead lease.
        if self.gone.contains(&client) {
            return LeaseEffect::Unchanged;
        }
        let was_presented = self.presented();
        if !self.live.insert((client, instance)) {
            return LeaseEffect::Reactivated;
        }
        if was_presented {
            LeaseEffect::Unchanged
        } else {
            LeaseEffect::Presented(true)
        }
    }

    pub(super) fn dismiss(&mut self, client: &ClientId, instance: InstanceId) -> LeaseEffect {
        let was_presented = self.presented();
        self.live.remove(&(client.clone(), instance));
        self.transition_from(was_presented)
    }

    pub(super) fn client_gone(&mut self, client: ClientId) -> LeaseEffect {
        let was_presented = self.presented();
        self.live.retain(|(owner, _)| *owner != client);
        self.gone.insert(client);
        self.transition_from(was_presented)
    }

    /// The daemon's own connection dropped: every client is gone, and unique names never recur on a new connection.
    pub(super) fn connection_lost(&mut self) -> LeaseEffect {
        let was_presented = self.presented();
        self.live.clear();
        self.gone.clear();
        self.transition_from(was_presented)
    }

    fn transition_from(&self, was_presented: bool) -> LeaseEffect {
        if was_presented == self.presented() {
            LeaseEffect::Unchanged
        } else {
            LeaseEffect::Presented(self.presented())
        }
    }
}

impl RuntimeState {
    /// Turns one page; returns whether the selection moved.
    pub(super) fn step_page(&mut self, step: PageStep) -> bool {
        let (current, count) = self.selection();
        let next = step.apply(current, count);
        self.page_index = usize::try_from(next).unwrap_or(0);
        next != current
    }

    /// Installs a new page registry, clamping the selection below its count.
    pub(super) fn replace_pages(&mut self, pages: Vec<Page>) {
        self.active = pages;
        self.page_index = self.page_index.min(self.active.len().saturating_sub(1));
    }

    /// Announces the selection when the page or the page count differs from the last announcement.
    pub(super) fn announce_selection(&mut self) {
        let selection = self.selection();
        if self.announced_selection != Some(selection) {
            self.announced_selection = Some(selection);
            self.sink.selection(selection.0, selection.1);
        }
    }

    fn selection(&self) -> (u32, u32) {
        let (index, _) = self.selected_page();
        (
            u32::try_from(index).unwrap_or(0),
            u32::try_from(self.active.len()).unwrap_or(u32::MAX),
        )
    }
}

/// Loop dependencies needed to apply one applet event.
pub(super) struct AppletContext<'a> {
    pub(super) config_path: Option<&'a Path>,
    pub(super) roots: &'a FilesystemRoots,
    pub(super) paths: &'a DaemonPaths,
    pub(super) cpu_count: usize,
    pub(super) clock: &'a ProductionClock,
}

/// Applies one applet event in receive order.
pub(super) fn handle_event(
    event: AppletEvent,
    context: &AppletContext<'_>,
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    actions: &mut VecDeque<SchedulerAction>,
    owner_messages: &mut VecDeque<(OwnerId, OwnerMessage)>,
) -> Result<()> {
    let effect = match event {
        AppletEvent::Request(AppletRequest::StepPage(step)) => {
            if state.step_page(step) {
                select_page(scheduler, state, context.clock, actions);
            }
            return Ok(());
        }
        AppletEvent::Request(AppletRequest::Geometry(geometry)) => {
            let measured = PanelGeometry::from(geometry);
            if state.geometry == Some(measured) {
                return Ok(());
            }
            state.geometry = Some(measured);
            if state.profile.is_none() {
                write_geom_cache(&context.paths.geom_cache, &measured);
            }
            reload::apply_config_change(
                context.config_path,
                context.roots,
                context.paths,
                context.cpu_count,
                scheduler,
                state,
                context.clock,
                actions,
                owner_messages,
            )?;
            return Ok(());
        }
        AppletEvent::Request(AppletRequest::Present { client, instance }) => {
            state.leases.present(client, instance)
        }
        AppletEvent::Request(AppletRequest::Dismiss { client, instance }) => {
            state.leases.dismiss(&client, instance)
        }
        AppletEvent::Request(AppletRequest::ClientGone { client }) => {
            state.leases.client_gone(client)
        }
        AppletEvent::ConnectionLost => state.leases.connection_lost(),
        AppletEvent::ServiceFailed(reason) => {
            eprintln!("[applet] {reason}");
            state.service_error = Some(reason);
            critical_failure(APPLET_SERVICE, scheduler, context.clock, actions);
            return Ok(());
        }
    };
    match effect {
        LeaseEffect::Presented(presented) => {
            let transition = scheduler.handle(SchedulerEvent::TooltipPresented {
                at: now(context.clock),
                presented,
            });
            if let Some(profile) = &state.profile {
                profile.observe_presentation(presented, &transition);
            }
            enqueue(actions, transition);
        }
        LeaseEffect::Reactivated => enqueue(
            actions,
            scheduler.handle(SchedulerEvent::TooltipReactivated {
                at: now(context.clock),
            }),
        ),
        LeaseEffect::Unchanged => {}
    }
    Ok(())
}

/// Tells the scheduler about the current selection and announces it to the applet.
pub(super) fn select_page(
    scheduler: &mut Scheduler,
    state: &mut RuntimeState,
    clock: &ProductionClock,
    actions: &mut VecDeque<SchedulerAction>,
) {
    let (index, page) = state.selected_page();
    let transition = scheduler.handle(SchedulerEvent::SelectedPageChanged {
        at: now(clock),
        page: page.clone(),
    });
    if let Some(profile) = &state.profile {
        profile.observe_page(index, page, &transition);
    }
    enqueue(actions, transition);
    state.announce_selection();
}
