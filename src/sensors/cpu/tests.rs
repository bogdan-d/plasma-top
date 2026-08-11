use super::*;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn fixture_proc() -> PathBuf {
    fixtures_root().join("proc")
}

fn fixture_sys() -> PathBuf {
    fixtures_root().join("sys")
}

fn clock_at(seconds: u64) -> ClockSnapshot {
    ClockSnapshot {
        monotonic: Duration::from_secs(seconds),
        wall: UNIX_EPOCH + Duration::from_secs(seconds),
    }
}

fn config_with_history_interval(seconds: f64) -> Config {
    let mut cfg = Config::default();
    cfg.display.history_interval =
        crate::domain::Cadence::from_millis(Duration::from_secs_f64(seconds).as_millis() as u64);
    cfg
}

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
            std::env::temp_dir().join(format!("plasma-top-cpu-{}-{unique}", std::process::id()));
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
fn discover_cpu_paths_collects_all_cpu_discovery_outputs() {
    let tmp = TempTree::new();
    tmp.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
    tmp.write("sys/class/hwmon/hwmon0/temp1_input", "55000\n");
    tmp.write(
        "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
        "3200000\n",
    );
    tmp.write("sys/devices/system/cpu/intel_pstate/no_turbo", "0\n");

    let paths = discover_cpu_paths(&tmp.path().join("sys"), &SensorOverrides::default());

    assert_eq!(
        paths.cpu_temp_path,
        Some(tmp.path().join("sys/class/hwmon/hwmon0/temp1_input"))
    );
    assert_eq!(
        paths.cpu_freq_path,
        Some(
            tmp.path()
                .join("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
        )
    );
    assert!(paths.cpu_turbo_supported);
    assert_eq!(
        paths.cpu_turbo_path,
        Some(
            tmp.path()
                .join("sys/devices/system/cpu/intel_pstate/no_turbo")
        )
    );
}

#[test]
fn find_cpu_temp_path_prefers_override_before_autodetect() {
    let tmp = TempTree::new();
    tmp.write("sys/class/hwmon/hwmon0/name", "coretemp\n");
    tmp.write("sys/class/hwmon/hwmon0/temp1_input", "55000\n");
    tmp.write("sys/class/hwmon/hwmon1/name", "zenpower\n");
    tmp.write("sys/class/hwmon/hwmon1/temp3_input", "44000\n");

    let overrides = SensorOverrides {
        cpu_temp: Some(String::from("zenpower|temp3_input")),
        ..SensorOverrides::default()
    };

    let found = find_cpu_temp_path(&tmp.path().join("sys"), &overrides);

    assert_eq!(
        found,
        Some(tmp.path().join("sys/class/hwmon/hwmon1/temp3_input"))
    );
}

#[test]
fn find_cpu_temp_path_autodetects_supported_fixture_chip() {
    let found = find_cpu_temp_path(&fixture_sys(), &SensorOverrides::default());

    assert_eq!(
        found,
        Some(fixture_sys().join("class/hwmon/hwmon0/temp1_input"))
    );
}

#[test]
fn find_cpu_freq_path_and_turbo_support_follow_sysfs_presence() {
    let tmp = TempTree::new();
    assert_eq!(find_cpu_freq_path(&tmp.path().join("sys")), None);
    assert!(!detect_cpu_turbo_supported(&tmp.path().join("sys")));

    tmp.write(
        "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
        "3200000\n",
    );
    tmp.write("sys/devices/system/cpu/cpufreq/boost", "1\n");

    assert_eq!(
        find_cpu_freq_path(&tmp.path().join("sys")),
        Some(
            tmp.path()
                .join("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
        )
    );
    assert!(detect_cpu_turbo_supported(&tmp.path().join("sys")));
}

#[test]
fn read_cpu_usage_first_sample_is_baseline_without_history() {
    let mut state = CpuState::default();
    let cfg = Config::default();

    let usage = read_cpu_usage(&fixture_proc(), &mut state, &cfg, clock_at(0));

    assert_eq!(usage, None);
    assert!(state.cpu_history.is_empty());
    assert_eq!(state.cpu_prev_times.len(), 10);
}

#[test]
fn read_cpu_usage_computes_delta_caps_at_ninety_nine_and_trims_history() {
    let tmp = TempTree::new();
    tmp.write("proc/stat", "cpu 10 0 10 80 0 0 0 0 0 0\n");

    let mut cfg = config_with_history_interval(1.0);
    cfg.spark_panel.cpu_spark_length = 1;
    cfg.spark_tooltip.cpu_spark_length = 1;
    cfg.braille_panel.cpu_braille_length = 1;
    cfg.braille_tooltip.cpu_braille_length = 1;
    cfg.pages.order = vec![String::from("graphs")];
    cfg.pages.graph_history_length = 2;

    let proc_root = tmp.path().join("proc");
    let mut state = CpuState::default();
    assert_eq!(
        read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(0)),
        None
    );

    tmp.write("proc/stat", "cpu 50 0 50 80 0 0 0 0 0 0\n");
    assert_eq!(
        read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(2)),
        Some(99)
    );

    tmp.write("proc/stat", "cpu 51 0 51 80 0 0 0 0 0 0\n");
    assert_eq!(
        read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(4)),
        Some(99)
    );
    assert_eq!(state.cpu_history, vec![99, 99]);
}

