use super::*;
use crate::domain::readings::{AmdGpuMemoryPaths, AmdGpuMemoryReading, AmdGpuSource};

pub(super) fn source() -> AmdGpuSource {
    AmdGpuSource {
        pci_identity: String::from("0000:c3:00.0"),
        usage_path: Some(PathBuf::from("/gpu/gpu_busy_percent")),
        codec_usage_path: Some(PathBuf::from("/gpu/vcn_busy_percent")),
        memory_paths: Some(AmdGpuMemoryPaths {
            used: PathBuf::from("/gpu/mem_info_vram_used"),
            total: PathBuf::from("/gpu/mem_info_vram_total"),
        }),
        freq_path: Some(PathBuf::from("/gpu/hwmon/freq1_input")),
        temp_path: Some(PathBuf::from("/gpu/hwmon/temp1_input")),
        power_path: Some(PathBuf::from("/gpu/hwmon/power1_average")),
        fan_speed_path: Some(PathBuf::from("/gpu/hwmon/fan1_input")),
    }
}

fn config() -> Config {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut cfg =
        load_config(Some(&root.join("config/config.toml")), Some(false)).expect("shipped config");
    let items = crate::domain::Metric::all()
        .iter()
        .filter(|metric| metric.as_str().starts_with("gpu_amd_"))
        .map(ToString::to_string)
        .collect();
    let surface = ConfigSurface {
        sections: vec![Section {
            key: String::from("amd"),
            title: String::new(),
            items,
        }],
        glyphs: true,
    };
    cfg.panel = surface.clone();
    cfg.tooltip = surface;
    cfg
}

#[test]
fn amd_rows_preserve_units_thresholds_and_memory_context() {
    let cfg = config();
    let hw = full_hw();
    let mut readings = full_readings();
    readings.gpu_amd_fan_speed = Some(0);
    let formatter = PanelFormatter::new(&cfg, &hw);
    let html = formatter.format_tooltip(&readings, "");
    for text in [
        "AMD GPU",
        "2800 MHz",
        "120 W",
        "65°C",
        "off",
        "4096 / 16384 MiB",
        "crit",
        "warn",
        "active",
    ] {
        assert!(html.contains(text), "missing {text}: {html}");
    }
    assert!(!html.contains("<table"));
    let panel = formatter.format_panel(&readings, "");
    assert!(panel.contains("2800 MHz"));
    assert!(!panel.contains("MiB"));
    readings.gpu_amd_fan_speed = Some(1800);
    assert!(formatter.format_tooltip(&readings, "").contains("1800 RPM"));
    readings.gpu_amd_codec_usage = Some(1);
    let codec = formatter
        .build_entries(&readings, true)
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Row(row) => Some(row),
            _ => None,
        })
        .find(|row| row.iter().any(|cell| cell.text.contains("codec")))
        .expect("codec row");
    assert!(!codec.iter().any(|cell| {
        cell.css_class
            .as_deref()
            .is_some_and(|classes| classes.split_whitespace().any(|class| class == "active"))
    }));
}

#[test]
fn amd_gates_each_source_and_omits_missing_readings() {
    let cfg = config();
    let readings = full_readings();
    let mut hw = full_hw();
    hw.amd_gpu = None;
    assert!(
        PanelFormatter::new(&cfg, &hw)
            .build_entries(&readings, true)
            .is_empty()
    );
    hw.amd_gpu = Some(source());
    assert!(
        PanelFormatter::new(&cfg, &hw)
            .build_entries(&DisplaySnapshot::default(), true)
            .is_empty()
    );
    for metric in crate::domain::Metric::all()
        .iter()
        .filter(|metric| metric.as_str().starts_with("gpu_amd_"))
    {
        let mut missing = source();
        match metric {
            crate::domain::Metric::GpuAmdUsage => missing.usage_path = None,
            crate::domain::Metric::GpuAmdCodecUsage => missing.codec_usage_path = None,
            crate::domain::Metric::GpuAmdMemUsage => missing.memory_paths = None,
            crate::domain::Metric::GpuAmdFreq => missing.freq_path = None,
            crate::domain::Metric::GpuAmdTemp => missing.temp_path = None,
            crate::domain::Metric::GpuAmdPower => missing.power_path = None,
            crate::domain::Metric::GpuAmdFanSpeed => missing.fan_speed_path = None,
            _ => unreachable!(),
        }
        hw.amd_gpu = Some(missing);
        let html = PanelFormatter::new(&cfg, &hw).format_tooltip(&readings, "");
        assert!(!html.contains(metric.as_str()), "{metric}");
        assert_eq!(
            PanelFormatter::new(&cfg, &hw)
                .build_entries(&readings, true)
                .len(),
            6
        );
    }
}

#[test]
fn amd_width_covers_extreme_values_and_reserves_before_first_sample() {
    let cfg = config();
    let hw = full_hw();
    let formatter = PanelFormatter::new(&cfg, &hw);
    let canonical = formatter.canonical_width(&DisplaySnapshot::default());
    let mut readings = full_readings();
    readings.gpu_amd_freq = Some(u32::MAX);
    readings.gpu_amd_power = Some(u32::MAX);
    readings.gpu_amd_fan_speed = Some(u32::MAX);
    readings.gpu_amd_temp = Some(-273);
    readings.gpu_amd_mem_usage = Some(AmdGpuMemoryReading {
        used_bytes: u64::MAX,
        total_bytes: u64::MAX,
    });
    let width = global_width_of(
        &group_rows_into_blocks(formatter.build_entries(&readings, true)),
        0,
    );
    assert!(canonical >= width);
    assert!(
        formatter
            .format_tooltip(&readings, "")
            .contains("99999+ RPM")
    );
}
