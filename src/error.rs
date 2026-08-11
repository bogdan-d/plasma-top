//! Top-level application errors.

use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};

use crate::{cli::CliError, config::ConfigError};

/// Result alias used by the crate's public entry points.
pub type Result<T> = std::result::Result<T, Error>;

/// A process-lifetime async service that exited unexpectedly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriticalService {
    /// External-command service.
    Command,
    /// System D-Bus service.
    SystemDbus,
    /// Session notification D-Bus service.
    SessionDbus,
}

impl Display for CriticalService {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command => formatter.write_str("command service"),
            Self::SystemDbus => formatter.write_str("system D-Bus service"),
            Self::SessionDbus => formatter.write_str("session D-Bus service"),
        }
    }
}

/// Top-level errors returned by the application.
#[derive(Debug)]
pub enum Error {
    /// The caller provided an invalid command line.
    Cli(CliError),
    /// Configuration loading failed.
    Config(ConfigError),
    /// Filesystem or process operation failed.
    Runtime(String),
    /// A critical async service exited and requires daemon restart.
    CriticalService(CriticalService),
    /// The async I/O shell did not stop within its bounded shutdown window.
    CriticalShutdownTimeout,
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cli(error) => write!(formatter, "{error}"),
            Self::Config(error) => write!(formatter, "{error}"),
            Self::Runtime(detail) => write!(formatter, "{detail}"),
            Self::CriticalService(service) => {
                write!(formatter, "critical {service} exited unexpectedly")
            }
            Self::CriticalShutdownTimeout => {
                formatter.write_str("critical async I/O shutdown timed out")
            }
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Cli(error) => Some(error),
            Self::Config(error) => Some(error),
            Self::Runtime(_) | Self::CriticalService(_) | Self::CriticalShutdownTimeout => None,
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
