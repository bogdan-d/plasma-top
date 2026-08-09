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
    dir.push(format!("plasma-top-fixture-loader-{nonce}"));
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
