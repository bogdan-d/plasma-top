#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn clock_at(seconds: u64) -> ClockSnapshot {
    ClockSnapshot {
        monotonic: Duration::from_secs(seconds),
        wall: UNIX_EPOCH + Duration::from_secs(seconds),
    }
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
        let root = std::env::temp_dir().join(format!(
            "plasma-top-intel-gpu-{}-{unique}",
            std::process::id()
        ));
        if let Err(error) = fs::create_dir_all(&root) {
            panic!("failed to create temp root {}: {error}", root.display());
        }
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write_str(&self, relative: &str, content: &str) {
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

    fn symlink(&self, original: &str, link_relative: &str) {
        let link = self.root.join(link_relative);
        if let Some(parent) = link.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                panic!("failed to create {}: {error}", parent.display());
            }
        }
        if let Err(error) = symlink(original, &link) {
            panic!(
                "failed to symlink {} -> {}: {error}",
                link.display(),
                original
            );
        }
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn detect_intel_gpu_returns_default_when_drm_dir_missing() {
    let tmp = TempTree::new();

    let paths = detect_intel_gpu(&tmp.path().join("sys"));

    assert_eq!(paths.freq_path, None);
    assert_eq!(paths.pci, None);
}

#[test]
fn detect_intel_gpu_skips_non_intel_and_non_display_cards() {
    let tmp = TempTree::new();
    // NVIDIA vendor, display class — skipped.
    tmp.write_str("sys/class/drm/card0/device/vendor", "0x10de\n");
    tmp.write_str("sys/class/drm/card0/device/class", "0x030000\n");
    // Intel vendor but not display — skipped.
    tmp.write_str("sys/class/drm/card1/device/vendor", "0x8086\n");
    tmp.write_str("sys/class/drm/card1/device/class", "0x088000\n");

    let paths = detect_intel_gpu(&tmp.path().join("sys"));

    assert_eq!(paths.freq_path, None);
    assert_eq!(paths.pci, None);
}

#[test]
fn detect_intel_gpu_picks_intel_display_card_and_freq_path() {
    let tmp = TempTree::new();
    // Place vendor/class on the resolved PCI device directory and make
    // `card0/device` a symlink to it so canonicalize() yields the PCI addr.
    tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/vendor", "0x8086\n");
    tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/class", "0x030000\n");
    tmp.write_str("sys/class/drm/card0/gt_act_freq_mhz", "1300\n");
    tmp.symlink(
        "../../../devices/pci0000:00/0000:00:02.0",
        "sys/class/drm/card0/device",
    );

    let paths = detect_intel_gpu(&tmp.path().join("sys"));

    assert_eq!(
        paths.freq_path,
        Some(tmp.path().join("sys/class/drm/card0/gt_act_freq_mhz"))
    );
    assert_eq!(paths.pci.as_deref(), Some("0000:00:02.0"));
}

#[test]
fn detect_intel_gpu_omits_freq_path_when_absent_but_returns_pci() {
    let tmp = TempTree::new();
    tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/vendor", "0x8086\n");
    tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/class", "0x030000\n");
    tmp.symlink(
        "../../../devices/pci0000:00/0000:00:02.0",
        "sys/class/drm/card0/device",
    );

    let paths = detect_intel_gpu(&tmp.path().join("sys"));

    assert_eq!(paths.freq_path, None);
    assert_eq!(paths.pci.as_deref(), Some("0000:00:02.0"));
}

#[test]
fn detect_intel_gpu_returns_first_card_in_sorted_order() {
    let tmp = TempTree::new();
    // Two Intel display cards; card0 wins because it sorts first.
    tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/vendor", "0x8086\n");
    tmp.write_str("sys/devices/pci0000:00/0000:00:02.0/class", "0x030000\n");
    tmp.write_str("sys/devices/pci0000:00/0000:01:00.0/vendor", "0x8086\n");
    tmp.write_str("sys/devices/pci0000:00/0000:01:00.0/class", "0x030000\n");
    tmp.symlink(
        "../../../devices/pci0000:00/0000:00:02.0",
        "sys/class/drm/card0/device",
    );
    tmp.symlink(
        "../../../devices/pci0000:00/0000:01:00.0",
        "sys/class/drm/card1/device",
    );

    let paths = detect_intel_gpu(&tmp.path().join("sys"));

    assert_eq!(paths.pci.as_deref(), Some("0000:00:02.0"));
}

