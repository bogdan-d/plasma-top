"""Tooltip page counter, shared by the daemon and the `page` wheel command.

Kept dependency-free (stdlib only) so `pirostats page next|prev` — fired once per
mouse-wheel notch — starts fast, without dragging in the sensor/formatter stack
that importing `daemon` would. The counter is a single integer in PAGE_FILE;
NPAGES_FILE holds the active page count the daemon publishes, used to wrap.

Concurrent wheel processes (a fast scroll spawns one `page` process per notch)
serialize on an flock: the counter is a read-modify-write, so without the lock
two overlapping processes would read the same value and write the same +1,
silently dropping notches — the "wheel skips pages / goes dead" bug.
"""
import fcntl
import os

from runtime import LOCK_FILE as _LOCK_FILE, NPAGES_FILE, PAGE_FILE, ensure_dirs


def read_page() -> int:
    """Raw page counter (0 = full view). Defaults to 0 when the state file is
    absent or unparseable. The active index is this modulo the page count."""
    try:
        return int(PAGE_FILE.read_text().strip())
    except (OSError, ValueError):
        return 0


def set_page(n: int) -> None:
    """Write the counter atomically. The tmp name is pid-unique so overlapping
    writers never clobber a shared tmp mid-rename."""
    ensure_dirs()
    tmp = PAGE_FILE.with_suffix(f".{os.getpid()}.tmp")
    tmp.write_text(str(n), encoding="utf-8")
    os.replace(tmp, PAGE_FILE)


def _npages() -> int:
    try:
        return int(NPAGES_FILE.read_text().strip())
    except (OSError, ValueError):
        return 1


def step_page(step: str) -> int:
    """Advance the counter by one notch (`step` 'next'/'prev'), wrapping against
    the published page count; a no-op when no deep-dive pages are configured.
    Serialized across concurrent wheel processes with an flock so a rapid scroll
    never drops a step. Returns a process exit code (always 0)."""
    n = _npages()
    if n <= 1:
        return 0
    delta = 1 if step == "next" else -1
    ensure_dirs()
    with open(_LOCK_FILE, "w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        set_page((read_page() + delta) % n)
    return 0
