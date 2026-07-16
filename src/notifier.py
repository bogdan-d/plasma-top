"""
Desktop notifications via gi.repository.Notify (GLib, part of python-gobject).
Uses edge-triggered logic: notifies once when threshold is crossed, resets when clear.
"""
from __future__ import annotations

import time
from dataclasses import dataclass, field

from config import Config
from sensors import HardwareInfo, Readings
from units import TEMP_SCALE

try:
    import gi
    gi.require_version("Notify", "0.7")
    from gi.repository import Notify as _Notify
    _Notify.init("pirostats")
    _GI_AVAILABLE = True
except Exception:
    _GI_AVAILABLE = False


def _send(title: str, body: str, icon: str = "dialog-error") -> None:
    if not _GI_AVAILABLE:
        return
    try:
        n = _Notify.Notification.new(title, body, icon)
        n.set_urgency(_Notify.Urgency.CRITICAL)
        n.set_timeout(_Notify.EXPIRES_NEVER)
        n.show()
    except Exception:
        pass


# ── State ─────────────────────────────────────────────────────────────────────

@dataclass
class NotifState:
    """Tracks which alerts are currently active (to avoid re-sending)."""
    cpu_temp: bool = False
    gpu_nvidia_temp: bool = False
    disk: dict[str, bool] = field(default_factory=dict)         # mount → active
    disk_smart: dict[str, bool] = field(default_factory=dict)   # hd_temp label → active (failing)
    hd_temp: dict[str, bool] = field(default_factory=dict)      # label → active
    battery_sys: dict[str, bool] = field(default_factory=dict)  # battery id → active
    battery_mouse: bool = False
    battery_kbd: bool = False
    server: bool = False
    load_avg: bool = False
    load_avg_high_since: float | None = None  # time.monotonic() when the threshold was first exceeded


# ── Main check ────────────────────────────────────────────────────────────────

def check_and_notify(r: Readings, cfg: Config, state: NotifState, hw: HardwareInfo) -> NotifState:
    """Edge-detect threshold crossings and send notifications. Returns updated state."""
    c  = cfg.notifications
    n  = cfg.notify_thresholds
    lb = cfg.labels
    nl = lb.get("notify", {})  # notification-only wording, see lang/<language>.toml [notify]

    # CPU temp
    if c.cpu_temp and r.cpu_temp is not None:
        over = r.cpu_temp >= n.cpu_temp
        if over and not state.cpu_temp:
            _send("PiroStats", f"{lb.get('cpu_temp', 'Cpu temp')} {r.cpu_temp}{TEMP_SCALE}")
        state.cpu_temp = over

    # GPU temp
    if c.gpu_nvidia_temp and r.gpu_temp is not None:
        over = r.gpu_temp >= n.gpu_nvidia_temp
        if over and not state.gpu_nvidia_temp:
            _send("PiroStats", f"{lb.get('gpu_nvidia_temp', 'Gpu temp')} {r.gpu_temp}{TEMP_SCALE}")
        state.gpu_nvidia_temp = over

    # Disk usage
    if c.disk_usage:
        for mount, du in r.disk_usage.items():
            if du is None or du.percent is None:
                continue
            over = du.percent >= n.disk_usage
            was  = state.disk.get(mount, False)
            if over and not was:
                _send("PiroStats", f"{nl.get('disk_usage', 'Disk')} {mount} {du.percent}%")
            state.disk[mount] = over

    # Disk SMART health (binary, edge-triggered when it turns bad)
    if c.disk_smart:
        for label, healthy in r.disk_smart.items():
            if healthy is None:
                continue
            bad = not healthy
            was = state.disk_smart.get(label, False)
            if bad and not was:
                _send("PiroStats",
                      f"{nl.get('disk_smart', 'Disk')} {label} {nl.get('smart_fail', 'SMART check FAILED')}",
                      icon="dialog-error")
            state.disk_smart[label] = bad

    # HD temp
    if c.hd_temp:
        for label, temp in r.hd_temps.items():
            if temp is None:
                continue
            over = temp >= n.hd_temp
            was  = state.hd_temp.get(label, False)
            if over and not was:
                _send("PiroStats", f"{lb.get('hd_temp', 'Disk')} {label} temp {temp}{TEMP_SCALE}",
                      icon="dialog-warning")
            state.hd_temp[label] = over

    # System battery (notify when *below* threshold, ignore while charging)
    if c.battery_sys:
        for bat in r.battery_sys:
            if not bat.perc:
                continue
            pv   = int(bat.perc.rstrip("%"))
            over = bat.state != "charging" and 0 < pv <= n.battery_sys
            was  = state.battery_sys.get(bat.id, False)
            if over and not was:
                _send("PiroStats", f"{lb.get('battery_sys', 'Battery')} {bat.perc}", icon="battery-caution")
            state.battery_sys[bat.id] = over

    # Peripheral batteries (notify when *below* threshold)
    if c.battery_mouse and r.battery_mouse and r.battery_mouse.perc:
        pv   = int(r.battery_mouse.perc.rstrip("%"))
        over = 0 < pv < n.battery_mouse   # ignore 0%: device disconnected
        if over and not state.battery_mouse:
            name = r.battery_mouse.name or lb.get("battery_mouse", "Mouse")
            _send("PiroStats", f"{name}: {r.battery_mouse.perc}", icon="battery-caution")
        state.battery_mouse = over

    if c.battery_kbd and r.battery_kbd and r.battery_kbd.perc:
        pv   = int(r.battery_kbd.perc.rstrip("%"))
        over = 0 < pv < n.battery_kbd     # ignore 0%: device disconnected
        if over and not state.battery_kbd:
            name = r.battery_kbd.name or lb.get("battery_kbd", "Keyboard")
            _send("PiroStats", f"{name}: {r.battery_kbd.perc}", icon="battery-caution")
        state.battery_kbd = over

    # Load avg 15min: notifies only if it stays above threshold for at least N
    # consecutive minutes (edge-triggered like the others, but with a time debounce).
    if c.load_avg and r.load_avg is not None:
        fifteen = r.load_avg[2]
        ratio   = fifteen / hw.cpu_count
        over    = ratio >= n.load_avg_15
        if over:
            if state.load_avg_high_since is None:
                state.load_avg_high_since = time.monotonic()
            sustained_min = (time.monotonic() - state.load_avg_high_since) / 60
            if sustained_min >= n.load_avg_minutes and not state.load_avg:
                _send("PiroStats",
                      f"{lb.get('load_avg', 'Load avg')} 15m {nl.get('load_high_for', 'high for')} "
                      f"{int(sustained_min)} min ({fifteen:.2f})",
                      icon="dialog-warning")
            state.load_avg = sustained_min >= n.load_avg_minutes
        else:
            state.load_avg_high_since = None
            state.load_avg = False

    # Server check
    if c.server_check and r.server_ok is not None:
        down = not r.server_ok
        if down and not state.server:
            _send("PiroStats", nl.get("server_down", "Server is not reachable!"), icon="dialog-error")
        state.server = down

    return state