#[test]
fn read_cpu_usage_invalid_delta_does_not_append_history() {
    let tmp = TempTree::new();
    tmp.write("proc/stat", "cpu 10 0 10 80 0 0 0 0 0 0\n");

    let cfg = config_with_history_interval(5.0);
    let proc_root = tmp.path().join("proc");
    let mut state = CpuState::default();
    assert_eq!(
        read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(0)),
        None
    );

    tmp.write("proc/stat", "cpu 30 0 20 90 0 0 0 0 0 0\n");
    assert_eq!(
        read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(1)),
        Some(75)
    );
    assert_eq!(state.cpu_history, vec![75]);

    assert_eq!(
        read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(2)),
        None
    );
    assert_eq!(state.cpu_history, vec![75]);

    tmp.write("proc/stat", "cpu 1 0 1 8 0 0 0 0 0 0\n");
    assert_eq!(
        read_cpu_usage(&proc_root, &mut state, &cfg, clock_at(6)),
        None
    );
    assert_eq!(state.cpu_history, vec![75]);
}

#[test]
fn read_cpu_cores_first_read_is_baseline_without_history() {
    let mut cfg = Config::default();
    cfg.display.history_interval = crate::domain::Cadence::from_millis(1000);
    cfg.braille_tooltip.cpu_braille_length = 2;
    cfg.display.tooltip_width = 3;

    let mut state = CpuState::default();
    let first = read_cpu_cores(&fixture_proc(), &mut state, &cfg, clock_at(0));

    assert_eq!(first, None);
    assert_eq!(state.cpu_core_history.len(), 8);
    assert!(state.cpu_core_history.iter().all(Vec::is_empty));
}

#[test]
fn read_cpu_cores_resets_on_core_count_change() {
    let tmp = TempTree::new();
    tmp.write(
        "proc/stat",
        "cpu 20 0 20 160 0 0 0 0 0 0\n\
         cpu0 10 0 10 80 0 0 0 0 0 0\n\
         cpu1 10 0 10 80 0 0 0 0 0 0\n",
    );

    let cfg = config_with_history_interval(1.0);
    let proc_root = tmp.path().join("proc");
    let mut state = CpuState::default();
    assert_eq!(
        read_cpu_cores(&proc_root, &mut state, &cfg, clock_at(0)),
        None
    );

    tmp.write(
        "proc/stat",
        "cpu 60 0 40 180 0 0 0 0 0 0\n\
         cpu0 30 0 20 90 0 0 0 0 0 0\n\
         cpu1 30 0 20 90 0 0 0 0 0 0\n",
    );
    assert_eq!(
        read_cpu_cores(&proc_root, &mut state, &cfg, clock_at(2)),
        Some(vec![75, 75])
    );

    tmp.write(
        "proc/stat",
        "cpu 90 0 60 210 0 0 0 0 0 0\n\
         cpu0 30 0 20 90 0 0 0 0 0 0\n\
         cpu1 30 0 20 90 0 0 0 0 0 0\n\
         cpu2 30 0 20 90 0 0 0 0 0 0\n",
    );
    assert_eq!(
        read_cpu_cores(&proc_root, &mut state, &cfg, clock_at(4)),
        None
    );
    assert_eq!(state.cpu_core_history.len(), 3);
    assert!(state.cpu_core_history.iter().all(Vec::is_empty));
}

