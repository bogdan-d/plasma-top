"""Deterministic render oracle for golden-parity fixtures.

Loads a small TOML fixture, reconstructs the render inputs, and renders one of
the shipped surfaces through the existing Python formatter.
"""
from __future__ import annotations

import argparse
import importlib
import sys
import tomllib
from contextlib import contextmanager
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Any
from unittest.mock import patch


ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src"


FIXED_TIME = 1_000_000.0
COMPONENTS = {
    "panel_v": (True, "panel"),
    "panel_h": (False, "panel"),
    "tooltip": (True, "tooltip"),
}


@dataclass(frozen=True)
class OracleFixture:
    hardware: Any
    readings: Any


@lru_cache(maxsize=1)
def _runtime_symbols() -> dict[str, Any]:
    if str(SRC) not in sys.path:
        sys.path.insert(0, str(SRC))

    config_module = importlib.import_module("config")
    config_exports = importlib.import_module("config")
    formatter_exports = importlib.import_module("formatter")
    sensors_exports = importlib.import_module("sensors")

    return {
        "config_module": config_module,
        "PanelGeometry": config_exports.PanelGeometry,
        "apply_canonical_width": config_exports.apply_canonical_width,
        "load_config": config_exports.load_config,
        "PanelFormatter": formatter_exports.PanelFormatter,
        "BatteryPeriph": sensors_exports.BatteryPeriph,
        "BatterySys": sensors_exports.BatterySys,
        "DiskUsage": sensors_exports.DiskUsage,
        "HardwareInfo": sensors_exports.HardwareInfo,
        "Readings": sensors_exports.Readings,
    }


def _maybe_path(value: str | None) -> Path | None:
    return Path(value) if value else None


def _path_map(raw: dict[str, str]) -> dict[str, Path]:
    return {key: Path(value) for key, value in raw.items()}


def _load_disk_smart_drives(raw: dict[str, dict[str, object]]) -> dict[str, tuple[str, str, bool]]:
    return {
        label: (str(data["path"]), str(data["kind"]), bool(data.get("rotational", False)))
        for label, data in raw.items()
    }


def _load_disk_usage(raw: dict[str, dict[str, int]]) -> dict[str, Any]:
    disk_usage_cls = _runtime_symbols()["DiskUsage"]
    return {
        mount: disk_usage_cls(
            percent=data.get("percent"),
            used_gb=data.get("used_gb"),
            total_gb=data.get("total_gb"),
        )
        for mount, data in raw.items()
    }


def _load_battery_periph(raw: dict[str, str] | None) -> Any | None:
    if not raw:
        return None
    battery_periph_cls = _runtime_symbols()["BatteryPeriph"]
    return battery_periph_cls(name=str(raw.get("name", "")), perc=str(raw.get("perc", "")))


def _load_hardware(raw: dict[str, object]) -> Any:
    hardware_info_cls = _runtime_symbols()["HardwareInfo"]
    return hardware_info_cls(
        cpu_temp_path=_maybe_path(raw.get("cpu_temp_path")),
        cpu_freq_path=_maybe_path(raw.get("cpu_freq_path")),
        hd_temp_paths=_path_map(raw.get("hd_temp_paths", {})),
        fan_paths=_path_map(raw.get("fan_paths", {})),
        battery_sys_ids=list(raw.get("battery_sys_ids", [])),
        has_nvidia=bool(raw.get("has_nvidia", False)),
        intel_gpu_freq_path=_maybe_path(raw.get("intel_gpu_freq_path")),
        intel_gpu_pci=raw.get("intel_gpu_pci"),
        net_device=raw.get("net_device"),
        disk_io_device=raw.get("disk_io_device"),
        cpu_count=int(raw.get("cpu_count") or 1),
        cpu_turbo_supported=bool(raw.get("cpu_turbo_supported", False)),
        has_backlight=bool(raw.get("has_backlight", False)),
        has_wifi=bool(raw.get("has_wifi", False)),
        battery_mouse_id=raw.get("battery_mouse_id"),
        battery_kbd_id=raw.get("battery_kbd_id"),
        disk_smart_drives=_load_disk_smart_drives(raw.get("disk_smart_drives", {})),
    )


