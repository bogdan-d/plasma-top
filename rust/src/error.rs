//! Top-level application errors.

use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};

use crate::{cli::CliError, config::ConfigError};

/// Result alias used by the crate's public entry points.
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level errors returned by the application.
#[derive(Debug)]
pub enum Error {
    /// The caller provided an invalid command line.
    Cli(CliError),
    /// Configuration loading failed.
    Config(ConfigError),
    /// Filesystem or process operation failed.
    Runtime(String),
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cli(error) => write!(formatter, "{error}"),
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Runtime(detail) => write!(formatter, "{detail}"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Cli(error) => Some(error),
            Self::Config(error) => Some(error),
            Self::Runtime(_) => None,
        }
    }
}

impl From<CliError> for Error {
    fn from(value: CliError) -> Self {
        Self::Cli(value)
    }
}

impl From<ConfigError> for Error {
    fn from(error: ConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Runtime(error.to_string())
    }
}
