"""notifier.check_and_notify: the edge/debounce logic only.

Pure once _send and the clock are stubbed — no gi, no sensors. What's worth
pinning here is the spike rejection: the alerts fire off instantaneous sysfs
readings, and a CPU crosses its threshold for a fraction of a second on every
boost burst, so "over the line" must never be enough on its own.
"""
import notifier
import pytest
from config import Config, NotificationConfig
from notifier import Latch, NotifState, check_and_notify
from sensors import Readings


class _Clock:
    """Monotonic stand-in the tests step by hand."""
    def __init__(self):
        self.t = 1000.0

    def __call__(self):
        return self.t

    def advance(self, seconds):
        self.t += seconds


class _Hw:
    """check_and_notify reads exactly one field off HardwareInfo."""
    cpu_count = 8


@pytest.fixture
def sent(monkeypatch):
    """Collects notification bodies instead of showing them."""
    bodies = []
    monkeypatch.setattr(notifier, "_send",
                        lambda title, body, icon="dialog-error": bodies.append(body))
    return bodies


@pytest.fixture
def clock(monkeypatch):
    c = _Clock()
    monkeypatch.setattr(notifier.time, "monotonic", c)
    return c


def _cfg(**notif_on):
    """Config with every notification off except those passed True, so one alert
    is isolable. Thresholds stay at their config.toml defaults."""
    cfg = Config()
    flags = {f.name: False for f in NotificationConfig.__dataclass_fields__.values()}
    flags.update(notif_on)
    cfg.notifications = NotificationConfig(**flags)
    return cfg


def _poll(cfg, state, clock, temp, *, seconds=0):
    """One daemon poll at `temp`, `seconds` after the previous one."""
    clock.advance(seconds)
    return check_and_notify(Readings(cpu_temp=temp), cfg, state, _Hw())


# ── The bug: a boost spike must not notify ────────────────────────────────────

def test_cpu_temp_spike_never_notifies(sent, clock):
    """Threshold 80, one poll at 82 between idle ones: the boost burst pattern
    that made the daemon alert every few seconds on an idle machine."""
    cfg, state = _cfg(cpu_temp=True), NotifState()
    for temp in (50, 82, 50, 84, 50, 91, 50):
        _poll(cfg, state, clock, temp, seconds=1.5)
    assert sent == []


def test_cpu_temp_notifies_once_when_sustained(sent, clock):
    """Genuinely hot: over the threshold for the whole hold window → one alert,
    and no repeat while it stays hot."""
    cfg, state = _cfg(cpu_temp=True), NotifState()
    for _ in range(60):
        _poll(cfg, state, clock, 85, seconds=1.5)   # 90s at 85°C, hold is 60s
    assert sent == ["Cpu temp 85C"]


def test_cpu_temp_hysteresis_blocks_rattle(sent, clock):
    """Once tripped, hovering across the threshold must not re-fire: the alert
    re-arms only below trip - temp_hysteresis (80 - 5 = 75)."""
    cfg, state = _cfg(cpu_temp=True), NotifState()
    for _ in range(60):
        _poll(cfg, state, clock, 85, seconds=1.5)
    assert len(sent) == 1
    for temp in (78, 81, 76, 82, 79):
        _poll(cfg, state, clock, temp, seconds=1.5)
    assert len(sent) == 1


def test_cpu_temp_rearms_after_cooling(sent, clock):
    """Below the hysteresis floor the latch clears, so a second real episode is
    reported — the alert is debounced, not one-shot."""
    cfg, state = _cfg(cpu_temp=True), NotifState()
    for _ in range(60):
        _poll(cfg, state, clock, 85, seconds=1.5)
    _poll(cfg, state, clock, 60, seconds=1.5)       # cooled well under 75 → re-arm
    for _ in range(60):
        _poll(cfg, state, clock, 85, seconds=1.5)
    assert len(sent) == 2


def test_cpu_temp_hold_restarts_on_a_dip(sent, clock):
    """The hold wants a *continuous* stretch: a single reading under the trip
    point resets it, so alternating hot/cool never accumulates to an alert."""
    cfg, state = _cfg(cpu_temp=True), NotifState()
    for _ in range(40):
        _poll(cfg, state, clock, 85, seconds=1.5)   # 60s worth, but…
        _poll(cfg, state, clock, 70, seconds=1.5)   # …each dip re-arms the wait
    assert sent == []


def test_cpu_temp_sustain_zero_fires_immediately(sent, clock):
    """temp_sustain_seconds = 0 opts out of the hold (documented in config.toml),
    leaving the pre-fix behavior for anyone who wants it."""
    cfg, state = _cfg(cpu_temp=True), NotifState()
    cfg.notify_thresholds.temp_sustain_seconds = 0
    _poll(cfg, state, clock, 82)
    assert sent == ["Cpu temp 82C"]


def test_cpu_temp_notification_off_stays_silent(sent, clock):
    cfg, state = _cfg(cpu_temp=False), NotifState()
    for _ in range(60):
        _poll(cfg, state, clock, 95, seconds=1.5)
    assert sent == []


# ── The latch itself ──────────────────────────────────────────────────────────

def test_sustained_hold_measures_time_not_polls(clock):
    """The hold is wall-clock, so it holds whatever display.poll_interval is: one
    poll on either side of the window is enough to trip it."""
    latch = Latch()
    assert notifier._sustained(latch, 85, 80, 75, 60, clock.t) is False
    clock.advance(61)
    assert notifier._sustained(latch, 85, 80, 75, 60, clock.t) is True


def test_sustained_fires_once_per_episode(clock):
    """True on the trip poll only — the caller sends on True, so a second True
    while still hot would be a duplicate notification."""
    latch = Latch()
    notifier._sustained(latch, 85, 80, 75, 60, clock.t)
    clock.advance(61)
    assert notifier._sustained(latch, 85, 80, 75, 60, clock.t) is True
    clock.advance(1.5)
    assert notifier._sustained(latch, 85, 80, 75, 60, clock.t) is False


def test_sustained_without_hysteresis_clears_at_the_trip_point(clock):
    """clear == trip is the load_avg case: no hysteresis band, so anything under
    the threshold re-arms."""
    latch = Latch()
    notifier._sustained(latch, 1.0, 0.9, 0.9, 0, clock.t)
    assert latch.active
    notifier._sustained(latch, 0.89, 0.9, 0.9, 0, clock.t)
    assert not latch.active
