#![allow(clippy::expect_used, clippy::too_many_lines)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use crate::config::{
    BarConfig, BrailleConfig, ColumnConfig, Config, Section, SparkConfig, Surface as ConfigSurface,
    apply_canonical_width, load_config,
};
use crate::domain::{
    BatteryPeripheralReading, BatteryState, BatterySystemReading, DiskUsageReading,
    DisplaySnapshot, HardwareInventory, LoadAverage, SmartDisk, TopProcessSummary,
};

use super::super::cells::{SSID_MAX, middle_ellipsis, net_fmt};
use super::super::model::{Entry, group_rows_into_blocks};
use super::super::mono::global_width_of;
use super::PanelFormatter;

mod amd;

fn bare_hw() -> HardwareInventory {
    HardwareInventory {
        net_device: Some(String::from("enp0s3")),
        disk_io_device: Some(String::from("sda2")),
        cpu_count: 2,
        ..HardwareInventory::default()
    }
}

fn full_hw() -> HardwareInventory {
    HardwareInventory {
        cpu_temp_path: Some(PathBuf::from("/x")),
        cpu_freq_path: Some(PathBuf::from("/x")),
        hd_temp_paths: BTreeMap::from([
            (String::from("nvme0"), PathBuf::from("/x")),
            (String::from("sda"), PathBuf::from("/x")),
        ]),
        fan_paths: BTreeMap::from([
            (String::from("1"), PathBuf::from("/x")),
            (String::from("2"), PathBuf::from("/x")),
        ]),
        battery_sys_ids: vec![String::from("/BAT0")],
        has_nvidia: true,
        amd_gpu: Some(amd::source()),
        intel_gpu_freq_path: Some(PathBuf::from("/x")),
        intel_gpu_pci: Some(String::from("0000:00:02.0")),
        net_device: Some(String::from("wlan0")),
        disk_io_device: Some(String::from("nvme0n1")),
        cpu_count: 8,
        cpu_turbo_supported: true,
        has_backlight: true,
        has_wifi: true,
        battery_mouse_id: Some(String::from("/m")),
        battery_kbd_id: Some(String::from("/k")),
        disk_smart_drives: BTreeMap::from([
            (
                String::from("nvme0"),
                SmartDisk {
                    object_path: String::from("/d0"),
                    interface: crate::domain::DiskSmartInterface::Nvme,
                    rotational: false,
                },
            ),
            (
                String::from("sda"),
                SmartDisk {
                    object_path: String::from("/d1"),
                    interface: crate::domain::DiskSmartInterface::Ata,
                    rotational: true,
                },
            ),
        ]),
        ..HardwareInventory::default()
    }
}

