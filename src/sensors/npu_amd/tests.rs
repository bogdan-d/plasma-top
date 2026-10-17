#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const APU: &str = "0000:c3:00.0";
const NPU: &str = "0000:c4:00.1";

fn captured(name: &str) -> Vec<u8> {
    fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/npu")
            .join(name),
    )
    .unwrap()
}

fn active() -> Vec<u8> {
    captured("gpu_metrics.active.bin")
}

fn suspended() -> Vec<u8> {
    captured("gpu_metrics.suspended.bin")
}

fn put_u16(table: &mut [u8], offset: usize, value: u16) {
    table[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "plasma-top-npu-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("class/drm")).unwrap();
        Self(root)
    }

    fn device(&self, pci: &str, driver: &str) -> PathBuf {
        let device = self.0.join("devices").join(pci);
        fs::create_dir_all(&device).unwrap();
        let driver_path = self.0.join("bus/pci/drivers").join(driver);
        fs::create_dir_all(&driver_path).unwrap();
        symlink(driver_path, device.join("driver")).unwrap();
        device
    }

    fn accel(&self, name: &str, pci: &str, driver: &str) {
        let device = self.device(pci, driver);
        let accel = self.0.join("class/accel").join(name);
        fs::create_dir_all(&accel).unwrap();
        symlink(device, accel.join("device")).unwrap();
    }

    fn card(&self, name: &str, pci: &str, table: Option<&[u8]>) -> PathBuf {
        let device = self.device(pci, "amdgpu");
        if let Some(table) = table {
            fs::write(device.join("gpu_metrics"), table).unwrap();
        }
        let card = self.0.join("class/drm").join(name);
        fs::create_dir_all(&card).unwrap();
        symlink(&device, card.join("device")).unwrap();
        device.join("gpu_metrics")
    }

    // The real Strix Halo pair: NPU and APU iGPU on separate PCI buses.
    fn strix_halo(&self, table: &[u8]) -> PathBuf {
        self.accel("accel0", NPU, "amdxdna");
        self.card("card1", APU, Some(table))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn state_for(fixture: &Fixture) -> AmdNpuState {
    let source = detect_amd_npu(&fixture.0).unwrap().expect("NPU source");
    let mut state = AmdNpuState::default();
    state.reconcile_source(Some(&source));
    state
}

fn latest<T: Copy>(sample: &RetainedMetricSample<T>) -> Option<T> {
    sample.latest.as_ref().map(|sample| sample.value)
}

fn readings(state: &AmdNpuState) -> (Option<i32>, Option<u16>, Option<u16>) {
    let samples = state.samples();
    (
        latest(&samples.usage),
        latest(&samples.power_mw),
        latest(&samples.frequency),
    )
}

#[test]
fn detection_pairs_the_amdxdna_device_with_the_qualifying_apu_table() {
    let fixture = Fixture::new();
    let path = fixture.strix_halo(&active());
    let source = detect_amd_npu(&fixture.0).unwrap().expect("NPU source");
    assert_eq!(source.accel_pci, NPU);
    assert_eq!(source.metrics_pci, APU);
    assert_eq!(source.metrics_path, fs::canonicalize(path).unwrap());
    assert_eq!(source.identity(), format!("npu:{NPU}+{APU}"));
}

#[test]
fn a_discrete_gpu_table_never_becomes_the_npu_source() {
    let fixture = Fixture::new();
    let mut discrete = active();
    discrete[2] = 1;
    fixture.card("card0", "0000:03:00.0", Some(&discrete));
    fixture.card("card2", "0000:04:00.0", None);
    fixture.strix_halo(&active());
    let source = detect_amd_npu(&fixture.0).unwrap().expect("NPU source");
    assert_eq!(source.metrics_pci, APU);
}

#[test]
fn detection_requires_an_amdxdna_accel_device_and_a_supported_table() {
    let missing_accel = Fixture::new();
    missing_accel.card("card1", APU, Some(&active()));
    assert_eq!(detect_amd_npu(&missing_accel.0).unwrap(), None);

    let other_driver = Fixture::new();
    other_driver.accel("accel0", NPU, "ivpu");
    other_driver.card("card1", APU, Some(&active()));
    assert_eq!(detect_amd_npu(&other_driver.0).unwrap(), None);

    for (offset, value) in [(2, 2), (3, 1)] {
        let fixture = Fixture::new();
        let mut table = active();
        table[offset] = value;
        fixture.strix_halo(&table);
        assert_eq!(
            detect_amd_npu(&fixture.0).unwrap(),
            None,
            "revision byte {offset}"
        );
    }

    let small = Fixture::new();
    let mut table = active();
    put_u16(&mut table, 0, 200);
    small.strix_halo(&table);
    assert_eq!(detect_amd_npu(&small.0).unwrap(), None);
}

#[test]
fn incomplete_tables_at_discovery_are_failures_not_absence() {
    let declared_larger = Fixture::new();
    let mut table = active();
    put_u16(&mut table, 0, 300);
    declared_larger.strix_halo(&table);
    assert!(detect_amd_npu(&declared_larger.0).is_err());

    let short = Fixture::new();
    short.strix_halo(&active()[..100]);
    assert!(detect_amd_npu(&short.0).is_err());
}

#[test]
fn captured_tables_decode_busy_power_and_clock() {
    let fixture = Fixture::new();
    let path = fixture.strix_halo(&active());
    let mut state = state_for(&fixture);
    state.sample(Duration::from_secs(1));
    assert_eq!(readings(&state), (Some(96), Some(3352), Some(1810)));

    fs::write(path, suspended()).unwrap();
    state.sample(Duration::from_secs(2));
    assert_eq!(readings(&state), (Some(0), Some(0), Some(0)));
}

#[test]
fn failed_reads_retain_values_and_capture_times() {
    let fixture = Fixture::new();
    let path = fixture.strix_halo(&active());
    let mut state = state_for(&fixture);
    state.sample(Duration::from_secs(1));
    let mut declared_larger = active();
    put_u16(&mut declared_larger, 0, 300);
    for broken in [declared_larger, active()[..100].to_vec()] {
        fs::write(&path, broken).unwrap();
        state.sample(Duration::from_secs(2));
        assert_eq!(readings(&state), (Some(96), Some(3352), Some(1810)));
        assert!(state.samples().usage.latest_attempt_failed);
        assert_eq!(
            state
                .samples()
                .power_mw
                .latest
                .as_ref()
                .unwrap()
                .captured_at,
            Duration::from_secs(1)
        );
    }
    fs::remove_file(&path).unwrap();
    state.sample(Duration::from_secs(3));
    assert_eq!(readings(&state), (Some(96), Some(3352), Some(1810)));
    assert!(state.samples().frequency.latest_attempt_failed);
}

#[test]
fn an_unsupported_table_clears_every_value_until_a_supported_table_requalifies() {
    let fixture = Fixture::new();
    let path = fixture.strix_halo(&active());
    let mut state = state_for(&fixture);
    state.sample(Duration::from_secs(1));
    let mut table = active();
    table[2] = 4;
    fs::write(&path, table).unwrap();
    state.sample(Duration::from_secs(2));
    assert_eq!(readings(&state), (None, None, None));
    assert!(!state.samples().usage.latest_attempt_failed);

    fs::write(&path, suspended()).unwrap();
    state.sample(Duration::from_secs(3));
    assert_eq!(readings(&state), (Some(0), Some(0), Some(0)));
}

#[test]
fn mixed_table_clears_unavailable_retains_invalid_and_updates_valid_fields() {
    let fixture = Fixture::new();
    let path = fixture.strix_halo(&active());
    let mut state = state_for(&fixture);
    state.sample(Duration::from_secs(1));

    let mut table = suspended();
    put_u16(&mut table, IPU_ACTIVITY_OFFSET + 6, 101);
    put_u16(&mut table, IPU_POWER_OFFSET, UNAVAILABLE);
    put_u16(&mut table, IPU_CLOCK_OFFSET, 400);
    fs::write(&path, &table).unwrap();
    state.sample(Duration::from_secs(2));
    assert_eq!(readings(&state), (Some(96), None, Some(400)));
    assert!(state.samples().usage.latest_attempt_failed);
    assert!(!state.samples().power_mw.latest_attempt_failed);

    put_u16(&mut table, IPU_ACTIVITY_OFFSET + 6, UNAVAILABLE);
    put_u16(&mut table, IPU_POWER_OFFSET, 1250);
    fs::write(&path, &table).unwrap();
    state.sample(Duration::from_secs(3));
    assert_eq!(readings(&state), (None, Some(1250), Some(400)));
}

#[test]
fn a_changed_source_identity_clears_samples() {
    let fixture = Fixture::new();
    fixture.strix_halo(&active());
    let mut state = state_for(&fixture);
    state.sample(Duration::from_secs(1));
    let mut replaced = detect_amd_npu(&fixture.0).unwrap().unwrap();
    replaced.accel_pci = String::from("0000:c5:00.1");
    state.reconcile_source(Some(&replaced));
    assert_eq!(readings(&state), (None, None, None));
    state.sample(Duration::from_secs(2));
    assert_eq!(readings(&state), (Some(96), Some(3352), Some(1810)));
    state.reconcile_source(None);
    state.sample(Duration::from_secs(3));
    assert_eq!(readings(&state), (None, None, None));
}
