//! One-shot render, probe, profiling, and item-list diagnostics.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use crate::adapters::{ProductionClock, ProductionCommandRunner, ProductionDbusFacade};
use crate::cli::{PanelLayout, RenderCommand, RenderComponent, RenderFormat};
use crate::config::{Config, apply_canonical_width, load_config, resolve_style};
use crate::daemon::{executable_lookup, plasma_is_light, read_css, render_page, render_page_id};
use crate::domain::boundary::FilesystemRoots;
use crate::domain::readings::{HardwareSnapshot, ReadingsSnapshot};
use crate::domain::registry::list_items;
use crate::domain::state::DaemonStateSnapshot;
use crate::error::Result;
use crate::page_commands::{PageCommandCache, build_pages};
use crate::render::PanelFormatter;
use crate::sensors::hid::BoltHidFacade;
use crate::sensors::process::read_top_process_page;
use crate::sensors::{CollectCtx, CollectorState, Timings, collect, discover_hardware};

const RENDER_PANEL_FILE: &str = "/tmp/plasma-top_render_panel.html";
const RENDER_TOOLTIP_FILE: &str = "/tmp/plasma-top_render_tooltip.html";

struct OneShot {
    cfg: Config,
    hw: HardwareSnapshot,
    readings: ReadingsSnapshot,
    commands: ProductionCommandRunner,
    roots: FilesystemRoots,
    clock: ProductionClock,
}

fn collect_one_shot(
    config_path: Option<&Path>,
    vertical: Option<bool>,
    page: Option<&str>,
) -> Result<OneShot> {
    let mut cfg = load_config(config_path, vertical)?;
    if let Some(page) = page
        && page != "full"
        && !cfg.pages.order.iter().any(|known| known == page)
    {
        cfg.pages.order.push(page.to_owned());
    }
    let roots = FilesystemRoots::default();
    let mut commands = ProductionCommandRunner;
    let mut dbus = ProductionDbusFacade::default();
    let cpu_count = thread::available_parallelism().map_or(1, std::num::NonZero::get);
    let mut hw = discover_hardware(
        &roots.sys_root,
        &roots.proc_root,
        &cfg,
        &mut dbus,
        &mut commands,
        cpu_count,
    );
    let clock = ProductionClock::default();
    let mut collector = CollectorState::default();
    let mut state = DaemonStateSnapshot::default();
    let mut bolt = BoltHidFacade::default();
    for warm in [false, true] {
        if warm {
            thread::sleep(Duration::from_secs(1));
        }
        let mut ctx = CollectCtx::new(&roots, &mut commands, &mut dbus, clock.snapshot());
        ctx.bolt = Some(&mut bolt);
        let readings = collect(&mut collector, &mut state, &mut hw, &cfg, &mut ctx, None);
        if warm {
            let mut readings = readings;
            if page == Some("processes") {
                let _ = read_top_process_page(
                    &roots.proc_root,
                    &mut collector.process,
                    clock.snapshot(),
                );
                thread::sleep(Duration::from_millis(500));
                readings.top_process_full = read_top_process_page(
                    &roots.proc_root,
                    &mut collector.process,
                    clock.snapshot(),
                )
                .or(readings.top_process_full);
            }
            let width = PanelFormatter::new(&cfg, &hw).canonical_width(&readings);
            apply_canonical_width(&mut cfg, i32::try_from(width).unwrap_or(i32::MAX));
            return Ok(OneShot {
                cfg,
                hw,
                readings,
                commands,
                roots,
                clock,
            });
        }
    }
    unreachable!("two fixed warm-up passes always return on the second pass")
}

fn active_css(cfg: &Config, commands: &mut ProductionCommandRunner) -> String {
    let home = std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
    let light = plasma_is_light(commands, &home.join(".config/kdeglobals"));
    let base = resolve_style(if light {
        "style-light.css"
    } else {
        "style-dark.css"
    });
    let overlay = cfg
        .display
        .overlay
        .then(|| resolve_style("style-overlay.css"));
    read_css(&base, overlay.as_deref())
}

fn tooltip_for(one: &mut OneShot, page_id: Option<&str>, css: &str) -> String {
    let ids = page_id
        .filter(|page| *page != "full")
        .map_or_else(Vec::new, |page| vec![page.to_owned()]);
    let active = build_pages(&ids);
    let index = usize::from(active.len() > 1);
    let lookup = executable_lookup();
    let mut cache = PageCommandCache::new();
    render_page(
        &one.cfg,
        &one.hw,
        &one.readings,
        css,
        &active,
        index,
        &mut one.commands,
        &lookup,
        &mut cache,
        one.clock.snapshot().monotonic,
        &one.roots.proc_root,
    )
}

