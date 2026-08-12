#![allow(clippy::expect_used)]

use super::*;
use crate::config::Config;
use crate::domain::{TopProcessDetails, TopProcessSummary};

fn hw() -> HardwareInventory {
    HardwareInventory {
        cpu_count: 8,
        net_device: Some(String::from("wlan0")),
        ..HardwareInventory::default()
    }
}

fn readings() -> DisplaySnapshot {
    DisplaySnapshot {
        cpu_core_usage: Some(vec![10, 20]),
        cpu_core_history: Some(vec![vec![10, 20, 30, 40], vec![15, 25, 35, 45]]),
        top_process: Some(vec![TopProcessSummary {
            command: String::from("plasmashell"),
            cpu_percent: 12,
        }]),
        top_process_full: Some(vec![
            TopProcessDetails {
                pid: 1234,
                command: String::from("plasmashell --replace"),
                cpu_percent: 12,
                memory_percent: 3.2,
            },
            TopProcessDetails {
                pid: 999_999,
                command: String::from("firefox"),
                cpu_percent: 70,
                memory_percent: 12.4,
            },
        ]),
        cpu_history: vec![10, 40, 60, 30],
        mem_history: vec![20, 30, 50, 40],
        gpu_usage_history: vec![15, 35, 55, 45],
        gpu_dec_history: vec![0, 5, 10, 5],
        net_down_history: vec![1000, 2000, 3000],
        net_up_history: vec![500, 1000, 1500],
        cpu_usage: Some(42),
        mem_usage: Some(55),
        net_down_bps: Some(3_000_000),
        net_up_bps: Some(1_500_000),
        ..DisplaySnapshot::default()
    }
}

fn normalize_png_uris(mut html: String) -> String {
    const PREFIX: &str = "data:image/png;base64,";
    while let Some(start) = html.find(PREFIX) {
        let end = html[start..]
            .find('"')
            .map_or(html.len(), |offset| start + offset);
        html.replace_range(start..end, "<PNG>");
    }
    html
}

#[test]
fn format_page_wraps_tooltip_shell() {
    let cfg = Config::default();
    let hardware = hw();
    let formatter = PageFormatter::new(&cfg, &hardware);

    let html = formatter.format_page("<div class=\"page\">x</div>", "", "<h>", "<f>");

    assert_eq!(
        html,
        r#"<div class="tooltip"><h><div class="page">x</div><f></div>"#
    );
}

#[test]
fn format_cpu_cores_uses_no_data_message_when_absent() {
    let cfg = Config::default();
    let hardware = hw();
    let formatter = PageFormatter::new(&cfg, &hardware);

    let html = formatter.format_cpu_cores(&DisplaySnapshot::default(), "", "", None);

    assert_eq!(
        html,
        r#"<div class="tooltip"><div class="page">cpu cores: no data yet</div></div>"#
    );
}

#[test]
fn format_cpu_cores_and_top_process_emit_expected_shell_bits() {
    let mut cfg = Config::default();
    cfg.display.tooltip_width = 34;
    let hardware = hw();
    let formatter = PageFormatter::new(&cfg, &hardware);
    let readings = readings();

    let cpu = formatter.format_cpu_cores(
        &readings,
        "",
        "<header>",
        Some(&|width| format!("<footer>{width}</footer>")),
    );
    let proc = formatter.format_top_process(
        &readings,
        "",
        "<header>",
        Some(&|width| format!("<footer>{width}</footer>")),
    );

    assert_eq!(
        cpu,
        "<div class=\"tooltip\"><header><div class=\"page\"><span class=\"label\">Core 0:&nbsp;</span>⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀<span class=\"grad-cpu-1\">⣀</span><span class=\"grad-cpu-3\">⣤</span><span class=\"gap\">&nbsp;</span><span class=\"val good\">&nbsp;10%</span><br><span class=\"label\">Core 1:&nbsp;</span>⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀⠀<span class=\"grad-cpu-2\">⣀</span><span class=\"grad-cpu-3\">⣤</span><span class=\"gap\">&nbsp;</span><span class=\"val good\">&nbsp;20%</span></div><footer>34</footer></div>"
    );
    assert_eq!(
        proc,
        "<div class=\"tooltip\"><header><div class=\"page\">&nbsp;&nbsp;&nbsp;<span class=\"label\">PID</span>&nbsp;&nbsp;<span class=\"label\">COMMAND</span>&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<span class=\"label\">%CPU</span>&nbsp;&nbsp;<span class=\"label\">%MEM</span><br>&nbsp;&nbsp;<span class=\"label\">1234</span>&nbsp;&nbsp;plasmashell --…&nbsp;&nbsp;&nbsp;<span class=\"val good\">12</span>&nbsp;&nbsp;&nbsp;<span class=\"val good\">3.2</span><br><span class=\"label\">999999</span>&nbsp;&nbsp;firefox&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;<span class=\"val crit\">70</span>&nbsp;&nbsp;<span class=\"val good\">12.4</span></div><footer>34</footer></div>"
    );
}

