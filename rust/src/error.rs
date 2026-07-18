//! Error types for the Rust scaffold.

use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};

use crate::cli::CliError;

/// Result alias used by the crate's public entry points.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level errors returned by the scaffold crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The caller provided an invalid command line.
    Cli(CliError),
    /// The command is known but intentionally deferred to a later migration slice.
    ScaffoldOnly {
        /// The accepted top-level command name.
        command: &'static str,
    },
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cli(error) => write!(formatter, "{error}"),
            Self::ScaffoldOnly { command } => {
                write!(
                    formatter,
                    "phase 1 scaffold: command `{command}` is recognized but not implemented"
                )
            }
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Cli(error) => Some(error),
            Self::ScaffoldOnly { .. } => None,
        }
    }
}

impl From<CliError> for Error {
    fn from(value: CliError) -> Self {
        Self::Cli(value)
    }
}
