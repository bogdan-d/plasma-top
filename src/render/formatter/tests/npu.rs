use super::*;
use crate::domain::readings::AmdNpuSource;

pub(super) fn source() -> AmdNpuSource {
    AmdNpuSource {
        accel_pci: String::from("0000:c4:00.1"),
        metrics_pci: String::from("0000:c3:00.0"),
        metrics_path: PathBuf::from("/gpu/gpu_metrics"),
    }
}

const ITEMS: [&str; 3] = ["npu_amd_usage", "npu_amd_power", "npu_amd_freq"];

fn config() -> Config {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut cfg =
        load_config(Some(&root.join("config/config.toml")), Some(false)).expect("shipped config");
    let surface = ConfigSurface {
        sections: vec![Section {
            key: String::from("npu"),
            title: String::new(),
            items: ITEMS.iter().map(|item| (*item).to_owned()).collect(),
        }],
        glyphs: true,
    };
    cfg.panel = surface.clone();
    cfg.tooltip = surface;
    cfg
}

fn npu_readings(usage: i32, power_mw: u16, freq: u16) -> DisplaySnapshot {
    DisplaySnapshot {
        npu_amd_usage: Some(usage),
        npu_amd_power_mw: Some(power_mw),
        npu_amd_freq: Some(freq),
        ..DisplaySnapshot::default()
    }
}

#[test]
fn active_npu_rows_show_one_decimal_watts_clock_and_busy_threshold() {
    let cfg = config();
    let hw = full_hw();
    let formatter = PanelFormatter::new(&cfg, &hw);
    let html = formatter.format_tooltip(&npu_readings(96, 3352, 1810), "");
    for text in ["AMD NPU usage", "96%", "crit", "3.4 W", "1810 MHz"] {
        assert!(html.contains(text), "missing {text}: {html}");
    }
    assert!(!html.contains("<table"));
    let panel = formatter.format_panel(&npu_readings(96, 3352, 1810), "");
    assert!(panel.contains("3.4 W") && panel.contains("1810 MHz"));
    assert!(
        formatter
            .format_tooltip(&npu_readings(60, 1149, 800), "")
            .contains("warn")
    );
}

#[test]
fn idle_npu_reads_zero_and_a_decayed_clock_reads_off() {
    let cfg = config();
    let hw = full_hw();
    let formatter = PanelFormatter::new(&cfg, &hw);
    let html = formatter.format_tooltip(&npu_readings(0, 0, 0), "");
    for text in ["0%", "0.0 W", "off"] {
        assert!(html.contains(text), "missing {text}: {html}");
    }
    assert!(!html.contains("MHz"));
}

#[test]
fn npu_rows_are_omitted_without_a_source_or_a_reading() {
    let cfg = config();
    let mut hw = full_hw();
    hw.amd_npu = None;
    assert!(
        PanelFormatter::new(&cfg, &hw)
            .build_entries(&npu_readings(96, 3352, 1810), true)
            .is_empty()
    );
    assert!(
        PanelFormatter::new(&cfg, &full_hw())
            .build_entries(&DisplaySnapshot::default(), true)
            .is_empty()
    );
}

#[test]
fn canonical_width_covers_npu_extremes_before_the_first_reading() {
    let cfg = config();
    let hw = full_hw();
    let formatter = PanelFormatter::with_now_unix(&cfg, &hw, 1_000_000);
    let extreme = npu_readings(100, 65_534, 65_534);
    let html = formatter.format_tooltip(&extreme, "");
    assert!(
        html.contains("65.5 W") && html.contains("65534 MHz"),
        "{html}"
    );
    let wide = global_width_of(
        &group_rows_into_blocks(formatter.build_entries(&extreme, true)),
        0,
    );
    assert!(formatter.canonical_width(&DisplaySnapshot::default()) >= wide);
}

#[test]
fn npu_rows_match_goldens() {
    let cfg = config();
    let hw = full_hw();
    let formatter = PanelFormatter::with_now_unix(&cfg, &hw, 1_000_000);
    let active = npu_readings(96, 3352, 1810);
    for (name, html) in [
        ("npu_tooltip_active", formatter.format_tooltip(&active, "")),
        (
            "npu_tooltip_idle",
            formatter.format_tooltip(&npu_readings(0, 0, 0), ""),
        ),
        ("npu_panel_active", formatter.format_panel(&active, "")),
    ] {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(format!("{name}.html"));
        let expected = std::fs::read_to_string(path).expect("read golden");
        assert_eq!(html, expected, "golden mismatch for {name}");
    }
}
