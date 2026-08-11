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
        let root =
            std::env::temp_dir().join(format!("plasma-top-hwmon-{}-{unique}", std::process::id()));
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
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                panic!("failed to create {}: {error}", parent.display());
            }
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
