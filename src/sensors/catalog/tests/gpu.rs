use super::*;

#[test]
fn gpu_history_is_deadline_only_and_selected_source_aware() {
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip.sections.clear();
    cfg.pages.order = vec![String::from("graphs")];
    let nvidia = scheduler_config(
        &cfg,
        &HardwareInventory {
            has_nvidia: true,
            ..HardwareInventory::default()
        },
        Path::new("/missing-proc"),
        ConfigGeneration(1),
    );
    let nvidia_history = nvidia
        .jobs
        .iter()
        .find(|spec| spec.id.kind == JobKind::GpuHistory)
        .expect("NVIDIA history");
    assert_eq!(nvidia_history.timing, TimingClass::History);
    assert_eq!(
        nvidia_history.id.source,
        SourceIdentity::Device(String::from("nvidia"))
    );

    let intel = scheduler_config(
        &cfg,
        &HardwareInventory {
            intel_gpu_pci: Some(String::from("0000:00:02.0")),
            ..HardwareInventory::default()
        },
        Path::new("/missing-proc"),
        ConfigGeneration(2),
    );
    let intel_history = intel
        .jobs
        .iter()
        .find(|spec| spec.id.kind == JobKind::GpuHistory)
        .expect("Intel history");
    assert_ne!(nvidia_history.id, intel_history.id);
}

#[test]
fn amd_graph_catalog_obeys_vendor_precedence_for_every_inventory() {
    use crate::domain::readings::AmdGpuSource;
    let mut cfg = without_notifications(Config::default());
    cfg.panel.sections.clear();
    cfg.tooltip.sections.clear();
    cfg.pages.order = vec![String::from("graphs")];
    for mask in 0..8 {
        let hw = HardwareInventory {
            has_nvidia: mask & 1 != 0,
            amd_gpu: (mask & 2 != 0).then(|| AmdGpuSource {
                pci_identity: String::from("0000:c3:00.0"),
                usage_path: Some("/amd/usage".into()),
                codec_usage_path: Some("/amd/codec".into()),
                ..AmdGpuSource::default()
            }),
            intel_gpu_pci: (mask & 4 != 0).then(|| String::from("0000:00:02.0")),
            ..HardwareInventory::default()
        };
        let config = scheduler_config(&cfg, &hw, Path::new("/missing-proc"), ConfigGeneration(1));
        let expected = if mask & 1 != 0 {
            Some("nvidia")
        } else if mask & 2 != 0 {
            Some("amd:0000:c3:00.0")
        } else if mask & 4 != 0 {
            Some("intel:0000:00:02.0")
        } else {
            None
        };
        let histories: Vec<_> = config
            .jobs
            .iter()
            .filter(|spec| spec.id.kind == JobKind::GpuHistory)
            .collect();
        assert_eq!(
            histories.len(),
            usize::from(expected.is_some()),
            "mask {mask}"
        );
        if let Some(expected) = expected {
            assert_eq!(
                histories[0].id.source,
                SourceIdentity::Device(expected.into())
            );
            assert_eq!(histories[0].timing, TimingClass::History);
        }
        assert_eq!(
            config
                .jobs
                .iter()
                .any(|spec| spec.id.kind == JobKind::AmdFast),
            mask & 1 == 0 && mask & 2 != 0
        );
        assert_eq!(
            config
                .jobs
                .iter()
                .any(|spec| spec.id.kind == JobKind::IntelUsage),
            mask == 4
        );
    }
}
