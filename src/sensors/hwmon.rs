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
mod tests;
