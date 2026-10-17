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

fn history_job(identity: &str) -> JobId {
    JobId::with_source(
        OwnerId::AmdNpu,
        JobKind::AmdNpuHistory,
        SourceIdentity::Device(String::from(identity)),
    )
}

// Graphs page configured with only the NPU chart, and no NPU items.
fn chart_config() -> Config {
    let mut cfg = config(&["cpu_usage"], &[]);
    cfg.pages.order = vec![String::from("graphs")];
    cfg.pages.graph_order = vec![String::from("npu")];
    cfg
}

fn run(
    tree: &TempTree,
    owners: &mut TestOwners,
    hw: &mut HardwareInventory,
    cfg: &Config,
    readings: &mut DisplaySnapshot,
) -> CompletionKind {
    run_job(npu_job(), tree, owners, hw, cfg, readings)
}

fn run_job(
    job: JobId,
    tree: &TempTree,
    owners: &mut TestOwners,
    hw: &mut HardwareInventory,
    cfg: &Config,
    readings: &mut DisplaySnapshot,
) -> CompletionKind {
    let ticket = JobTicket {
        job,
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

#[test]
fn the_npu_chart_demands_npu_work_only_while_graphs_lists_it() {
    use crate::domain::readings::InventoryFamily;
    let tree = TempTree::new();
    let hw = hardware(&tree);
    let plan = |cfg: &Config, hw: &HardwareInventory| {
        crate::sensors::scheduler_config(cfg, hw, &[], ConfigGeneration(1))
    };
    let inventory = JobId::with_source(
        OwnerId::Discovery,
        JobKind::HardwareDiscovery,
        SourceIdentity::Inventory(InventoryFamily::AmdNpu),
    );

    let chart = plan(&chart_config(), &hw);
    for job in [npu_job(), history_job(IDENTITY), inventory.clone()] {
        assert!(chart.demand.hidden.contains(&job), "{job:?}");
    }
    let history = chart
        .jobs
        .iter()
        .find(|spec| spec.id == history_job(IDENTITY))
        .expect("history job");
    assert_eq!(history.history_source, Some(npu_job()));

    let mut graphs_without_chart = chart_config();
    graphs_without_chart.pages.graph_order = vec![String::from("gpu")];
    let mut chart_without_graphs = chart_config();
    chart_without_graphs.pages.order.clear();
    for cfg in [graphs_without_chart, chart_without_graphs] {
        let plan = plan(&cfg, &hw);
        assert!(
            plan.jobs
                .iter()
                .all(|spec| spec.id.owner != OwnerId::AmdNpu)
        );
        assert!(!plan.demand.hidden.contains(&inventory));
    }

    let absent = plan(&chart_config(), &HardwareInventory::default());
    assert!(
        absent
            .jobs
            .iter()
            .all(|spec| spec.id.owner != OwnerId::AmdNpu)
    );

    // Dropped job ids are what the scheduler invalidates, clearing the merged history.
    let item_only = plan(&config(&[], &["npu_amd_usage"]), &hw);
    assert!(item_only.jobs.iter().any(|spec| spec.id == npu_job()));
    assert!(
        item_only
            .jobs
            .iter()
            .all(|spec| spec.id.kind != JobKind::AmdNpuHistory)
    );
    let mut replaced = hw.clone();
    let source = replaced.amd_npu.as_mut().expect("NPU source");
    source.accel_pci = String::from("0000:c5:00.1");
    let identity = source.identity();
    let replaced = plan(&chart_config(), &replaced);
    assert!(
        replaced
            .jobs
            .iter()
            .any(|spec| spec.id == history_job(&identity))
    );
    assert!(
        replaced
            .jobs
            .iter()
            .all(|spec| spec.id != history_job(IDENTITY))
    );
}

#[test]
fn chart_history_survives_item_removal_and_clears_with_the_chart() {
    let tree = TempTree::new();
    let mut hw = hardware(&tree);
    let mut owners = TestOwners::default();
    let mut readings = DisplaySnapshot::default();
    let history = history_job(IDENTITY);

    let mut chart_and_item = chart_config();
    chart_and_item.tooltip = panel_section(&["npu_amd_power"]);
    for cfg in [&chart_config(), &chart_and_item, &chart_config()] {
        assert_eq!(
            run(&tree, &mut owners, &mut hw, cfg, &mut readings),
            CompletionKind::Captured
        );
        assert_eq!(
            run_job(
                history.clone(),
                &tree,
                &mut owners,
                &mut hw,
                cfg,
                &mut readings
            ),
            CompletionKind::Captured
        );
    }
    assert_eq!(readings.npu_amd_usage_history, [96, 96, 96]);
    assert_eq!(owners.amd_npu.usage_history(), [96, 96, 96]);

    let item_only = config(&[], &["npu_amd_usage"]);
    assert_eq!(
        run(&tree, &mut owners, &mut hw, &item_only, &mut readings),
        CompletionKind::Captured
    );
    assert_eq!(readings.npu_amd_usage, Some(96));
    assert!(owners.amd_npu.usage_history().is_empty());

    assert_eq!(
        run_job(
            history,
            &tree,
            &mut owners,
            &mut hw,
            &chart_config(),
            &mut readings
        ),
        CompletionKind::Captured
    );
    assert_eq!(readings.npu_amd_usage_history, [96]);
    let nothing = config(&["cpu_usage"], &[]);
    assert_eq!(
        run(&tree, &mut owners, &mut hw, &nothing, &mut readings),
        CompletionKind::ConfirmedAbsent
    );
    assert!(owners.amd_npu.usage_history().is_empty());
    assert_eq!(readings.npu_amd_usage, None);
}

#[test]
fn chart_history_resets_when_the_npu_source_changes() {
    let tree = TempTree::new();
    let mut hw = hardware(&tree);
    let mut owners = TestOwners::default();
    let mut readings = DisplaySnapshot::default();
    let cfg = chart_config();
    for _ in 0..2 {
        run(&tree, &mut owners, &mut hw, &cfg, &mut readings);
        run_job(
            history_job(IDENTITY),
            &tree,
            &mut owners,
            &mut hw,
            &cfg,
            &mut readings,
        );
    }
    assert_eq!(owners.amd_npu.usage_history(), [96, 96]);

    let mut replaced = hw.amd_npu.clone().expect("NPU source");
    replaced.accel_pci = String::from("0000:c5:00.1");
    let identity = replaced.identity();
    hw.amd_npu = Some(replaced);
    assert_eq!(
        run_job(
            history_job(IDENTITY),
            &tree,
            &mut owners,
            &mut hw,
            &cfg,
            &mut readings
        ),
        CompletionKind::ConfirmedAbsent
    );
    assert!(owners.amd_npu.usage_history().is_empty());
    assert_eq!(
        run_job(
            history_job(&identity),
            &tree,
            &mut owners,
            &mut hw,
            &cfg,
            &mut readings
        ),
        CompletionKind::Baseline
    );
    assert!(owners.amd_npu.usage_history().is_empty());
}
