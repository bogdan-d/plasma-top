//! Shared hwmon helpers for disk/fan discovery.
//!
//! The Python sensor module uses a small cluster of generic hwmon helpers to
//! locate sensor directories, resolve manual `chip|file` overrides, and parse
//! integer-valued sysfs files. Responsibilities stay in a
//! small sibling module so `disk.rs` stays focused and other sensors can reuse
//! the same deterministic helpers.

use std::fs;
use std::path::{Path, PathBuf};

/// Returns every hwmon directory whose `name` file contains `chip_substr`.
#[must_use]
pub(crate) fn hwmon_dirs_matching(sys_root: &Path, chip_substr: &str) -> Vec<PathBuf> {
    let mut matches = Vec::new();
    let root = sys_root.join("class/hwmon");
    let Ok(entries) = fs::read_dir(root) else {
        return matches;
    };
    let needle = chip_substr.to_ascii_lowercase();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(name) = fs::read_to_string(path.join("name")) else {
            continue;
        };
        if name.trim().to_ascii_lowercase().contains(&needle) {
            matches.push(path);
        }
    }
    matches.sort();
    matches
}

/// Resolves a manual `chip|file` override to a concrete hwmon path.
#[must_use]
pub(crate) fn resolve_sensor_spec(sys_root: &Path, spec: &str) -> Option<PathBuf> {
    let (chip, filename) = spec.split_once('|')?;
    for hwmon in hwmon_dirs_matching(sys_root, chip) {
        let path = hwmon.join(filename);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Reads a millidegree-Celsius sysfs file as whole degrees Celsius.
#[must_use]
pub(crate) fn read_path_millidegrees_celsius(path: Option<&Path>) -> Option<i32> {
    let path = path?;
    let text = fs::read_to_string(path).ok()?;
    let value = text.trim().parse::<i32>().ok()?;
    Some(value / 1000)
}

/// Reads a plain integer-valued sysfs file.
#[must_use]
pub(crate) fn read_path_int(path: Option<&Path>) -> Option<i32> {
    let path = path?;
    fs::read_to_string(path).ok()?.trim().parse::<i32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    struct TempTree {
        root: PathBuf,
    }

    impl TempTree {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::ZERO)
                .as_nanos();
            let root = std::env::temp_dir()
                .join(format!("pirostats-hwmon-{}-{unique}", std::process::id()));
            if let Err(error) = fs::create_dir_all(&root) {
                panic!("failed to create temp root {}: {error}", root.display());
            }
            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent()
                && let Err(error) = fs::create_dir_all(parent)
            {
                panic!("failed to create {}: {error}", parent.display());
            }
            if let Err(error) = fs::write(&path, content) {
                panic!("failed to write {}: {error}", path.display());
            }
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn hwmon_dirs_matching_filters_by_name_substring_case_insensitively() {
        let tmp = TempTree::new();
        tmp.write("sys/class/hwmon/hwmon0/name", "nvme\n");
        tmp.write("sys/class/hwmon/hwmon1/name", "DriveTemp\n");
        tmp.write("sys/class/hwmon/hwmon2/name", "coretemp\n");

        let matches = hwmon_dirs_matching(&tmp.path().join("sys"), "temp");

        assert_eq!(matches.len(), 2);
        assert!(matches[0].ends_with("hwmon1"));
        assert!(matches[1].ends_with("hwmon2"));
    }

    #[test]
    fn resolve_sensor_spec_requires_matching_chip_and_existing_file() {
        let tmp = TempTree::new();
        tmp.write("sys/class/hwmon/hwmon0/name", "nvme\n");
        tmp.write("sys/class/hwmon/hwmon0/temp1_input", "42000\n");

        assert_eq!(
            resolve_sensor_spec(&tmp.path().join("sys"), "nvme|temp1_input"),
            Some(tmp.path().join("sys/class/hwmon/hwmon0/temp1_input"))
        );
        assert_eq!(
            resolve_sensor_spec(&tmp.path().join("sys"), "nvme|temp2_input"),
            None
        );
        assert_eq!(resolve_sensor_spec(&tmp.path().join("sys"), "bogus"), None);
    }

    #[test]
    fn read_path_helpers_parse_or_return_none() {
        let tmp = TempTree::new();
        tmp.write("sys/class/hwmon/hwmon0/temp1_input", "43750\n");
        tmp.write("sys/class/hwmon/hwmon0/fan1_input", "1234\n");
        tmp.write("sys/class/hwmon/hwmon0/bad", "nope\n");

        let hwmon = tmp.path().join("sys/class/hwmon/hwmon0");
        assert_eq!(
            read_path_millidegrees_celsius(Some(&hwmon.join("temp1_input"))),
            Some(43)
        );
        assert_eq!(read_path_int(Some(&hwmon.join("fan1_input"))), Some(1234));
        assert_eq!(read_path_int(Some(&hwmon.join("bad"))), None);
        assert_eq!(read_path_int(None), None);
    }
}