#[test]
fn read_intel_gpu_engine_times_returns_empty_when_no_clients_match() {
    let tmp = TempTree::new();

    let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

    assert!(result.is_empty());
}

#[test]
fn read_intel_gpu_engine_times_collects_engine_counters_keyed_by_client_id() {
    let tmp = TempTree::new();
    // pid 100 has a fd whose readlink points at /dev/dri/renderD128.
    tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tmp.write_str(
        "proc/100/fdinfo/3",
        "pos:\t0\n\
         flags:\t02\n\
         drm-pdev:\t0000:00:02.0\n\
         drm-client-id:\t5\n\
         drm-engine-render:\t1000000000 ns\n\
         drm-engine-copy:\t500000000 ns\n\
         drm-engine-video:\t0 ns\n\
         drm-engine-video-enhance:\t0 ns\n",
    );

    let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

    let engines = result.get(&5).expect("client 5 present");
    assert_eq!(engines.get("render").copied(), Some(1_000_000_000));
    assert_eq!(engines.get("copy").copied(), Some(500_000_000));
    assert_eq!(engines.get("video").copied(), Some(0));
}

#[test]
fn read_intel_gpu_engine_times_skips_fds_without_dri_link() {
    let tmp = TempTree::new();
    tmp.symlink("/dev/null", "proc/100/fd/3");
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\n",
    );

    let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

    assert!(result.is_empty());
}

#[test]
fn read_intel_gpu_engine_times_skips_fds_with_mismatched_pdev() {
    let tmp = TempTree::new();
    tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:07.0\ndrm-client-id:\t5\n",
    );

    let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

    assert!(result.is_empty());
}

#[test]
fn read_intel_gpu_engine_times_skips_non_numeric_pid_dirs() {
    let tmp = TempTree::new();
    tmp.symlink("/dev/dri/renderD128", "proc/self/fd/3");
    tmp.write_str(
        "proc/self/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\n",
    );

    let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

    assert!(result.is_empty());
}

#[test]
fn read_intel_gpu_engine_times_dedupes_shared_fds_by_client_id() {
    let tmp = TempTree::new();
    // Two fds in one process pointing at the same DRM file (dup'd fd),
    // both reporting the same client id and engine counter. The scan
    // order from readdir is unspecified, so assert only the dedup: one
    // entry for client 5, not two.
    tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tmp.symlink("/dev/dri/renderD128", "proc/100/fd/4");
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t200 ns\n",
    );
    tmp.write_str(
        "proc/100/fdinfo/4",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t200 ns\n",
    );

    let result = read_intel_gpu_engine_times(&tmp.path().join("proc"), "0000:00:02.0");

    // Exactly one client entry, regardless of which fd was scanned last.
    assert_eq!(result.len(), 1);
    let engines = result.get(&5).expect("client 5 present");
    assert_eq!(engines.get("render").copied(), Some(200));
}

#[test]
fn read_intel_gpu_metrics_first_sample_is_baseline_without_metrics() {
    let tmp = TempTree::new();
    tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
    );

    let mut state = IntelGpuState::default();
    let metrics = read_intel_gpu_metrics(
        &tmp.path().join("proc"),
        &mut state,
        "0000:00:02.0",
        clock_at(0),
    );

    assert!(metrics.is_none());
    assert!(state.engine_prev.contains_key(&5));
    assert_eq!(state.prev_sample_at, Some(Duration::ZERO));
}

#[test]
fn read_intel_gpu_metrics_diffs_per_engine_and_caps_at_99() {
    let tmp = TempTree::new();
    tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");

    let mut state = IntelGpuState::default();
    // First sample: 0 ns everywhere (seeds prev).
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\n\
         drm-engine-render:\t0 ns\ndrm-engine-video:\t0 ns\n",
    );
    let _ = read_intel_gpu_metrics(
        &tmp.path().join("proc"),
        &mut state,
        "0000:00:02.0",
        clock_at(0),
    );

    // Second sample after 1s: render advanced by 1.5s of ns (150% → capped
    // at 99), video advanced by 0.5s of ns (50%).
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\n\
         drm-engine-render:\t1500000000 ns\ndrm-engine-video:\t500000000 ns\n",
    );
    let metrics = read_intel_gpu_metrics(
        &tmp.path().join("proc"),
        &mut state,
        "0000:00:02.0",
        clock_at(1),
    )
    .expect("comparable Intel GPU sample");

    assert_eq!(metrics.get("render").copied(), Some(99));
    assert_eq!(metrics.get("video").copied(), Some(50));
}