def _load_readings(raw: dict[str, object]) -> Any:
    runtime = _runtime_symbols()
    readings_cls = runtime["Readings"]
    battery_sys_cls = runtime["BatterySys"]
    return readings_cls(
        cpu_usage=raw.get("cpu_usage"),
        cpu_temp=raw.get("cpu_temp"),
        cpu_freq=raw.get("cpu_freq"),
        cpu_turbo=raw.get("cpu_turbo"),
        cpu_history=list(raw.get("cpu_history", [])),
        mem_history=list(raw.get("mem_history", [])),
        uptime=raw.get("uptime"),
        load_avg=tuple(raw["load_avg"]) if "load_avg" in raw else None,
        top_process=[(name, pct) for name, pct in raw.get("top_process", [])],
        mem_usage=raw.get("mem_usage"),
        mem_used_gb=raw.get("mem_used_gb"),
        mem_total_gb=raw.get("mem_total_gb"),
        swap_usage=raw.get("swap_usage"),
        net_up_bps=raw.get("net_up_bps"),
        net_down_bps=raw.get("net_down_bps"),
        net_device=raw.get("net_device"),
        ip_address=raw.get("ip_address"),
        wifi_ssid=raw.get("wifi_ssid"),
        wifi_signal=raw.get("wifi_signal"),
        disk_read_bps=raw.get("disk_read_bps"),
        disk_write_bps=raw.get("disk_write_bps"),
        disk_usage=_load_disk_usage(raw.get("disk_usage", {})),
        disk_smart=dict(raw.get("disk_smart", {})),
        hd_temps=dict(raw.get("hd_temps", {})),
        fan_speeds=dict(raw.get("fan_speeds", {})),
        battery_sys=[battery_sys_cls(**entry) for entry in raw.get("battery_sys", [])],
        battery_mouse=_load_battery_periph(raw.get("battery_mouse")),
        battery_kbd=_load_battery_periph(raw.get("battery_kbd")),
        gpu_temp=raw.get("gpu_temp"),
        gpu_usage=raw.get("gpu_usage"),
        gpu_mem=raw.get("gpu_mem"),
        gpu_dec=raw.get("gpu_dec"),
        gpu_fan=raw.get("gpu_fan"),
        gpu_intel_freq=raw.get("gpu_intel_freq"),
        gpu_intel_usage=raw.get("gpu_intel_usage"),
        gpu_intel_dec_usage=raw.get("gpu_intel_dec_usage"),
        screen_brightness=raw.get("screen_brightness"),
        system_updates=raw.get("system_updates"),
        server_ok=raw.get("server_ok"),
    )


def load_fixture(path: str | Path) -> OracleFixture:
    fixture_path = Path(path)
    with fixture_path.open("rb") as handle:
        raw = tomllib.load(handle)
    return OracleFixture(
        hardware=_load_hardware(raw["hardware"]),
        readings=_load_readings(raw["readings"]),
    )


@contextmanager
def deterministic_render_env():
    runtime = _runtime_symbols()
    config_module = runtime["config_module"]
    panel_geometry_cls = runtime["PanelGeometry"]
    with patch.object(config_module, "detect_panel_geometry", return_value=panel_geometry_cls(vertical=True)):
        with patch("time.time", return_value=FIXED_TIME):
            yield


def render_component(fixture: OracleFixture, component: str) -> str:
    if component not in COMPONENTS:
        choices = ", ".join(COMPONENTS)
        raise ValueError(f"unknown component {component!r}; expected one of: {choices}")

    vertical, kind = COMPONENTS[component]
    runtime = _runtime_symbols()
    load_config = runtime["load_config"]
    panel_formatter_cls = runtime["PanelFormatter"]
    apply_canonical_width = runtime["apply_canonical_width"]
    with deterministic_render_env():
        cfg = load_config(vertical=vertical)
        formatter = panel_formatter_cls(cfg, fixture.hardware)
        if kind != "panel":
            apply_canonical_width(cfg, formatter.canonical_width(fixture.readings))
        return formatter.format_panel(fixture.readings) if kind == "panel" else formatter.format_tooltip(fixture.readings)


def render_fixture(path: str | Path, component: str) -> str:
    return render_component(load_fixture(path), component)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Render a deterministic PiroStats fixture through the Python formatter")
    parser.add_argument("fixture", type=Path, help="Path to the TOML oracle fixture")
    parser.add_argument("component", choices=sorted(COMPONENTS), help="Surface to render")
    args = parser.parse_args(argv)
    sys.stdout.write(render_fixture(args.fixture, args.component))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
