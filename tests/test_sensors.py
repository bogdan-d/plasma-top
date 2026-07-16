import sensors
from config import Config


class _Part:
    """Minimal stand-in for psutil's sdiskpart (only .mountpoint is read)."""
    def __init__(self, mountpoint):
        self.mountpoint = mountpoint


# ── _resolve_mounts ───────────────────────────────────────────────────────────

def test_resolve_mounts_explicit_list_used_as_is():
    cfg = Config()
    cfg.disks.mounts = ["/", "/data"]
    assert sensors._resolve_mounts(cfg) == ["/", "/data"]


def test_resolve_mounts_auto_filters_to_roots_and_orders(monkeypatch):
    cfg = Config()  # mounts default "auto", auto_roots = /mnt /media /run/media
    parts = [
        _Part("/"), _Part("/boot"), _Part("/proc"),       # / kept, others dropped
        _Part("/run/media/user/Backup"),
        _Part("/mnt/data"),
        _Part("/media/x"),
    ]
    monkeypatch.setattr(sensors.psutil, "disk_partitions", lambda: parts)
    # "/" first, the rest sorted alphabetically
    assert sensors._resolve_mounts(cfg) == [
        "/", "/media/x", "/mnt/data", "/run/media/user/Backup"]


def test_resolve_mounts_auto_root_only_when_nothing_mounted(monkeypatch):
    cfg = Config()
    monkeypatch.setattr(sensors.psutil, "disk_partitions",
                        lambda: [_Part("/"), _Part("/boot/efi")])
    assert sensors._resolve_mounts(cfg) == ["/"]


def test_resolve_mounts_auto_ignores_bare_root_dir(monkeypatch):
    # "/mnt" itself (no trailing child) is not a watched mount; only children of
    # the roots count (startswith "/mnt/").
    cfg = Config()
    monkeypatch.setattr(sensors.psutil, "disk_partitions",
                        lambda: [_Part("/"), _Part("/mnt")])
    assert sensors._resolve_mounts(cfg) == ["/"]
