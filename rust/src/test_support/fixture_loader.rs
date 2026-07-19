//! Fixture file loader and oracle-TOML decoder.
//!
//! [`FixtureLoader`] reads shared fixture files under a [`FixtureRoot`]:
//! raw text/bytes for proc/sys fixtures, and parsed TOML for oracle fixtures
//! that mirror the BASE schema in `tests/fixtures/oracle_render_full.toml`.
//!
//! The Rust side currently defers typed deserialization of the oracle schema
//! until the `HardwareInfo`/`Readings` types land in Wave 3/4
//! (DOMAIN/CONFIG lanes). Until then, [`OracleFixtureRaw`] hands downstream
//! lanes the untyped `toml::Value` tables with stable accessors so they can
//! extend the typed view without breaking the on-disk schema or this lane's
//! API.

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
/// Mirrors the Python `OracleFixture` shape from `tests/oracle.py` but defers
/// typed deserialization until the Rust `HardwareInfo`/`Readings` types land
/// (Wave 3/4 DOMAIN/CONFIG). Downstream lanes extend this with typed
/// accessors built on top of the raw [`toml::Value`] tables, without changing
/// the on-disk schema or this lane's surface.
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
    /// Downstream lanes read typed fields off this value; this lane never
    /// interprets the contents.
    #[must_use]
    pub fn hardware(&self) -> &TomlValue {
        &self.hardware
    }

    /// Returns the raw `[readings]` table.
    ///
    /// Downstream lanes read typed fields off this value; this lane never
    /// interprets the contents.
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
mod tests {
    use super::*;
    use crate::test_support::FixtureRoot;

    fn env_loader() -> FixtureLoader {
        FixtureLoader::new(FixtureRoot::from_env())
    }

    /// Like `.expect(msg)` but produces a panic directly so the crate-wide
    /// `clippy::expect_used` / `clippy::unwrap_used` denial stays green.
    macro_rules! must {
        ($expr:expr, $msg:expr) => {
            match $expr {
                Ok(value) => value,
                Err(error) => panic!("{}: {error}", $msg),
            }
        };
    }

    #[test]
    fn load_text_reads_proc_stat_fixture() {
        let loader = env_loader();
        let text = must!(
            loader.load_text("proc/stat"),
            "proc/stat fixture must be readable"
        );

        assert!(
            text.starts_with("cpu  "),
            "proc/stat starts with the cpu aggregate: {}",
            text
        );
        assert!(text.contains("btime"));
    }

    #[test]
    fn load_text_returns_io_error_for_missing_file() {
        let loader = env_loader();

        let err = match loader.load_text("does/not/exist") {
            Ok(text) => panic!("expected error, got {text:?}"),
            Err(error) => error,
        };
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn load_bytes_reads_sysfs_fixture() {
        let loader = env_loader();
        let bytes = must!(
            loader.load_bytes("sys/class/hwmon/hwmon0/temp1_input"),
            "sysfs fixture must be readable"
        );

        // The fixture stores millidegrees Celsius as ASCII digits.
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => panic!("fixture is UTF-8: {error}"),
        };
        let parsed: i64 = match text.trim().parse() {
            Ok(value) => value,
            Err(error) => panic!("fixture is an integer: {error}"),
        };
        assert!(parsed > 0);
    }

    #[test]
    fn load_bytes_returns_io_error_for_missing_file() {
        let loader = env_loader();

        let err = match loader.load_bytes("does/not/exist.bin") {
            Ok(bytes) => panic!("expected error, got {bytes:?}"),
            Err(error) => error,
        };
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn load_oracle_fixture_parses_sample_fixture() {
        let loader = env_loader();
        let fixture = must!(
            loader.load_oracle_fixture("oracle_render_full"),
            "sample oracle fixture must load"
        );

        let Some(hardware) = fixture.hardware().as_table() else {
            panic!("hardware is a table");
        };
        let Some(readings) = fixture.readings().as_table() else {
            panic!("readings is a table");
        };

        assert_eq!(
            hardware.get("cpu_count").and_then(TomlValue::as_integer),
            Some(8)
        );
        assert_eq!(
            readings.get("cpu_usage").and_then(TomlValue::as_integer),
            Some(73)
        );
        assert!(hardware.get("hd_temp_paths").is_some());
        assert!(readings.get("disk_usage").is_some());
    }

    #[test]
    fn load_oracle_fixture_returns_io_error_for_missing_fixture() {
        let loader = env_loader();

        let err = match loader.load_oracle_fixture("nonexistent") {
            Ok(fixture) => panic!("expected error, got {fixture:?}"),
            Err(error) => error,
        };
        match err {
            FixtureError::Io { relative, source } => {
                assert_eq!(relative, "oracle/nonexistent.toml");
                assert_eq!(source.kind(), io::ErrorKind::NotFound);
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn load_oracle_fixture_reports_missing_table() {
        let tmp = tempdir_for_loader_tests();
        let loader = FixtureLoader::new(FixtureRoot::new(tmp.join("root")));
        write_file(
            tmp.join("root").join("oracle").join("partial.toml"),
            "[hardware]\ncpu_count = 1\n",
        );

        let err = match loader.load_oracle_fixture("partial") {
            Ok(fixture) => panic!("expected error, got {fixture:?}"),
            Err(error) => error,
        };
        match err {
            FixtureError::MissingTable { relative, table } => {
                assert_eq!(relative, "oracle/partial.toml");
                assert_eq!(table, "readings");
            }
            other => panic!("unexpected error variant: {other:?}"),
        }
    }

    #[test]
    fn load_oracle_fixture_reports_toml_parse_error() {
        let tmp = tempdir_for_loader_tests();
        let loader = FixtureLoader::new(FixtureRoot::new(tmp.join("root")));
        write_file(
            tmp.join("root").join("oracle").join("bad.toml"),
            "this is = not = valid\n",
        );

        let err = match loader.load_oracle_fixture("bad") {
            Ok(fixture) => panic!("expected error, got {fixture:?}"),
            Err(error) => error,
        };
        assert!(matches!(err, FixtureError::TomlParse { .. }));
    }

    #[test]
    fn resolve_joins_relative_onto_root() {
        let loader = FixtureLoader::new(FixtureRoot::new(PathBuf::from("/tmp/example")));

        assert_eq!(
            loader.resolve("proc/stat"),
            PathBuf::from("/tmp/example/proc/stat"),
        );
    }

    /// Creates a unique tempdir under the system temp location.
    ///
    /// We do not use the `tempfile` crate (kept out of the production dep set
    /// deliberately); a process-unique name plus manual cleanup is enough for
    /// the loader's error-path tests.
    fn tempdir_for_loader_tests() -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let nonce = std::process::id() ^ nanos;
        dir.push(format!("pirostats-fixture-loader-{nonce}"));
        if let Err(error) = std::fs::create_dir_all(&dir) {
            panic!("tempdir creation failed for {}: {error}", dir.display());
        }
        dir
    }

    /// Writes `contents` to `path`, creating parent directories as needed.
    ///
    /// Panics on I/O failure — these are tests exercising the loader, not I/O
    /// robustness, so a panic on tempdir setup is the right level of noise.
    fn write_file(path: PathBuf, contents: &str) {
        if let Some(parent) = path.parent() {
            if let Err(error) = std::fs::create_dir_all(parent) {
                panic!("failed to create parent dir {}: {error}", parent.display());
            }
        }
        std::fs::write(&path, contents)
            .unwrap_or_else(|error| panic!("failed to write fixture {}: {error}", path.display()));
    }
}
