#![deny(unsafe_code)]
#![warn(
    missing_docs,
    unreachable_pub,
    unused_lifetimes,
    unused_macro_rules,
    unused_qualifications
)]
#![deny(clippy::correctness, clippy::suspicious)]
#![warn(clippy::style, clippy::complexity, clippy::perf)]
#![deny(
    clippy::dbg_macro,
    clippy::todo,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::unimplemented
)]

//! Phase 1 scaffold for the Rust PiroStats migration.
//!
//! This crate intentionally exposes only frozen contracts and build shells.
//! Runtime collection, rendering, and daemon behavior land in later phases.

pub mod cli;
pub mod config;
pub mod domain;
pub mod error;
pub mod render;
pub mod runtime;

#[cfg(feature = "test-support")]
pub mod test_support;

use std::ffi::OsString;

pub use cli::{Cli, Command};
pub use error::{Error, Result};

/// Parses a command line and executes only the scaffold-level shell behavior.
///
/// Phase 1 intentionally stops after help/version output or after confirming
/// that a requested runtime command is recognized but not yet implemented.
///
/// # Errors
///
/// Returns [`Error::Cli`] when the command line is invalid and
/// [`Error::ScaffoldOnly`] for any runtime command that belongs to a later
/// migration slice.
pub fn run(args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let cli = Cli::parse(args)?;
    match cli.command {
        Command::Help => {
            println!("{}", cli::help_text());
            Ok(())
        }
        Command::Version => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        command => Err(Error::ScaffoldOnly {
            command: command.name(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_accepts_help_without_runtime() {
        let args = [OsString::from("pirostats"), OsString::from("--help")];

        let result = run(args);

        assert!(result.is_ok());
    }

    #[test]
    fn run_rejects_runtime_commands_until_later_phase() {
        let args = [OsString::from("pirostats"), OsString::from("daemon")];

        let result = run(args);

        assert!(matches!(
            result,
            Err(Error::ScaffoldOnly { command: "daemon" })
        ));
    }
}
