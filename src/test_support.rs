//! Test support: deterministic fakes and fixture loaders shared by
//! integration tests.
//!
//! This module is gated behind the `test-support` cargo feature and excluded
//! from production builds. It provides the in-memory boundaries used by
//! sensor, adapter, formatter, and daemon tests:
//!
//! - [`FixtureRoot`] for virtual filesystem roots (proc/sys/run subtrees).
//! - [`FakeClock`] for deterministic clock advancement (rate/history/hysteresis).
//! - [`FakeCommandRunner`] / [`CommandRunner`] for argv-keyed command fakes.
//! - [`FakeDbus`] / [`DbusFacade`] for decoded D-Bus reply fakes.
//! - [`FixtureLoader`] / [`OracleFixtureRaw`] for reading the shared Python/
//!   Rust fixture tree (TOML oracle + raw text proc/sys files).
//!
//! All fakes are in-memory: no test ever touches `/proc`, `/sys`, the system
//! or session D-Bus, or spawns a child process. The fake adapters implement
//! the production [`crate::domain::boundary`] traits directly, so downstream
//! lanes can depend on one boundary contract in both production and tests.

pub mod fake_clock;
pub mod fake_command_runner;
pub mod fake_dbus;
pub mod fake_notification;
pub mod fixture_loader;
pub mod fixture_root;

pub use crate::domain::boundary::{
    BoundaryError, BusKind, ClockSnapshot, CommandOutput, CommandRunner, CommandStatus,
    DbusArgument, DbusFacade, DbusOutput, DbusRequest, NotificationError, NotificationFacade,
    NotificationPayload, NotificationTimeout, NotificationUrgency,
};
pub use fake_clock::FakeClock;
pub use fake_command_runner::{CommandCall, FakeCommandRunner};
pub use fake_dbus::{DbusCall, FakeDbus};
pub use fake_notification::FakeNotificationFacade;
pub use fixture_loader::{FixtureError, FixtureLoader, OracleFixtureRaw};
pub use fixture_root::FixtureRoot;

#[cfg(test)]
mod tests;
