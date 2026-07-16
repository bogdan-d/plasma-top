#!/usr/bin/env python3
"""Inject a fixed, disguised set of readings into the live widget so the README
screenshots show realistic-but-fake data (no real IP/SSID/disk/host).

The widget is watcher-driven: it reads <runtime>/{panel,tooltip}.html and shows
whatever is there. This renders the full panel + main tooltip page (with pager)
from a demo Readings — the same sanitized fixture the golden test uses — and
writes them into the runtime dir, so a pinned/hovered tooltip shows the fake data.

Usage:
    systemctl --user stop pirostats     # else it overwrites these every poll
    python3 tools/demo_shot.py          # add --light for a light desktop
    # middle-click the widget to pin the tooltip, take the screenshot
    systemctl --user start pirostats    # restore live data when done

Only the tooltip's main page carries the sensitive fields (IP/SSID/disk), so this
is what you need for the main-page shot; the other pages/panel run fine on real
data once the daemon is back.
"""
import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "src"))

from config import apply_canonical_width, load_config, resolve_style
from daemon import _publish_pages, _read_css, _render_tooltip
from formatter import PanelFormatter
from pagestate import set_page
from runtime import PANEL_FILE, TOOLTIP_FILE, ensure_dirs
from sensors import BatteryPeriph, BatterySys, DiskUsage, HardwareInfo, Readings


def _demo_hw() -> HardwareInfo:
    """Every sensor present, so every configured item renders (like the golden)."""
    return HardwareInfo(
        cpu_temp_path=Path("/x"), cpu_freq_path=Path("/x"),
        hd_temp_paths={"nvme0": Path("/x"), "sda": Path("/x")},
        fan_paths={"1": Path("/x"), "2": Path("/x")},
        battery_sys_ids=["/org/freedesktop/UPower/devices/battery_BAT0"],
        # Nvidia only: the igpu items are hidden (gated off) so the GPU section
        # doesn't duplicate usage/decoder rows in the screenshot.
        has_nvidia=True, intel_gpu_freq_path=None, intel_gpu_pci=None,
        net_device="wlan0", disk_io_device="nvme0n1", cpu_count=8,
        cpu_turbo_supported=True, has_backlight=True, has_wifi=True,
        battery_mouse_id="/m", battery_kbd_id="/k",
        disk_smart_drives={"nvme0": ("/d0", "nvme", False), "sda": ("/d1", "ata", True)},
    )


def _demo_readings() -> Readings:
    """Realistic but fake: no real IP/SSID/disk name. Mirrors the golden fixture."""
    return Readings(
        cpu_usage=31, cpu_temp=48, cpu_freq=3200.0, cpu_turbo=True,
        cpu_history=[10, 20, 30, 40, 50, 60, 70, 80, 73, 65] * 2,
        mem_history=[15, 25, 35, 45, 55, 42, 38, 44, 50, 42] * 2,
        uptime=123456, load_avg=(0.39, 0.80, 0.76),
        top_process=[("plasmashell", 12), ("code", 8)],
        mem_usage=23, mem_used_gb=4, mem_total_gb=15, swap_usage=5,
        net_up_bps=0, net_down_bps=0,
        net_device="wlan0", ip_address="192.168.1.5", wifi_ssid="MyWifi", wifi_signal=100,
        disk_read_bps=0, disk_write_bps=8000,
        disk_usage={"/": DiskUsage(34, 32, 98), "/mnt/data": DiskUsage(22, 141, 672)},
        disk_smart={"nvme0": True, "sda": True},
        hd_temps={"nvme0": 31, "sda": 38},
        fan_speeds={"1": 0, "2": 0},
        battery_sys=[BatterySys(id="/BAT0", perc="87%", rate=-15, state="charging", limit=80)],
        battery_mouse=BatteryPeriph("Logitech Mouse", "55%"),
        battery_kbd=BatteryPeriph("Logitech Kbd", "85%"),
        gpu_temp=42, gpu_usage=3, gpu_mem=18, gpu_dec=0, gpu_fan=0,
        gpu_intel_freq=300, gpu_intel_usage=1, gpu_intel_dec_usage=0,
        screen_brightness=60, system_updates=0, server_ok=True,
    )


def main() -> None:
    ap = argparse.ArgumentParser(description="Write disguised demo HTML into the runtime dir.")
    ap.add_argument("--light", action="store_true", help="use the light stylesheet")
    args = ap.parse_args()

    cfg = load_config()
    fmt = PanelFormatter(cfg, _demo_hw())
    r = _demo_readings()

    css = _read_css(resolve_style("style-light.css" if args.light else "style-dark.css"))
    apply_canonical_width(cfg, fmt.canonical_width(r))

    ensure_dirs()                  # before _publish_pages: it writes into state/
    active = _publish_pages(cfg)   # writes npages so the pager dots match
    set_page(0)                    # main page

    PANEL_FILE.write_text(fmt.format_panel(r, css=css), encoding="utf-8")
    TOOLTIP_FILE.write_text(_render_tooltip(fmt, r, css, active), encoding="utf-8")
    print(f"wrote demo panel + tooltip to {PANEL_FILE.parent}")
    print("pin the widget (middle-click) and screenshot; `systemctl --user start pirostats` to restore.")


if __name__ == "__main__":
    main()