/// Runs one-shot panel/tooltip rendering.
pub fn run_render(command: &RenderCommand) -> Result<()> {
    let vertical = match command.layout {
        PanelLayout::Auto => None,
        PanelLayout::Horizontal => Some(false),
        PanelLayout::Vertical => Some(true),
    };
    let page = command.page.map(render_page_id);
    let mut one = collect_one_shot(command.config.as_deref(), vertical, page)?;
    let mut css = active_css(&one.cfg, &mut one.commands);
    let want_panel = page.is_none()
        && matches!(
            command.component,
            RenderComponent::Panel | RenderComponent::Both
        );
    let want_tooltip = page.is_some()
        || matches!(
            command.component,
            RenderComponent::Tooltip | RenderComponent::Both
        );
    match command.format {
        RenderFormat::Html => {
            css.push_str(" body { background: #000; color: #fff; } .panel, .tooltip { font-family: \"NotoSansM Nerd Font Mono\", monospace; }");
            println!("── HTML written ──────────────────────────────────────────────");
            if want_panel {
                fs::write(
                    RENDER_PANEL_FILE,
                    PanelFormatter::new(&one.cfg, &one.hw).format_panel(&one.readings, &css),
                )?;
                println!("  panel:   {RENDER_PANEL_FILE}");
            }
            if want_tooltip {
                let tooltip = tooltip_for(&mut one, page, &css);
                fs::write(RENDER_TOOLTIP_FILE, tooltip)?;
                println!("  tooltip: {RENDER_TOOLTIP_FILE}");
            }
        }
        RenderFormat::Text => {
            if want_panel {
                println!("── Panel output ────────────────────────────────────────────");
                let html = PanelFormatter::new(&one.cfg, &one.hw).format_panel(&one.readings, &css);
                println!("{}", strip_html(&html));
                if want_tooltip {
                    println!();
                }
            }
            if want_tooltip {
                let label = page.unwrap_or("full");
                println!("── Tooltip output ({label}) ──────────────────────────────────");
                let html = tooltip_for(&mut one, page, &css);
                println!("{}", strip_html(&html));
            }
        }
    }
    Ok(())
}

/// Removes generated tags while retaining row and non-breaking-space layout.
#[must_use]
pub fn strip_html(html: &str) -> String {
    let mut input = html.to_owned();
    while let Some(start) = input.find("<style>") {
        let Some(end) = input[start..].find("</style>") else {
            break;
        };
        input.replace_range(start..start + end + "</style>".len(), "");
    }
    input = input
        .replace("</div>", "</div>\n")
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");
    let mut output = String::new();
    let mut in_tag = false;
    for character in input.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output.replace("&nbsp;", " ")
}

/// Prints hardware discovery and all typed readings.
pub fn run_probe(config_path: Option<&Path>) -> Result<()> {
    let one = collect_one_shot(config_path, None, None)?;
    println!("── Hardware discovery ──────────────────────────────────────");
    println!(
        "machine:         {}",
        if one.cfg.machine.is_empty() {
            "(none)"
        } else {
            &one.cfg.machine
        }
    );
    println!(
        "net_device:      {}",
        one.hw.net_device.as_deref().unwrap_or("(not found)")
    );
    println!(
        "cpu_temp_path:   {}",
        one.hw.cpu_temp_path.as_deref().map_or_else(
            || "(not found)".to_owned(),
            |path| path.display().to_string()
        )
    );
    for (label, path) in &one.hw.hd_temp_paths {
        println!("hd_temp [{label}]:  {}", path.display());
    }
    for (label, path) in &one.hw.fan_paths {
        println!("fan [{label}]:         {}", path.display());
    }
    for id in &one.hw.battery_sys_ids {
        println!("battery_sys:     {id}");
    }
    println!(
        "battery_mouse:   {}",
        one.hw.battery_mouse_id.as_deref().unwrap_or("(not found)")
    );
    println!(
        "battery_kbd:     {}",
        one.hw.battery_kbd_id.as_deref().unwrap_or("(not found)")
    );
    println!(
        "intel_gpu:       {}",
        one.hw.intel_gpu_pci.as_deref().unwrap_or("(not found)")
    );
    println!("has_nvidia:      {}\n", one.hw.has_nvidia);
    println!("── Readings ────────────────────────────────────────────────");
    print_readings(&one.readings);
    println!();
    Ok(())
}

macro_rules! reading {
    ($r:expr, $field:ident) => {
        println!("  {:<22} {:?}", stringify!($field), $r.$field)
    };
    ($r:expr, $label:literal, $field:ident) => {
        println!("  {:<22} {:?}", $label, $r.$field)
    };
}