#[test]
fn format_graphs_embeds_pngs_and_legends() {
    let mut cfg = Config::default();
    cfg.display.tooltip_width = 35;
    cfg.pages.graph_width = 120;
    let hardware = hw();
    let formatter = PageFormatter::new(&cfg, &hardware);
    let readings = readings();

    let html = formatter.format_graphs(
        &readings,
        "",
        "<header>",
        Some(&|width| format!("<footer>{width}</footer>")),
    );

    assert!(html.contains("data:image/png;base64,"));
    assert!(html.contains("CPU usage"));
    assert!(html.contains("Memory usage"));
    assert!(html.contains("Download"));
    assert!(html.contains("Upload"));
    assert!(html.contains("<footer>35</footer>"));
    assert_eq!(html.matches("data:image/png;base64,").count(), 3);
    assert_eq!(html.matches("width=\"120\" height=\"84\"").count(), 3);
    assert_eq!(
        normalize_png_uris(html),
        r#"<div class="tooltip"><header><div style="font-size:6px">&nbsp;</div><div><img src="<PNG>" width="120" height="84"></div><div class="page"><span style="color:rgb(61,174,233)">●</span>&nbsp;<span class="label">CPU usage:</span>&nbsp;<span class="val good">42%</span></div><div style="font-size:16px">&nbsp;</div><div><img src="<PNG>" width="120" height="84"></div><div class="page"><span style="color:rgb(163,102,255)">●</span>&nbsp;<span class="label">Memory usage:</span>&nbsp;<span class="val warn">55%</span></div><div style="font-size:16px">&nbsp;</div><div><img src="<PNG>" width="120" height="84"></div><div class="page"><span style="color:rgb(26,188,156)">●</span>&nbsp;<span class="label">Download:</span>&nbsp;<span class="val">3M</span><br><span style="color:rgb(231,76,60)">●</span>&nbsp;<span class="label">Upload:</span>&nbsp;<span class="val">1M</span></div><footer>35</footer></div>"#
    );
}

#[test]
fn format_top_process_escapes_commands_and_caps_rows() {
    let mut cfg = Config::default();
    cfg.display.tooltip_width = 34;
    let hardware = hw();
    let formatter = PageFormatter::new(&cfg, &hardware);
    let rows = (0..16)
        .map(|index| TopProcessDetails {
            pid: 1000 + index,
            command: if index == 0 {
                String::from("<worker>")
            } else {
                format!("worker-{index}")
            },
            cpu_percent: 1,
            memory_percent: 1.0,
        })
        .collect();
    let readings = DisplaySnapshot {
        top_process_full: Some(rows),
        ..DisplaySnapshot::default()
    };

    let html = formatter.format_top_process(&readings, "", "", None);

    assert!(html.contains("&lt;worker&gt;"));
    assert!(!html.contains("worker-15"));
    assert!(!html.contains("<table"));
}

#[test]
fn format_graphs_prefers_nvidia_and_omits_absent_network() {
    let mut cfg = Config::default();
    cfg.pages.graph_width = 120;
    cfg.thresholds.gpu_nvidia_usage = vec![10, 20];
    cfg.thresholds.gpu_intel_usage = vec![80, 90];
    let hardware = HardwareInventory {
        has_nvidia: true,
        intel_gpu_pci: Some(String::from("0000:00:02.0")),
        ..HardwareInventory::default()
    };
    let formatter = PageFormatter::new(&cfg, &hardware);
    let readings = DisplaySnapshot {
        cpu_history: vec![10],
        mem_history: vec![20],
        gpu_usage_history: vec![60],
        gpu_dec_history: vec![5],
        gpu_usage: Some(60),
        gpu_dec: Some(5),
        gpu_intel_usage: Some(1),
        gpu_intel_dec_usage: Some(1),
        ..DisplaySnapshot::default()
    };

    let html = formatter.format_graphs(&readings, "", "", None);

    assert_eq!(html.matches("data:image/png;base64,").count(), 3);
    assert!(html.contains(r#"<span class="val crit">60%</span>"#));
    assert!(!html.contains("Download"));
}

#[test]
fn format_graphs_handles_present_sources_before_histories_exist() {
    let cfg = Config::default();
    let hardware = HardwareInventory {
        has_nvidia: true,
        net_device: Some(String::from("eth0")),
        ..HardwareInventory::default()
    };
    let formatter = PageFormatter::new(&cfg, &hardware);

    let html = formatter.format_graphs(&DisplaySnapshot::default(), "", "", None);

    assert_eq!(html.matches("data:image/png;base64,").count(), 4);
    assert!(html.contains("GPU usage"));
    assert!(html.contains("Download"));
}

#[test]
fn graph_value_helpers_match_threshold_classes() {
    assert!(graph_value_band(None, Some((50, 70))).contains(EMPTY_VALUE));
    assert_eq!(
        graph_value_active(Some(0), Some(1)),
        r#"<span class="val ">0%</span>"#
    );
}