fn full_readings() -> DisplaySnapshot {
    DisplaySnapshot {
        cpu_usage: Some(73),
        cpu_temp: Some(55),
        cpu_freq_mhz: Some(3200.0),
        cpu_turbo: Some(true),
        cpu_history: [10, 20, 30, 40, 50, 60, 70, 80, 73, 65].repeat(2),
        mem_history: [15, 25, 35, 45, 55, 42, 38, 44, 50, 42].repeat(2),
        uptime_seconds: Some(123_456),
        load_average: Some(LoadAverage {
            one: 1.2,
            five: 0.9,
            fifteen: 0.7,
        }),
        top_process: Some(vec![
            TopProcessSummary {
                command: String::from("plasmashell"),
                cpu_percent: 12,
            },
            TopProcessSummary {
                command: String::from("firefox"),
                cpu_percent: 8,
            },
        ]),
        mem_usage: Some(42),
        mem_used_gib: Some(13),
        mem_total_gib: Some(32),
        swap_usage: Some(10),
        net_up_bps: Some(500_000),
        net_down_bps: Some(2_000_000),
        net_device: Some(String::from("wlan0")),
        ip_address: Some(String::from("192.168.1.5")),
        wifi_ssid: Some(String::from("MyWifi")),
        wifi_signal_percent: Some(80),
        disk_read_bps: Some(1_500_000),
        disk_write_bps: Some(800_000),
        disk_usage: BTreeMap::from([
            (
                String::from("/"),
                Some(DiskUsageReading {
                    percent: 50,
                    used_gib: 100,
                    total_gib: 200,
                }),
            ),
            (
                String::from("/mnt/data"),
                Some(DiskUsageReading {
                    percent: 70,
                    used_gib: 700,
                    total_gib: 1000,
                }),
            ),
        ]),
        disk_smart: BTreeMap::from([
            (String::from("nvme0"), Some(true)),
            (String::from("sda"), Some(true)),
        ]),
        hd_temps: BTreeMap::from([
            (String::from("nvme0"), Some(45)),
            (String::from("sda"), Some(50)),
        ]),
        fan_speeds: BTreeMap::from([
            (String::from("1"), Some(1200)),
            (String::from("2"), Some(0)),
        ]),
        battery_sys: vec![BatterySystemReading {
            id: String::from("/BAT0"),
            charge_percent: 80,
            rate_watts: 15,
            state: BatteryState::Discharging,
            charge_limit_percent: Some(80),
        }],
        battery_mouse: Some(BatteryPeripheralReading {
            name: String::from("Logi Mouse"),
            charge_percent: 90,
        }),
        battery_kbd: Some(BatteryPeripheralReading {
            name: String::from("Logi Kbd"),
            charge_percent: 85,
        }),
        gpu_amd_usage: Some(73),
        gpu_amd_codec_usage: Some(25),
        gpu_amd_mem_usage: Some(crate::domain::readings::AmdGpuMemoryReading {
            used_bytes: 4 << 30,
            total_bytes: 16 << 30,
        }),
        gpu_amd_freq: Some(2800),
        gpu_amd_temp: Some(65),
        gpu_amd_power: Some(120),
        gpu_amd_fan_speed: Some(1800),
        gpu_temp: Some(60),
        gpu_usage: Some(30),
        gpu_mem: Some(40),
        gpu_dec: Some(5),
        gpu_fan: Some(25),
        gpu_intel_freq: Some(900),
        gpu_intel_usage: Some(20),
        gpu_intel_dec_usage: Some(2),
        screen_brightness: Some(75),
        system_updates: Some(3),
        server_ok: Some(true),
        ..DisplaySnapshot::default()
    }
}

fn canonical_guard_cfg() -> Config {
    let mut cfg = Config::default();
    cfg.pages.order = Vec::new();
    cfg.tooltip = ConfigSurface {
        sections: vec![Section {
            key: String::from("io"),
            title: String::from("IO"),
            items: vec![
                String::from("net_device_ip"),
                String::from("wifi_ssid_signal"),
                String::from("load_avg"),
                String::from("uptime"),
                String::from("cpu_freq"),
                String::from("cpu_temp"),
            ],
        }],
        glyphs: true,
    };
    cfg
}

#[test]
fn net_and_label_helpers_match_python_behavior() {
    assert_eq!(net_fmt(0), "0");
    assert_eq!(net_fmt(500_000), "500K");
    assert_eq!(net_fmt(2_500_000), "2M");
    assert_eq!(
        middle_ellipsis("FRITZ!Box 7590 Guest", SSID_MAX)
            .chars()
            .count(),
        SSID_MAX
    );
}

#[test]
fn canonical_width_exceeds_short_content() {
    let cfg = canonical_guard_cfg();
    let hw = HardwareInventory {
        net_device: Some(String::from("wlan0")),
        has_wifi: true,
        cpu_temp_path: Some(PathBuf::from("/x")),
        cpu_count: 8,
        ..HardwareInventory::default()
    };
    let formatter = PanelFormatter::new(&cfg, &hw);
    let readings = DisplaySnapshot {
        net_device: Some(String::from("wlan0")),
        ip_address: Some(String::from("10.0.0.1")),
        wifi_ssid: Some(String::from("Home")),
        wifi_signal_percent: Some(50),
        load_average: Some(LoadAverage {
            one: 0.1,
            five: 0.2,
            fifteen: 0.3,
        }),
        uptime_seconds: Some(60),
        cpu_freq_mhz: Some(800.0),
        cpu_temp: Some(40),
        ..DisplaySnapshot::default()
    };
    let actual = global_width_of(
        &group_rows_into_blocks(formatter.build_entries(&readings, true)),
        0,
    );
    assert!(formatter.canonical_width(&readings) > actual);
}