fn print_readings(r: &ReadingsSnapshot) {
    reading!(r, cpu_usage);
    reading!(r, cpu_temp);
    reading!(r, "cpu_freq", cpu_freq_mhz);
    reading!(r, cpu_turbo);
    reading!(r, cpu_history);
    reading!(r, mem_history);
    reading!(r, "uptime", uptime_seconds);
    reading!(r, "load_avg", load_average);
    reading!(r, top_process);
    reading!(r, top_process_full);
    reading!(r, cpu_core_usage);
    reading!(r, cpu_core_history);
    reading!(r, mem_usage);
    reading!(r, "mem_used_gb", mem_used_gib);
    reading!(r, "mem_total_gb", mem_total_gib);
    reading!(r, swap_usage);
    reading!(r, net_up_bps);
    reading!(r, net_down_bps);
    reading!(r, net_device);
    reading!(r, ip_address);
    reading!(r, wifi_ssid);
    reading!(r, "wifi_signal", wifi_signal_percent);
    reading!(r, disk_read_bps);
    reading!(r, disk_write_bps);
    reading!(r, disk_usage);
    reading!(r, disk_smart);
    reading!(r, hd_temps);
    reading!(r, fan_speeds);
    reading!(r, battery_sys);
    reading!(r, battery_mouse);
    reading!(r, battery_kbd);
    reading!(r, gpu_temp);
    reading!(r, gpu_usage);
    reading!(r, gpu_mem);
    reading!(r, gpu_dec);
    reading!(r, gpu_fan);
    reading!(r, gpu_intel_freq);
    reading!(r, gpu_intel_usage);
    reading!(r, gpu_intel_dec_usage);
    reading!(r, gpu_usage_history);
    reading!(r, gpu_dec_history);
    reading!(r, net_up_history);
    reading!(r, net_down_history);
    reading!(r, screen_brightness);
    reading!(r, system_updates);
    reading!(r, server_ok);
}

/// Profiles cold/warm collection sections without touching runtime files.
pub fn run_profiling(config_path: Option<&Path>) -> Result<()> {
    let startup = Instant::now();
    let mut cfg = load_config(config_path, None)?;
    let config_ms = startup.elapsed().as_secs_f64() * 1000.0;
    let roots = FilesystemRoots::default();
    let mut commands = ProductionCommandRunner;
    let mut dbus = ProductionDbusFacade::default();
    let discover_start = Instant::now();
    let mut hw = discover_hardware(
        &roots.sys_root,
        &roots.proc_root,
        &cfg,
        &mut dbus,
        &mut commands,
        thread::available_parallelism().map_or(1, std::num::NonZero::get),
    );
    let discovery_ms = discover_start.elapsed().as_secs_f64() * 1000.0;
    println!("══════════════════════════════════════════════════════════════════════");
    println!("  STARTUP");
    println!("══════════════════════════════════════════════════════════════════════");
    println!("  load_config()                {config_ms:7.2}ms");
    println!("  discover_hardware()          {discovery_ms:7.2}ms\n");
    let clock = ProductionClock::default();
    let mut lanes = CollectorState::default();
    let mut state = DaemonStateSnapshot::default();
    for label in ["COLD POLL (EMPTY CACHE)", "WARM POLL (VALID CACHE)"] {
        let mut timings = Timings::new();
        let mut ctx = CollectCtx::new(&roots, &mut commands, &mut dbus, clock.snapshot());
        let start = Instant::now();
        let readings = collect(
            &mut lanes,
            &mut state,
            &mut hw,
            &cfg,
            &mut ctx,
            Some(&mut timings),
        );
        let total = start.elapsed();
        let width = PanelFormatter::new(&cfg, &hw).canonical_width(&readings);
        apply_canonical_width(&mut cfg, i32::try_from(width).unwrap_or(i32::MAX));
        println!("══════════════════════════════════════════════════════════════════════");
        println!("  {label}");
        println!("══════════════════════════════════════════════════════════════════════");
        println!(
            "  collect() by section — total {:.2}ms",
            total.as_secs_f64() * 1000.0
        );
        let mut rows = timings.iter().collect::<Vec<_>>();
        rows.sort_by(|left, right| right.1.cmp(left.1));
        for (name, elapsed) in rows {
            let ms = elapsed.as_secs_f64() * 1000.0;
            if ms >= 0.5 {
                println!("    {name:<26} {ms:7.2}ms");
            }
        }
        println!();
    }
    Ok(())
}

/// Prints exact item tokens and derived placement ordering.
pub fn run_list_items() -> Result<()> {
    let rows = list_items();
    let width = rows.iter().map(|(token, _)| token.len()).max().unwrap_or(0);
    println!("Available items (metric[:form] → where it can go):\n");
    for (token, placement) in rows {
        println!("  {token:<width$}  {placement}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_html_preserves_rows_and_spacing() {
        assert_eq!(
            strip_html("<style>.x{}</style><div>a&nbsp;b<br>c</div>"),
            "a b\nc\n"
        );
    }
}
