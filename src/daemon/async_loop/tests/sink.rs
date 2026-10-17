//! Recording publication sink and state constructors shared by the loop tests.

use std::sync::Arc;

use super::*;

/// One publication observed by [`RecordingSink`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Frame {
    Panel(String),
    Tooltip(String),
    Selection(u32, u32),
}

/// Test sink that records every publication in order.
#[derive(Debug, Clone, Default)]
pub(super) struct RecordingSink(Arc<std::sync::Mutex<Vec<Frame>>>);

impl RecordingSink {
    pub(super) fn frames(&self) -> Vec<Frame> {
        self.0.lock().expect("recorded frames").clone()
    }

    pub(super) fn last_panel(&self) -> Option<String> {
        self.frames()
            .into_iter()
            .rev()
            .find_map(|frame| match frame {
                Frame::Panel(html) => Some(html),
                _ => None,
            })
    }

    pub(super) fn last_tooltip(&self) -> Option<String> {
        self.frames()
            .into_iter()
            .rev()
            .find_map(|frame| match frame {
                Frame::Tooltip(html) => Some(html),
                _ => None,
            })
    }

    fn record(&self, frame: Frame) {
        self.0.lock().expect("recorded frames").push(frame);
    }
}

impl PublicationSink for RecordingSink {
    fn panel(&mut self, html: &str) {
        self.record(Frame::Panel(html.to_owned()));
    }

    fn tooltip(&mut self, html: &str) {
        self.record(Frame::Tooltip(html.to_owned()));
    }

    fn selection(&mut self, page: u32, count: u32) {
        self.record(Frame::Selection(page, count));
    }
}

pub(super) fn test_state(
    paths: &DaemonPaths,
    cfg: Config,
    hw: HardwareInventory,
    active: Vec<crate::page_commands::Page>,
) -> RuntimeState {
    recorded_state(paths, cfg, hw, active).0
}

pub(super) fn recorded_state(
    paths: &DaemonPaths,
    cfg: Config,
    hw: HardwareInventory,
    active: Vec<crate::page_commands::Page>,
) -> (RuntimeState, RecordingSink) {
    let sink = RecordingSink::default();
    (
        RuntimeState::new(paths, cfg, hw, active, None, Box::new(sink.clone())),
        sink,
    )
}
