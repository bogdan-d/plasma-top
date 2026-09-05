#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::os::unix::fs::symlink;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "plasma-top-amd-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("class/drm")).unwrap();
        Self(root)
    }

    fn card(&self, card: &str, pci: &str, vendor: &str, class: &str, driver: &str) -> PathBuf {
        let device = self.0.join("devices").join(pci);
        fs::create_dir_all(&device).unwrap();
        fs::write(device.join("vendor"), vendor).unwrap();
        fs::write(device.join("class"), class).unwrap();
        let driver_path = self.0.join("bus/pci/drivers").join(driver);
        fs::create_dir_all(&driver_path).unwrap();
        symlink(driver_path, device.join("driver")).unwrap();
        let card = self.0.join("class/drm").join(card);
        fs::create_dir_all(&card).unwrap();
        symlink(&device, card.join("device")).unwrap();
        device
    }

    fn amd(&self, card: &str, pci: &str) -> PathBuf {
        self.card(card, pci, "0x1002\n", "0x030000\n", "amdgpu")
    }

    fn full(&self) -> (PathBuf, AmdGpuSource) {
        let device = self.amd("card17", "0000:c3:00.0");
        for (file, text) in [
            ("gpu_busy_percent", "73\n"),
            ("vcn_busy_percent", "25\n"),
            ("mem_info_vram_used", "4294967296\n"),
            ("mem_info_vram_total", "17179869184\n"),
            ("mem_info_gtt_used", "999999999999\n"),
            ("mem_info_gtt_total", "9999999999999\n"),
        ] {
            fs::write(device.join(file), text).unwrap();
        }
        let hwmon = device.join("hwmon/hwmon42");
        fs::create_dir_all(&hwmon).unwrap();
        for (file, text) in [
            ("name", "amdgpu\n"),
            ("temp7_label", " EDGE \n"),
            ("temp7_input", "65999\n"),
            ("temp1_label", "junction\n"),
            ("temp1_input", "85000\n"),
            ("freq3_label", "ScLk\n"),
            ("freq3_input", "2800999999\n"),
            ("freq1_label", "mclk\n"),
            ("freq1_input", "9999999999\n"),
            ("power1_average", "120999999\n"),
            ("fan1_input", "1800\n"),
            ("pwm1", "255\n"),
        ] {
            fs::write(hwmon.join(file), text).unwrap();
        }
        let source = detect_amd_gpu(&self.0).unwrap().unwrap();
        (device, source)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn all_metrics() -> BTreeSet<Metric> {
    BTreeSet::from([
        Metric::GpuAmdUsage,
        Metric::GpuAmdCodecUsage,
        Metric::GpuAmdMemUsage,
        Metric::GpuAmdFreq,
        Metric::GpuAmdTemp,
        Metric::GpuAmdPower,
        Metric::GpuAmdFanSpeed,
    ])
}

fn value<T: Copy>(sample: &RetainedMetricSample<T>) -> T {
    sample.latest.as_ref().unwrap().value
}

#[test]
fn selection_uses_pci_identity_and_excludes_other_cards() {
    let fixture = Fixture::new();
    fixture.card("card0", "0000:01:00.0", "0x10de", "0x030000", "nvidia");
    fixture.card("card1", "0000:02:00.0", "0x1002", "0x040300", "amdgpu");
    fixture.card("card2", "0000:03:00.0", "0x1002", "0x030000", "radeon");
    fixture.amd("card3", "0000:c3:00.0");
    let first = fixture.amd("card99", "0000:04:00.1");
    // Connector and render-node entries are not DRM cards.
    fs::create_dir_all(fixture.0.join("class/drm/card0-DP-1")).unwrap();
    fs::create_dir_all(fixture.0.join("class/drm/renderD128")).unwrap();
    fs::write(first.join("gpu_busy_percent"), "0").unwrap();
    let source = detect_amd_gpu(&fixture.0).unwrap().unwrap();
    assert_eq!(source.pci_identity, "0000:04:00.1");
    assert_eq!(source.usage_path, Some(first.join("gpu_busy_percent")));
    fs::rename(
        fixture.0.join("class/drm/card99"),
        fixture.0.join("class/drm/card4"),
    )
    .unwrap();
    assert_eq!(detect_amd_gpu(&fixture.0).unwrap(), Some(source));
}

#[test]
fn only_complete_enumeration_can_confirm_absence() {
    let fixture = Fixture::new();
    assert_eq!(detect_amd_gpu(&fixture.0).unwrap(), None);
    let device = fixture.card("card0", "0000:01:00.0", "0x1002", "0x030000", "radeon");
    assert_eq!(detect_amd_gpu(&fixture.0).unwrap(), None);
    fs::remove_file(device.join("driver")).unwrap();
    assert_eq!(detect_amd_gpu(&fixture.0).unwrap(), None);
    symlink(device.join("missing-driver"), device.join("driver")).unwrap();
    assert!(detect_amd_gpu(&fixture.0).is_err());
    fs::remove_file(fixture.0.join("class/drm/card0/device")).unwrap();
    symlink(
        device.join("missing"),
        fixture.0.join("class/drm/card0/device"),
    )
    .unwrap();
    assert!(detect_amd_gpu(&fixture.0).is_err());
    assert!(detect_amd_gpu(&fixture.0.join("missing-root")).is_err());
}

#[test]
fn a_valid_candidate_does_not_hide_later_inspection_failure() {
    let fixture = Fixture::new();
    fixture.amd("card0", "0000:01:00.0");
    let second = fixture.amd("card9", "0000:09:00.0");
    for (file, malformed) in [
        ("vendor", "garbage"),
        ("vendor", "0x10000"),
        ("class", "0x1000000"),
        ("class", "0xzzzzzz"),
    ] {
        let path = second.join(file);
        let original = fs::read_to_string(&path).unwrap();
        fs::write(&path, malformed).unwrap();
        assert!(detect_amd_gpu(&fixture.0).is_err(), "{file}: {malformed}");
        fs::write(path, original).unwrap();
    }
    fs::remove_file(second.join("vendor")).unwrap();
    assert!(detect_amd_gpu(&fixture.0).is_err());
    fs::create_dir(second.join("vendor")).unwrap();
    assert!(detect_amd_gpu(&fixture.0).is_err());
}

#[test]
fn canonical_pci_name_must_be_valid() {
    for name in [
        "not-pci",
        "0000:gg:00.0",
        "0000:01:20.0",
        "0000:01:00.8",
        "0000:01:00.💥",
    ] {
        let fixture = Fixture::new();
        fixture.amd("card0", name);
        assert!(detect_amd_gpu(&fixture.0).is_err(), "{name}");
    }
}

#[test]
fn optional_hwmon_and_vram_capabilities_are_independent() {
    let fixture = Fixture::new();
    let (device, mut source) = fixture.full();
    assert!(source.temp_path.as_ref().unwrap().ends_with("temp7_input"));
    assert!(source.freq_path.as_ref().unwrap().ends_with("freq3_input"));
    fs::remove_file(source.fan_speed_path.as_ref().unwrap()).unwrap();
    source.fan_speed_path = None;
    assert_eq!(detect_amd_gpu(&fixture.0).unwrap(), Some(source.clone()));
    fs::remove_file(device.join("mem_info_vram_total")).unwrap();
    source.memory_paths = None;
    assert_eq!(detect_amd_gpu(&fixture.0).unwrap(), Some(source.clone()));
    fs::write(device.join("hwmon/hwmon42/temp7_label"), "edge hotspot").unwrap();
    fs::write(device.join("hwmon/hwmon42/freq3_label"), "mclk").unwrap();
    source.temp_path = None;
    source.freq_path = None;
    assert_eq!(detect_amd_gpu(&fixture.0).unwrap(), Some(source));
}

#[test]
fn broken_hwmon_inspection_is_failure_not_missing_capability() {
    for broken in [
        "hwmon",
        "hwmon/hwmon5",
        "hwmon/hwmon5/name",
        "gpu_busy_percent",
    ] {
        let fixture = Fixture::new();
        let device = fixture.amd("card0", "0000:01:00.0");
        let path = device.join(broken);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(device.join("missing"), path).unwrap();
        assert!(detect_amd_gpu(&fixture.0).is_err(), "{broken}");
    }
    let fixture = Fixture::new();
    let (device, _) = fixture.full();
    fs::remove_file(device.join("hwmon/hwmon42/temp7_label")).unwrap();
    fs::create_dir(device.join("hwmon/hwmon42/temp7_label")).unwrap();
    assert!(detect_amd_gpu(&fixture.0).is_err());
}

#[test]
fn samples_convert_units_and_preserve_only_vram_allocation_bytes() {
    let fixture = Fixture::new();
    let (_, source) = fixture.full();
    let mut state = AmdGpuState::default();
    state.reconcile_source(Some(&source));
    state.sample(&all_metrics(), Duration::ZERO);
    let samples = state.samples();
    assert_eq!(value(&samples.usage), 73);
    assert_eq!(value(&samples.codec_usage), 25);
    assert_eq!(
        value(&samples.memory),
        AmdGpuMemoryReading {
            used_bytes: 4_294_967_296,
            total_bytes: 17_179_869_184
        }
    );
    assert_eq!(value(&samples.memory).percent(), Some(25));
    assert_eq!(value(&samples.frequency), 2800);
    assert_eq!(value(&samples.temperature), 65);
    assert_eq!(value(&samples.power), 120);
    assert_eq!(value(&samples.fan_speed), 1800);
    assert_eq!(samples.usage.attempted_at, Some(Duration::ZERO));
}

#[test]
fn parsers_validate_boundaries_and_failed_file_reads() {
    let fixture = Fixture::new();
    let path = fixture.0.join("value");
    for (text, expected) in [
        ("0", Some(0)),
        ("100\n", Some(100)),
        (" 50 \n", Some(50)),
        ("-1", None),
        ("101", None),
        ("1.5", None),
        ("", None),
        ("18446744073709551616", None),
    ] {
        fs::write(&path, text).unwrap();
        assert_eq!(read_percent(&path), expected, "{text}");
    }
    for (text, expected) in [
        ("-273150", Some(-273)),
        ("-273151", None),
        ("-1999", Some(-1)),
        ("0", Some(0)),
        ("999999", Some(999)),
        ("1000000", None),
        ("9223372036854775808", None),
        ("garbage", None),
    ] {
        fs::write(&path, text).unwrap();
        assert_eq!(read_temperature(&path), expected, "{text}");
    }
    for divisor in [1, 1_000_000] {
        let maximum = u64::from(u32::MAX) * divisor + divisor - 1;
        for (text, expected) in [
            ("0".to_owned(), Some(0)),
            (maximum.to_string(), Some(u32::MAX)),
            ((maximum + 1).to_string(), None),
            ("-1".to_owned(), None),
            ("18446744073709551616".to_owned(), None),
            ("garbage".to_owned(), None),
        ] {
            fs::write(&path, &text).unwrap();
            assert_eq!(
                read_scaled_u32(&path, divisor),
                expected,
                "{text} / {divisor}"
            );
        }
    }
    fs::remove_file(&path).unwrap();
    for unreadable in [&path, &fixture.0] {
        assert_eq!(read_percent(unreadable), None);
        assert_eq!(read_temperature(unreadable), None);
        assert_eq!(read_scaled_u32(unreadable, 1), None);
        assert_eq!(read_scaled_u32(unreadable, 1_000_000), None);
    }
}

#[test]
fn vram_pair_rejects_invalid_counters_without_overflow() {
    let fixture = Fixture::new();
    let paths = AmdGpuMemoryPaths {
        used: fixture.0.join("used"),
        total: fixture.0.join("total"),
    };
    for (used, total, percent) in [
        ("0", "1", Some(0)),
        ("1", "1", Some(100)),
        ("0", "0", None),
        ("2", "1", None),
        ("-1", "1", None),
        ("1", "garbage", None),
        ("garbage", "1", None),
        ("18446744073709551615", "18446744073709551615", Some(100)),
        ("18446744073709551616", "18446744073709551615", None),
        ("1", "18446744073709551616", None),
    ] {
        fs::write(&paths.used, used).unwrap();
        fs::write(&paths.total, total).unwrap();
        assert_eq!(
            read_memory(&paths).and_then(AmdGpuMemoryReading::percent),
            percent,
            "{used} / {total}"
        );
    }
    for path in [&paths.used, &paths.total] {
        fs::write(&paths.used, "1").unwrap();
        fs::write(&paths.total, "2").unwrap();
        fs::remove_file(path).unwrap();
        assert_eq!(read_memory(&paths), None);
        fs::create_dir(path).unwrap();
        assert_eq!(read_memory(&paths), None);
        fs::remove_dir(path).unwrap();
    }
}

#[test]
fn attempts_retain_failed_fields_update_siblings_and_respect_demand() {
    let fixture = Fixture::new();
    let (_, source) = fixture.full();
    let mut state = AmdGpuState::default();
    state.reconcile_source(Some(&source));
    state.sample(&all_metrics(), Duration::from_secs(1));
    fs::write(source.temp_path.as_ref().unwrap(), "bad").unwrap();
    fs::remove_file(source.usage_path.as_ref().unwrap()).unwrap();
    fs::write(source.codec_usage_path.as_ref().unwrap(), "80").unwrap();
    fs::write(source.fan_speed_path.as_ref().unwrap(), "0").unwrap();
    state.sample(&all_metrics(), Duration::from_secs(2));
    assert_eq!(value(&state.samples().temperature), 65);
    assert_eq!(value(&state.samples().usage), 73);
    assert_eq!(
        state
            .samples()
            .temperature
            .latest
            .as_ref()
            .unwrap()
            .captured_at,
        Duration::from_secs(1)
    );
    assert_eq!(
        state.samples().temperature.failed_at,
        Some(Duration::from_secs(2))
    );
    assert!(state.samples().usage.latest_attempt_failed);
    assert_eq!(value(&state.samples().codec_usage), 80);
    assert_eq!(value(&state.samples().fan_speed), 0);
    assert!(!state.samples().memory.latest_attempt_failed);
    let previous = state.samples().clone();
    state.sample(&BTreeSet::new(), Duration::from_secs(3));
    assert_eq!(state.samples(), &previous);
    state.sample(
        &BTreeSet::from([Metric::GpuAmdCodecUsage]),
        Duration::from_secs(4),
    );
    assert_eq!(
        state.samples().codec_usage.attempted_at,
        Some(Duration::from_secs(4))
    );
    assert_eq!(state.samples().temperature, previous.temperature);
    fs::write(source.temp_path.as_ref().unwrap(), "42000").unwrap();
    state.sample(
        &BTreeSet::from([Metric::GpuAmdTemp]),
        Duration::from_secs(5),
    );
    assert_eq!(value(&state.samples().temperature), 42);
    assert!(!state.samples().temperature.latest_attempt_failed);
}

#[test]
fn source_reconciliation_invalidates_only_affected_samples() {
    let fixture = Fixture::new();
    let (_, mut source) = fixture.full();
    let mut state = AmdGpuState::default();
    state.reconcile_source(Some(&source));
    state.sample(&all_metrics(), Duration::from_secs(1));
    let original = state.clone();
    state.reconcile_source(Some(&source));
    assert_eq!(state, original);
    source.fan_speed_path = None;
    source.temp_path = Some(fixture.0.join("replacement-temperature"));
    state.reconcile_source(Some(&source));
    assert_eq!(state.samples().temperature, RetainedMetricSample::default());
    assert_eq!(state.samples().fan_speed, RetainedMetricSample::default());
    assert_eq!(state.samples().usage, original.samples().usage);
    state.sample(&all_metrics(), Duration::from_secs(2));
    assert!(state.samples().temperature.latest_attempt_failed);
    assert!(!state.samples().fan_speed.latest_attempt_failed);
    assert_eq!(
        state.samples().fan_speed.attempted_at,
        Some(Duration::from_secs(2))
    );
    source.pci_identity = "0000:01:00.0".to_owned();
    state.reconcile_source(Some(&source));
    assert_eq!(state.samples(), &AmdGpuSamples::default());
    state.sample(&all_metrics(), Duration::from_secs(3));
    state.reconcile_source(None);
    assert_eq!(state.samples(), &AmdGpuSamples::default());
    state.sample(&all_metrics(), Duration::from_secs(4));
    assert_eq!(state.samples().usage.latest, None);
    assert_eq!(
        state.samples().usage.attempted_at,
        Some(Duration::from_secs(4))
    );
    assert!(!state.samples().usage.latest_attempt_failed);
}

#[test]
fn local_discovery_merges_amd_success_failure_and_confirmed_removal() {
    use crate::config::Config;
    use crate::sensors::{discover_local_hardware, discover_local_hardware_attempt};

    let fixture = Fixture::new();
    let (_, source) = fixture.full();
    let cfg = Config::default();
    let proc_root = fixture.0.join("proc");
    let mut inventory = discover_local_hardware(&fixture.0, &proc_root, &cfg, 4);
    assert_eq!(inventory.amd_gpu, Some(source));
    let drm = fixture.0.join("class/drm");
    let saved = fixture.0.join("saved-drm");
    fs::rename(&drm, &saved).unwrap();
    discover_local_hardware_attempt(&fixture.0, &proc_root, &cfg, 4).merge_into(&mut inventory);
    assert!(inventory.amd_gpu.is_some());
    fs::create_dir(&drm).unwrap();
    discover_local_hardware_attempt(&fixture.0, &proc_root, &cfg, 4).merge_into(&mut inventory);
    assert!(inventory.amd_gpu.is_none());
}
