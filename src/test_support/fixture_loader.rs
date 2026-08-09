//! Fixture file loader and oracle-TOML decoder.
//!
//! [`FixtureLoader`] reads shared fixture files under a [`FixtureRoot`]:
//! raw text/bytes for proc/sys fixtures, and parsed TOML for oracle fixtures
//! that mirror the BASE schema in `tests/fixtures/oracle_render_full.toml`.
//!
//! [`OracleFixtureRaw`] intentionally keeps the retained Python oracle schema
//! as untyped `toml::Value` tables. Production uses typed snapshots; fixture
//! consumers can decode only the fields needed without coupling this loader to
//! every domain type.

use std::fmt::{self, Display, Formatter};
use std::io;
use std::path::PathBuf;

use toml::Value as TomlValue;

use super::fixture_root::FixtureRoot;

/// Error returned by [`FixtureLoader`] when a fixture cannot be read or
/// decoded.
///
/// Carries the requested relative path so callers can produce a useful
/// diagnostic without re-threading the path through every call site.
#[derive(Debug)]
pub enum FixtureError {
    /// The fixture file does not exist or cannot be read.
    Io {
        /// Relative path requested from the loader.
        relative: String,
        /// Underlying I/O error.
        source: io::Error,
    },
    /// The fixture file is not valid TOML.
    TomlParse {
        /// Relative path requested from the loader.
        relative: String,
        /// Underlying parse error.
        source: toml::de::Error,
    },
    /// The fixture TOML is missing a required top-level table.
    MissingTable {
        /// Relative path requested from the loader.
        relative: String,
        /// Expected table name (`hardware`, `readings`).
        table: &'static str,
    },
    /// The top-level TOML value is not a table.
    NotATable {
        /// Relative path requested from the loader.
        relative: String,
    },
}

impl Display for FixtureError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { relative, source } => {
                write!(formatter, "cannot read fixture `{relative}`: {source}")
            }
            Self::TomlParse { relative, source } => {
                write!(
                    formatter,
                    "cannot parse fixture `{relative}` as TOML: {source}"
                )
            }
            Self::MissingTable { relative, table } => {
                write!(
                    formatter,
                    "fixture `{relative}` is missing required `[{table}]` table",
                )
            }
            Self::NotATable { relative } => {
                write!(
                    formatter,
                    "fixture `{relative}` top-level TOML value is not a table",
                )
            }
        }
    }
}

impl std::error::Error for FixtureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::TomlParse { source, .. } => Some(source),
            Self::MissingTable { .. } | Self::NotATable { .. } => None,
        }
    }
}

/// Raw, untyped view of an oracle fixture file.
///
/// Mirrors the Python `OracleFixture` shape from `tests/oracle.py`. Consumers
/// build typed accessors over the raw [`toml::Value`] tables without changing
/// the shared on-disk schema.
#[derive(Debug, Clone, PartialEq)]
pub struct OracleFixtureRaw {
    /// Raw `[hardware]` table from the fixture.
    hardware: TomlValue,
    /// Raw `[readings]` table from the fixture.
    readings: TomlValue,
}

impl OracleFixtureRaw {
    /// Returns the raw `[hardware]` table.
    ///
    /// Consumers read typed fields off this value; the loader never interprets
    /// the contents.
    #[must_use]
    pub fn hardware(&self) -> &TomlValue {
        &self.hardware
    }

    /// Returns the raw `[readings]` table.
    ///
    /// Consumers read typed fields off this value; the loader never interprets
    /// the contents.
    #[must_use]
    pub fn readings(&self) -> &TomlValue {
        &self.readings
    }
}

/// Loader for fixture files shared between the Python oracle and Rust tests.
///
/// A loader is a thin wrapper around a [`FixtureRoot`] that adds:
///
/// - raw text/byte readers ([`load_text`](Self::load_text),
///   [`load_bytes`](Self::load_bytes)) for proc/sys fixtures that are plain
///   text;
/// - a TOML decoder ([`load_oracle_fixture`](Self::load_oracle_fixture)) for
///   oracle fixtures that mirror the BASE schema.
///
/// No method on this type touches the host filesystem outside `root`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FixtureLoader {
    /// Root directory for fixture files.
    pub root: FixtureRoot,
}

impl FixtureLoader {
    /// Creates a loader rooted at `root`.
    #[must_use]
    pub fn new(root: FixtureRoot) -> Self {
        Self { root }
    }

    /// Resolves `relative` against the loader's [`FixtureRoot`] and returns
    /// the joined path without touching the host.
    ///
    /// Tests that need to construct sibling paths (e.g. for `nftw`-style walks
    /// over `/proc/[pid]` fixtures) use this to keep their joins consistent
    /// with the loader.
    #[must_use]
    pub fn resolve(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    /// Reads a fixture file as raw UTF-8 text.
    ///
    /// `relative` is joined onto the loader's [`FixtureRoot`]; the host
    /// filesystem outside `root` is never touched.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] when the file cannot be read.
    pub fn load_text(&self, relative: &str) -> Result<String, io::Error> {
        std::fs::read_to_string(self.root.join(relative))
    }

    /// Reads a fixture file as raw bytes.
    ///
    /// `relative` is joined onto the loader's [`FixtureRoot`]; the host
    /// filesystem outside `root` is never touched.
    ///
    /// # Errors
    ///
    /// Returns the underlying [`io::Error`] when the file cannot be read.
    pub fn load_bytes(&self, relative: &str) -> Result<Vec<u8>, io::Error> {
        std::fs::read(self.root.join(relative))
    }

    /// Loads an oracle TOML fixture into an untyped [`OracleFixtureRaw`].
    ///
    /// `name` is resolved as `oracle/<name>.toml` under the loader's
    /// [`FixtureRoot`]. The fixture must contain top-level `[hardware]` and
    /// `[readings]` tables matching the BASE schema
    /// (`tests/fixtures/oracle_render_full.toml`); other top-level tables are
    /// ignored so future schema extensions do not break this loader.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError::Io`] when the file cannot be read,
    /// [`FixtureError::TomlParse`] when the TOML is malformed,
    /// [`FixtureError::NotATable`] when the top-level value is not a table,
    /// or [`FixtureError::MissingTable`] when `[hardware]` or `[readings]`
    /// is absent.
    pub fn load_oracle_fixture(&self, name: &str) -> Result<OracleFixtureRaw, FixtureError> {
        let relative = format!("oracle/{name}.toml");
        let path = self.root.join(&relative);
        let text = std::fs::read_to_string(&path).map_err(|source| FixtureError::Io {
            relative: relative.clone(),
            source,
        })?;
        let mut root =
            toml::from_str::<TomlValue>(&text).map_err(|source| FixtureError::TomlParse {
                relative: relative.clone(),
                source,
            })?;
        let table = root.as_table_mut().ok_or(FixtureError::NotATable {
            relative: relative.clone(),
        })?;
        let hardware = table.remove("hardware").ok_or(FixtureError::MissingTable {
            relative: relative.clone(),
            table: "hardware",
        })?;
        let readings = table.remove("readings").ok_or(FixtureError::MissingTable {
            relative,
            table: "readings",
        })?;
        Ok(OracleFixtureRaw { hardware, readings })
    }
}

#[cfg(test)]
mod tests;
