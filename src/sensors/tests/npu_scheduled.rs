use super::*;
use crate::domain::readings::AmdNpuSource;
use crate::scheduler::{
    CompletionKind, ConfigGeneration, InventoryGeneration, JobId, JobKind, JobTicket, OwnerId,
    RunId, SourceIdentity,
};
use crate::sensors::ReconciliationOutcome;

const IDENTITY: &str = "npu:0000:c4:00.1+0000:c3:00.0";

fn hardware(tree: &TempTree) -> HardwareInventory {
    let table = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/npu/gpu_metrics.active.bin"),
    )
    .expect("captured table");
    tree.mkdir("sys/npu");
    fs::write(tree.sys().join("npu/gpu_metrics"), table).expect("table");
    HardwareInventory {
        amd_npu: Some(AmdNpuSource {
            accel_pci: String::from("0000:c4:00.1"),
            metrics_pci: String::from("0000:c3:00.0"),
            metrics_path: tree.sys().join("npu/gpu_metrics"),
        }),
        ..HardwareInventory::default()
    }
}

fn config(panel: &[&str], tooltip: &[&str]) -> Config {
    let mut cfg = cfg_panel(panel);
    cfg.tooltip = panel_section(tooltip);
    cfg.pages.order.clear();
    cfg
}

fn npu_job() -> JobId {
    JobId::with_source(
        OwnerId::AmdNpu,
        JobKind::AmdNpu,
        SourceIdentity::Device(String::from(IDENTITY)),
    )
}

fn run(
    tree: &TempTree,
    owners: &mut TestOwners,
    hw: &mut HardwareInventory,
    cfg: &Config,
    readings: &mut DisplaySnapshot,
) -> CompletionKind {
    let ticket = JobTicket {
        job: npu_job(),
        metrics: BTreeSet::new(),
        run_id: RunId(1),
        config_generation: ConfigGeneration(1),
        inventory_generation: InventoryGeneration(1),
        history_deadline: None,
    };
    let mut commands = FakeCommandRunner::new();
    let mut dbus = FakeDbus::new();
    let mut clock = || clock(1);
    let proc_root = tree.proc();
    let sys_root = tree.sys();
    let mut ctx = crate::sensors::CollectCtx {
        proc_root: &proc_root,
        sys_root: &sys_root,
        commands: &mut commands,
        dbus: &mut dbus,
        clock: &mut clock,
        nvml: None,
        bolt: None,
        skip_slow: false,
    };
    crate::sensors::execute_scheduled_job(&ticket, owners.refs(), hw, cfg, &mut ctx, readings, None)
        .completion
}

#[test]
fn npu_jobs_exist_only_while_an_npu_item_is_demanded() {
    let tree = TempTree::new();
    let hw = hardware(&tree);
    let plan = |cfg: &Config| crate::sensors::scheduler_config(cfg, &hw, &[], ConfigGeneration(1));

    let idle = plan(&config(&["cpu_usage"], &["gpu_amd_usage"]));
    assert!(
        idle.jobs
            .iter()
            .all(|spec| spec.id.owner != OwnerId::AmdNpu)
    );
    assert!(idle.jobs.iter().all(|spec| spec.id.source
        != SourceIdentity::Inventory(crate::domain::readings::InventoryFamily::AmdNpu)));

    let panel = plan(&config(&["npu_amd_power"], &[]));
    assert!(panel.demand.hidden.contains(&npu_job()));
    assert!(panel.demand.hidden.contains(&JobId::with_source(
        OwnerId::Discovery,
        JobKind::HardwareDiscovery,
        SourceIdentity::Inventory(crate::domain::readings::InventoryFamily::AmdNpu),
    )));

    let tooltip = plan(&config(&[], &["npu_amd_usage", "npu_amd_freq"]));
    assert!(tooltip.demand.main_tooltip.contains(&npu_job()));
    assert!(!tooltip.demand.hidden.contains(&npu_job()));
    let spec = tooltip
        .jobs
        .iter()
        .find(|spec| spec.id == npu_job())
        .expect("NPU job");
    assert_eq!(spec.freshness, tooltip.display_interval);

    let absent = crate::sensors::scheduler_config(
        &config(&["npu_amd_usage"], &[]),
        &HardwareInventory::default(),
        &[],
        ConfigGeneration(1),
    );
    assert!(
        absent
            .jobs
            .iter()
            .all(|spec| spec.id.owner != OwnerId::AmdNpu)
    );
}

#[test]
fn one_demanded_item_reads_every_field_and_removed_demand_clears_them() {
    let tree = TempTree::new();
    let mut hw = hardware(&tree);
    let mut owners = TestOwners::default();
    let mut readings = DisplaySnapshot::default();
    let power_only = config(&["npu_amd_power"], &[]);
    assert_eq!(
        run(&tree, &mut owners, &mut hw, &power_only, &mut readings),
        CompletionKind::Captured
    );
    assert_eq!(readings.npu_amd_usage, Some(96));
    assert_eq!(readings.npu_amd_power_mw, Some(3352));
    assert_eq!(readings.npu_amd_freq, Some(1810));

    let none = config(&["cpu_usage"], &[]);
    assert_eq!(
        run(&tree, &mut owners, &mut hw, &none, &mut readings),
        CompletionKind::ConfirmedAbsent
    );
    assert_eq!(
        (
            readings.npu_amd_usage,
            readings.npu_amd_power_mw,
            readings.npu_amd_freq
        ),
        (None, None, None)
    );
}

#[test]
fn inventory_reconciliation_retains_on_failure_and_clears_on_confirmed_removal() {
    use crate::domain::readings::InventoryFamily;
    let tree = TempTree::new();
    let root = tree.root.display().to_string();
    for (pci, driver) in [("0000:c4:00.1", "amdxdna"), ("0000:c3:00.0", "amdgpu")] {
        tree.mkdir(&format!("sys/bus/pci/drivers/{driver}"));
        tree.symlink(
            &format!("{root}/sys/bus/pci/drivers/{driver}"),
            &format!("sys/devices/{pci}/driver"),
        );
    }
    tree.symlink(
        &format!("{root}/sys/devices/0000:c4:00.1"),
        "sys/class/accel/accel0/device",
    );
    tree.symlink(
        &format!("{root}/sys/devices/0000:c3:00.0"),
        "sys/class/drm/card1/device",
    );
    let table = tree.sys().join("devices/0000:c3:00.0/gpu_metrics");
    let active = fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/npu/gpu_metrics.active.bin"),
    )
    .expect("captured table");
    fs::write(&table, &active).expect("table");

    let mut hw = HardwareInventory::default();
    let reconcile = |hw: &mut HardwareInventory| {
        crate::sensors::reconcile_inventory_family(
            InventoryFamily::AmdNpu,
            hw,
            &tree.sys(),
            &tree.proc(),
            &Config::default(),
            &mut FakeDbus::new(),
            &mut FakeCommandRunner::new(),
        )
    };
    assert_eq!(reconcile(&mut hw), ReconciliationOutcome::Captured);
    let source = hw.amd_npu.clone().expect("NPU source");
    assert_eq!(source.identity(), IDENTITY);

    fs::write(&table, &active[..100]).expect("truncated table");
    assert_eq!(reconcile(&mut hw), ReconciliationOutcome::Failed);
    assert_eq!(hw.amd_npu, Some(source));

    fs::write(&table, &active).expect("table");
    fs::remove_dir_all(tree.sys().join("class/accel")).expect("remove accel class");
    assert_eq!(reconcile(&mut hw), ReconciliationOutcome::Captured);
    assert_eq!(hw.amd_npu, None);
}
