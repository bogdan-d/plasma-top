//! Brightness and external status-file sample ownership.

use std::path::PathBuf;

use crate::config::Config;
use crate::domain::readings::{HardwareInventory, RetainedMetricSample};

/// Mutable samples and source identities for brightness and external status files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExternalState {
    pub(super) brightness: RetainedMetricSample<i32>,
    pub(super) updates: RetainedMetricSample<i32>,
    pub(super) updates_source: Option<PathBuf>,
    pub(super) server: RetainedMetricSample<bool>,
    pub(super) server_source: Option<PathBuf>,
}

impl ExternalState {
    pub(crate) fn reconcile_sources(
        &mut self,
        cfg: &Config,
        hw: &HardwareInventory,
        wants_updates: bool,
        wants_server: bool,
        wants_brightness: bool,
    ) {
        if wants_updates && !cfg.system_updates.file.is_empty() {
            let source = PathBuf::from(&cfg.system_updates.file);
            if self.updates_source.as_ref() != Some(&source) {
                self.updates.invalidate();
                self.updates_source = Some(source);
            }
        } else {
            self.updates.invalidate();
            self.updates_source = None;
        }

        if wants_server && !cfg.server_check.file.is_empty() {
            let source = PathBuf::from(&cfg.server_check.file);
            if self.server_source.as_ref() != Some(&source) {
                self.server.invalidate();
                self.server_source = Some(source);
            }
        } else {
            self.server.invalidate();
            self.server_source = None;
        }

        if !wants_brightness || !hw.has_backlight {
            self.brightness.invalidate();
        }
    }
}
