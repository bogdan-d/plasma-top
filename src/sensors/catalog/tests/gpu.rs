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