#[test]
fn nonpositive_elapsed_does_not_emit_zeros_or_advance_intel_baseline() {
    let tmp = TempTree::new();
    tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    let path = "proc/100/fdinfo/3";
    let fdinfo = |counter| {
        format!("drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t{counter} ns\n")
    };
    let mut state = IntelGpuState::default();
    tmp.write_str(path, &fdinfo(0));
    assert_eq!(
        read_intel_gpu_metrics_once(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(0),
        ),
        IntelGpuReadOutcome::Baseline
    );
    tmp.write_str(path, &fdinfo(1_000_000_000));
    assert!(matches!(
        read_intel_gpu_metrics_once(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(1),
        ),
        IntelGpuReadOutcome::Value(_)
    ));

    tmp.write_str(path, &fdinfo(1_500_000_000));
    assert_eq!(
        read_intel_gpu_metrics_once(
            &tmp.path().join("proc"),
            &mut state,
            "0000:00:02.0",
            clock_at(1),
        ),
        IntelGpuReadOutcome::Baseline
    );
    assert_eq!(state.prev_sample_at, Some(Duration::from_secs(1)));
    assert_eq!(state.engine_prev[&5]["render"], 1_000_000_000);

    tmp.write_str(path, &fdinfo(2_000_000_000));
    let recovered = read_intel_gpu_metrics_once(
        &tmp.path().join("proc"),
        &mut state,
        "0000:00:02.0",
        clock_at(2),
    );
    let IntelGpuReadOutcome::Value(recovered) = recovered else {
        panic!("expected comparable Intel sample");
    };
    assert_eq!(recovered["render"], 99);
}

#[test]
fn read_intel_gpu_metrics_sums_engines_across_clients() {
    let tmp = TempTree::new();
    tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    tmp.symlink("/dev/dri/renderD129", "proc/200/fd/7");

    let mut state = IntelGpuState::default();
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
    );
    tmp.write_str(
        "proc/200/fdinfo/7",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t7\ndrm-engine-render:\t0 ns\n",
    );
    let _ = read_intel_gpu_metrics(
        &tmp.path().join("proc"),
        &mut state,
        "0000:00:02.0",
        clock_at(0),
    );

    // Each client adds 0.4s of render over 1s → sum 0.8s/1s = 80%.
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t400000000 ns\n",
    );
    tmp.write_str(
        "proc/200/fdinfo/7",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t7\ndrm-engine-render:\t400000000 ns\n",
    );
    let metrics = read_intel_gpu_metrics(
        &tmp.path().join("proc"),
        &mut state,
        "0000:00:02.0",
        clock_at(1),
    )
    .expect("comparable Intel GPU sample");

    assert_eq!(metrics.get("render").copied(), Some(80));
}

#[test]
fn read_intel_gpu_metrics_skips_clients_absent_from_prev() {
    let tmp = TempTree::new();
    tmp.symlink("/dev/dri/renderD128", "proc/100/fd/3");
    let mut state = IntelGpuState::default();
    // Seed prev with only client 5.
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t5\ndrm-engine-render:\t0 ns\n",
    );
    let _ = read_intel_gpu_metrics(
        &tmp.path().join("proc"),
        &mut state,
        "0000:00:02.0",
        clock_at(0),
    );

    // Now point the same fd at a different client (new DRM client id).
    tmp.write_str(
        "proc/100/fdinfo/3",
        "drm-pdev:\t0000:00:02.0\ndrm-client-id:\t9\ndrm-engine-render:\t1000000000 ns\n",
    );
    let metrics = read_intel_gpu_metrics(
        &tmp.path().join("proc"),
        &mut state,
        "0000:00:02.0",
        clock_at(1),
    )
    .expect("comparable Intel GPU sample");

    // Client 9 had no prev → contributes 0.
    assert_eq!(metrics.get("render").copied(), Some(0));
}
