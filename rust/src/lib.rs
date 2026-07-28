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

//! Native PlasmaTop backend.

pub mod adapters;
pub mod cli;
pub mod config;
pub mod daemon;
pub mod diagnostics;
pub mod domain;
pub mod error;
pub mod notify;
pub mod page_commands;
pub mod render;
pub mod runtime;
pub mod sensors;

#[cfg(feature = "test-support")]
pub mod test_support;

use std::ffi::OsString;

pub use cli::{Cli, Command};
pub use error::{Error, Result};

/// Parses and executes one command.
///
/// Dispatch remains synchronous; feature modules own runtime behavior and
/// return contextual errors to this composition root.
///
/// # Errors
///
/// Returns a contextual [`Error`] on invalid input or runtime failure.
pub fn run(args: impl IntoIterator<Item = OsString>) -> Result<()> {
    let cli = Cli::parse(args)?;
    match cli.command {
        Command::Help => {
            println!("{}", cli::help_text());
            Ok(())
        }
        Command::HelpFor(command) => {
            println!("{}", cli::subcommand_help(command));
            Ok(())
        }
        Command::Version => {
            println!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Command::Daemon(command) => daemon::run_daemon(command.config.as_deref()),
        Command::Render(command) => diagnostics::run_render(&command),
        Command::Probe(command) => diagnostics::run_probe(command.config.as_deref()),
        Command::Profiling(command) => diagnostics::run_profiling(command.config.as_deref()),
        Command::ListItems => diagnostics::run_list_items(),
        Command::Page(command) => daemon::run_page(command.direction),
        Command::Click => daemon::run_click(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_accepts_help_without_runtime() {
        let args = [OsString::from("plasma-top"), OsString::from("--help")];

        let result = run(args);

        assert!(result.is_ok());
    }

    #[test]
    fn help_text_omits_internal_migration_terms() {
        assert!(!cli::help_text().contains("scaffold"));
    }
}
