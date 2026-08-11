//! Production async I/O services and process-lifetime clock.

use std::time::{Instant, SystemTime};

use crate::domain::boundary::ClockSnapshot;

#[path = "adapters/command.rs"]
mod command;
#[path = "adapters/dbus.rs"]
mod dbus;
#[path = "adapters/shell.rs"]
mod shell;

pub use command::ProductionCommandRunner;
pub use dbus::{ProductionDbusFacade, ProductionNotificationFacade};
pub use shell::{ProductionIo, ProductionIoEvents};

/// Process-lifetime monotonic clock paired with wall time.
#[derive(Debug, Clone)]
pub struct ProductionClock {
    origin: Instant,
}

impl Default for ProductionClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl ProductionClock {
    /// Samples monotonic and wall clocks once.
    #[must_use]
    pub fn snapshot(&self) -> ClockSnapshot {
        ClockSnapshot {
            monotonic: self.origin.elapsed(),
            wall: SystemTime::now(),
        }
    }
}

#[cfg(test)]
#[path = "adapters/tests.rs"]
mod tests;
