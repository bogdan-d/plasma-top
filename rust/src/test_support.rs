//! Test support: deterministic fakes and fixture loaders shared by
//! integration tests and downstream lanes.
//!
//! This module is gated behind the `test-support` cargo feature and excluded
//! from production builds. It provides the in-memory boundaries every later
//! sensor/adapter lane depends on:
//!
//! - [`FixtureRoot`] for virtual filesystem roots (proc/sys/run subtrees).
//! - [`FakeClock`] for deterministic clock advancement (rate/history/hysteresis).
//! - [`FakeCommandRunner`] / [`CommandRunner`] for argv-keyed command fakes.
//! - [`FakeDbus`] / [`DbusFacade`] for decoded D-Bus reply fakes.
//! - [`FixtureLoader`] / [`OracleFixtureRaw`] for reading the shared Python/
//!   Rust fixture tree (TOML oracle + raw text proc/sys files).
//!
//! All fakes are in-memory: no test ever touches `/proc`, `/sys`, the system
//! or session D-Bus, or spawns a child process. The [`RuntimeError`] type is
//! the local error contract for the fake boundaries; production adapter
//! errors live in [`crate::error`] (frozen by SCAFFOLD) and are kept separate
//! until the COLLECTOR/POWER/NOTIFY lanes promote variants via the
//! integration owner.

use std::ffi::OsString;
use std::fmt::{self, Display, Formatter};
use std::path::PathBuf;

pub mod fake_clock;
pub mod fake_command_runner;
pub mod fake_dbus;
pub mod fixture_loader;
pub mod fixture_root;

pub use crate::domain::boundary::{
    BusKind, ClockSnapshot, CommandOutput, CommandStatus, DbusOutput,
};
pub use fake_clock::FakeClock;
pub use fake_command_runner::{CommandRunner, FakeCommandRunner};
pub use fake_dbus::{DbusCall, DbusFacade, FakeDbus};
pub use fixture_loader::{FixtureError, FixtureLoader, OracleFixtureRaw};
pub use fixture_root::FixtureRoot;

/// Error returned by the test-support fake boundaries when a fake is asked to
/// dispatch a call for which no fixture reply was enqueued.
///
/// Lives in `test_support` rather than [`crate::error`] because `error::Error`
/// is a frozen shared contract owned by the SCAFFOLD lane and this type is
/// test-only. The COLLECTOR lane (Wave 5) will propose promoting these
/// variants (or replacing them with adapter-specific ones) into `error::Error`
/// via the integration owner — see the handoff proposal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// A fake command runner was asked to execute a command for which no
    /// reply was enqueued (or the per-argv queue was exhausted).
    CommandNotQueued {
        /// Program path the fake was asked to run.
        program: PathBuf,
        /// Argv values the fake was asked to run with.
        args: Vec<OsString>,
    },
    /// A fake D-Bus facade was asked to dispatch a call for which no reply
    /// was enqueued (or the per-signature queue was exhausted).
    DbusCallNotQueued {
        /// Which bus the fake was asked to call on.
        bus: BusKind,
        /// Remote service name.
        service: String,
        /// Object path.
        path: String,
        /// Interface name.
        interface: String,
        /// Method or signal member name.
        member: String,
    },
}

impl Display for RuntimeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::CommandNotQueued { program, args } => {
                write!(
                    formatter,
                    "no fixture reply queued for command `{}` with {} arg(s)",
                    program.display(),
                    args.len(),
                )
            }
            Self::DbusCallNotQueued {
                bus,
                service,
                path,
                interface,
                member,
            } => {
                let bus_label = match bus {
                    BusKind::Session => "session",
                    BusKind::System => "system",
                };
                write!(
                    formatter,
                    "no fixture reply queued for D-Bus {bus_label} call \
                     `{service}` `{path}` `{interface}` `{member}`",
                )
            }
        }
    }
}

impl std::error::Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_error_command_not_queued_displays_program_and_arg_count() {
        let err = RuntimeError::CommandNotQueued {
            program: PathBuf::from("/bin/false"),
            args: vec![OsString::from("--flag"), OsString::from("value")],
        };

        let msg = format!("{err}");
        assert!(msg.contains("/bin/false"), "message includes program path");
        assert!(msg.contains("2 arg"), "message includes arg count");
    }

    #[test]
    fn runtime_error_dbus_not_queued_displays_full_signature() {
        let err = RuntimeError::DbusCallNotQueued {
            bus: BusKind::System,
            service: "org.freedesktop.UPower".to_owned(),
            path: "/org/freedesktop/UPower".to_owned(),
            interface: "org.freedesktop.UPower".to_owned(),
            member: "EnumerateDevices".to_owned(),
        };

        let msg = format!("{err}");
        assert!(msg.contains("system"), "message includes bus label");
        assert!(msg.contains("org.freedesktop.UPower"));
        assert!(msg.contains("/org/freedesktop/UPower"));
        assert!(msg.contains("EnumerateDevices"));
    }

    #[test]
    fn re_exports_are_visible_at_module_root() {
        // Compile-time check that every documented re-export is reachable
        // from the crate root as `pirostats::test_support::*`. The actual
        // behavior is exercised by the submodule test suites; this just
        // guards against accidental visibility regressions.
        fn _check(
            _clock: FakeClock,
            _runner: FakeCommandRunner,
            _dbus: FakeDbus,
            _loader: FixtureLoader,
            _root: FixtureRoot,
        ) {
        }

        _check(
            FakeClock::default(),
            FakeCommandRunner::new(),
            FakeDbus::new(),
            FixtureLoader::default(),
            FixtureRoot::default(),
        );
    }
}