#[test]
fn canonical_width_covers_every_tooltip_item() {
    let hw = full_hw();
    let lo = full_readings();
    let hi = PanelFormatter::with_now_unix(&Config::default(), &hw, 1_000_000).maxed_readings(&lo);
    let tokens = crate::domain::list_items()
        .into_iter()
        .filter(|(_, placement)| *placement != "panel only")
        .map(|(token, _)| token)
        .collect::<Vec<_>>();

    let mut failures = Vec::new();
    for token in tokens {
        let mut cfg = Config::default();
        cfg.pages.order = Vec::new();
        cfg.tooltip = ConfigSurface {
            sections: vec![Section {
                key: String::from("s"),
                title: String::from("S"),
                items: vec![token.clone()],
            }],
            glyphs: true,
        };
        let formatter = PanelFormatter::with_now_unix(&cfg, &hw, 1_000_000);
        let wide = global_width_of(
            &group_rows_into_blocks(formatter.build_entries(&hi, true)),
            0,
        );
        if wide == 0 {
            continue;
        }
        let canonical = formatter.canonical_width(&lo);
        if canonical < wide {
            failures.push(format!(
                "{token}: canonical {canonical} < wide render {wide}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn available_sections_collapse_and_panel_omits_titles() {
    let mut cfg = Config::default();
    let sections = vec![
        Section {
            key: String::from("live"),
            title: String::from("Live"),
            items: vec![String::from("cpu_usage"), String::from("mem_usage")],
        },
        Section {
            key: String::from("thermal"),
            title: String::from("Thermal"),
            items: vec![String::from("cpu_temp"), String::from("fan_speed")],
        },
        Section {
            key: String::from("load"),
            title: String::from("Load"),
            items: vec![String::from("uptime"), String::from("load_avg")],
        },
    ];
    cfg.tooltip = ConfigSurface {
        sections: sections.clone(),
        glyphs: true,
    };
    cfg.panel = ConfigSurface {
        sections,
        glyphs: true,
    };

    let hw = bare_hw();
    let formatter = PanelFormatter::new(&cfg, &hw);
    let readings = DisplaySnapshot {
        cpu_usage: Some(10),
        mem_usage: Some(20),
        uptime_seconds: Some(60),
        load_average: Some(LoadAverage {
            one: 0.1,
            five: 0.2,
            fifteen: 0.3,
        }),
        ..DisplaySnapshot::default()
    };

    let tooltip_entries = formatter.build_entries(&readings, true);
    let panel_entries = formatter.build_entries(&readings, false);
    let tooltip_titles = tooltip_entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Row(row) if row.len() == 1 && row[0].css_class.as_deref() == Some("title") => {
                Some(row[0].text.clone())
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(
        tooltip_titles,
        vec![String::from("Live"), String::from("Load")]
    );
    assert!(!matches!(
        tooltip_entries.first(),
        Some(Entry::Separator(_))
    ));
    assert!(panel_entries.iter().all(|entry| {
        !matches!(entry, Entry::Separator(_))
            && !matches!(entry, Entry::Row(row) if row.len() == 1 && row[0].css_class.as_deref() == Some("title"))
    }));
}

#[test]
fn tooltip_and_panel_goldens_match_python_snapshots() {
    // Historical snapshots describe an NVIDIA/Intel machine without AMDGPU.
    let hw = HardwareInventory {
        amd_gpu: None,
        ..full_hw()
    };
    let readings = full_readings();
    let cases = [
        ("panel_v", true, true),
        ("panel_h", false, true),
        ("tooltip", true, false),
    ];

    for (name, vertical, panel) in cases {
        let mut cfg = load_config(None, Some(vertical)).expect("load shipped config");
        reset_panel_autofit_fields(&mut cfg);
        let formatter = PanelFormatter::with_now_unix(&cfg, &hw, 1_000_000);
        if !panel {
            let canonical = formatter.canonical_width(&readings) as i32;
            apply_canonical_width(&mut cfg, canonical);
        }
        let formatter = PanelFormatter::with_now_unix(&cfg, &hw, 1_000_000);
        let html = if panel {
            formatter.format_panel(&readings, "")
        } else {
            formatter.format_tooltip(&readings, "")
        };
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/golden")
            .join(format!("{name}.html"));
        let expected = std::fs::read_to_string(path).expect("read golden");
        assert_eq!(html, expected, "golden mismatch for {name}");
    }
}

fn reset_panel_autofit_fields(cfg: &mut Config) {
    let defaults = Config::default();
    cfg.display.panel_font_size = defaults.display.panel_font_size;
    cfg.display.panel_min_width = defaults.display.panel_min_width;
    cfg.bar_panel = BarConfig {
        width: BarConfig::default().width,
        height: 3,
    };
    cfg.column_panel = ColumnConfig::default();
    cfg.spark_panel = SparkConfig::default();
    cfg.braille_panel = BrailleConfig::default();
}