#[test]
fn read_cpu_cores_reports_zero_delta_and_rollback_as_invalid() {
    let tmp = TempTree::new();
    tmp.write(
        "proc/stat",
        "cpu 20 0 20 160 0 0 0 0 0 0\n\
         cpu0 10 0 10 80 0 0 0 0 0 0\n\
         cpu1 10 0 10 80 0 0 0 0 0 0\n",
    );
    let proc_root = tmp.path().join("proc");
    let mut state = CpuState::default();
    assert_eq!(
        read_cpu_cores_once(&proc_root, &mut state),
        CpuCoreReadOutcome::Baseline
    );

    tmp.write(
        "proc/stat",
        "cpu 60 0 40 180 0 0 0 0 0 0\n\
         cpu0 30 0 20 90 0 0 0 0 0 0\n\
         cpu1 30 0 20 90 0 0 0 0 0 0\n",
    );
    let measured = read_cpu_cores_once(&proc_root, &mut state);
    assert_eq!(measured, CpuCoreReadOutcome::Value(vec![75, 75]));
    append_cpu_core_history(
        &mut state,
        &Config::default(),
        Duration::from_secs(1),
        &[75, 75],
    );
    let history = state.cpu_core_history.clone();

    assert_eq!(
        read_cpu_cores_once(&proc_root, &mut state),
        CpuCoreReadOutcome::InvalidDelta
    );
    tmp.write(
        "proc/stat",
        "cpu 2 0 2 16 0 0 0 0 0 0\n\
         cpu0 1 0 1 8 0 0 0 0 0 0\n\
         cpu1 1 0 1 8 0 0 0 0 0 0\n",
    );
    assert_eq!(
        read_cpu_cores_once(&proc_root, &mut state),
        CpuCoreReadOutcome::InvalidDelta
    );
    assert_eq!(state.cpu_core_history, history);
}

#[test]
fn read_cpu_cores_returns_none_for_malformed_stat() {
    let tmp = TempTree::new();
    tmp.write("proc/stat", "cpu x y z\n");

    let mut state = CpuState::default();
    assert_eq!(
        read_cpu_cores(
            &tmp.path().join("proc"),
            &mut state,
            &Config::default(),
            clock_at(0)
        ),
        None
    );
    assert!(state.cpu_core_prev_times.is_empty());
}

#[test]
fn read_uptime_and_load_average_parse_fixture_proc_files() {
    assert_eq!(read_uptime_seconds(&fixture_proc()), Some(12345));
    assert_eq!(read_load_average(&fixture_proc()), Some((1.20, 0.90, 0.70)));
}

#[test]
fn read_uptime_and_load_average_return_none_for_malformed_files() {
    let tmp = TempTree::new();
    tmp.write("proc/uptime", "nope\n");
    tmp.write("proc/loadavg", "1.0 broken\n");

    assert_eq!(read_uptime_seconds(&tmp.path().join("proc")), None);
    assert_eq!(read_load_average(&tmp.path().join("proc")), None);
}

#[test]
fn read_cpu_frequency_prefers_sysfs_and_falls_back_to_cpuinfo() {
    let freq_path = fixture_sys().join("devices/system/cpu/cpu0/cpufreq/scaling_cur_freq");
    assert_eq!(
        read_cpu_frequency_mhz(&fixture_proc(), Some(&freq_path)),
        Some(3200.0)
    );

    let tmp = TempTree::new();
    tmp.write(
        "proc/cpuinfo",
        &fs::read_to_string(fixture_proc().join("cpuinfo")).unwrap_or_default(),
    );
    tmp.write(
        "sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq",
        "bogus\n",
    );
    assert_eq!(
        read_cpu_frequency_mhz(
            &tmp.path().join("proc"),
            Some(
                &tmp.path()
                    .join("sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
            )
        ),
        Some(2195.104)
    );
}

#[test]
fn read_cpu_turbo_handles_inversion_and_boost_fallback() {
    assert_eq!(read_cpu_turbo(&fixture_sys()), Some(true));

    let tmp = TempTree::new();
    tmp.write("sys/devices/system/cpu/cpufreq/boost", "1\n");
    assert_eq!(read_cpu_turbo(&tmp.path().join("sys")), Some(true));

    tmp.write("sys/devices/system/cpu/cpufreq/boost", "bogus\n");
    assert_eq!(read_cpu_turbo(&tmp.path().join("sys")), Some(false));
}
