#!/usr/bin/env python3
"""Read HID++ 2.0 battery and device name from a Logitech Bolt receiver device.
Usage:
  logitech-bolt-battery [device_index]          → print "<N>%"
  logitech-bolt-battery --name [device_index]   → print device name
  logitech-bolt-battery --info [device_index]   → print "<name>\\n<N>%" (one call)
Exits 1 if the data cannot be read.
Requires: libhidapi-hidraw (Logitech udev rules grant hidraw access to the user).
"""
import ctypes, ctypes.util, glob, os, sys

# ── libhidapi ──────────────────────────────────────────────────────────────────

def _load_hidapi():
    for name in ("hidapi-hidraw", "hidapi"):
        path = ctypes.util.find_library(name)
        if path:
            return ctypes.CDLL(path)
    raise OSError("libhidapi-hidraw not found")

_lib = _load_hidapi()
_lib.hid_open_path.restype     = ctypes.c_void_p
_lib.hid_open_path.argtypes    = [ctypes.c_char_p]
_lib.hid_write.restype         = ctypes.c_int
_lib.hid_write.argtypes        = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t]
_lib.hid_read_timeout.restype  = ctypes.c_int
_lib.hid_read_timeout.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_size_t, ctypes.c_int]
_lib.hid_close.restype         = None
_lib.hid_close.argtypes        = [ctypes.c_void_p]

# ── Device discovery ───────────────────────────────────────────────────────────

BOLT_PID       = "c548"
BOLT_USB_IFACE = 2        # DJ/HID++ control interface
SW_ID          = 1        # arbitrary software-id tag
TIMEOUT_MS     = 1000


def _bolt_hidraw():
    """Return the hidraw path of the Bolt receiver's HID++ control interface."""
    for h in sorted(glob.glob("/sys/class/hidraw/hidraw*")):
        try:
            cur, prev = os.path.realpath(h + "/device"), None
            for _ in range(8):
                if os.path.exists(os.path.join(cur, "idProduct")):
                    pid = open(os.path.join(cur, "idProduct")).read().strip().lower()
                    if pid == BOLT_PID and prev is not None:
                        iface = int(os.path.basename(prev).rsplit(".", 1)[-1])
                        if iface == BOLT_USB_IFACE:
                            return "/dev/" + os.path.basename(h)
                    break
                prev, cur = cur, os.path.dirname(cur)
        except (OSError, ValueError):
            pass
    return None

# ── HID++ 2.0 helpers ─────────────────────────────────────────────────────────

def _xfer(handle, pkt, expect_feat):
    """Write pkt, return first matching response or None on timeout."""
    buf = ctypes.create_string_buffer(64)
    if _lib.hid_write(handle, pkt, len(pkt)) < 0:
        return None
    for _ in range(10):
        n = _lib.hid_read_timeout(handle, buf, 64, TIMEOUT_MS)
        if n >= 5:
            raw = buf.raw[:n]
            if raw[1] == pkt[1] and raw[2] == expect_feat:
                return raw
        if n <= 0:
            break
    return None


def _get_feature_idx(handle, dev_idx, feature_id):
    """Ask ROOT (feature 0) for the index of feature_id; return 0 if unsupported."""
    hi, lo = feature_id >> 8, feature_id & 0xFF
    pkt = bytes([0x11, dev_idx, 0x00, SW_ID, hi, lo] + [0] * 14)
    r = _xfer(handle, pkt, 0x00)
    return r[4] if r else 0


def _get_battery(handle, dev_idx):
    """Return battery percentage (int) or None."""
    feat = _get_feature_idx(handle, dev_idx, 0x1004)  # UNIFIED_BATTERY
    if not feat:
        return None
    pkt = bytes([0x11, dev_idx, feat, (1 << 4) | SW_ID] + [0] * 16)
    r = _xfer(handle, pkt, feat)
    return r[4] if r else None


def _get_name(handle, dev_idx):
    """Return device name string (ASCII) or empty string."""
    feat = _get_feature_idx(handle, dev_idx, 0x0005)  # DEVICE_NAME
    if not feat:
        return ""
    # Function 1 = getDeviceName(charIndex=0)
    pkt = bytes([0x11, dev_idx, feat, (1 << 4) | SW_ID, 0x00] + [0] * 15)
    r = _xfer(handle, pkt, feat)
    if not r:
        return ""
    payload = r[4:]
    end = payload.find(0)
    return payload[:end if end >= 0 else len(payload)].decode("ascii", errors="replace").strip()

# ── Public API ─────────────────────────────────────────────────────────────────

def query(dev_idx=1, want_name=False):
    """Return (name, level) where name is "" if not requested, level is None on failure."""
    path = _bolt_hidraw()
    if not path:
        return ("", None)
    handle = _lib.hid_open_path(path.encode())
    if not handle:
        return ("", None)
    try:
        name  = _get_name(handle, dev_idx) if want_name else ""
        level = _get_battery(handle, dev_idx)
        return (name, level)
    finally:
        _lib.hid_close(handle)

# ── CLI ────────────────────────────────────────────────────────────────────────

if __name__ == "__main__":
    args = sys.argv[1:]

    mode = "battery"
    if args and args[0] in ("--name", "--info"):
        mode = args.pop(0).lstrip("-")

    dev_idx = int(args[0]) if args else 1

    name, level = query(dev_idx, want_name=(mode in ("name", "info")))

    if mode == "name":
        if not name:
            sys.exit(1)
        print(name)
    elif mode == "info":
        if level is None:
            sys.exit(1)
        print(name or "Unknown")
        print(f"{level}%")
    else:
        if level is None:
            sys.exit(1)
        print(f"{level}%")
