from config import Config, NotificationConfig, Section, Surface as CfgSurface
from forms import Surface
from metrics import item_surfaces
from registry import misplaced_items, needed_capabilities, parse, unknown_item_names


_NOTIF_FIELDS = (
    "disk_usage", "disk_smart", "cpu_temp", "gpu_nvidia_temp", "hd_temp",
    "battery_sys", "battery_mouse", "battery_kbd", "server_check", "load_avg",
)


def _cfg(panel=(), tooltip=(), **notif_on):
    """Config with only the given sections and every notification off except
    those in notif_on=True — so needed_capabilities is deterministic and isolable."""
    cfg = Config()
    cfg.panel = CfgSurface(sections=[Section(key="m", items=list(panel))])
    cfg.tooltip = CfgSurface(sections=[Section(key="m", items=list(tooltip))])
    flags = {f: False for f in _NOTIF_FIELDS}
    flags.update(notif_on)
    cfg.notifications = NotificationConfig(**flags)
    return cfg


def _where(token: str) -> Surface:
    """The token's actual surfaces (form ∩ metric), via the new model."""
    parsed = parse(token)
    assert parsed is not None, f"invalid token: {token}"
    return item_surfaces(*parsed)


# ── needed_capabilities (collection gating) ───────────────────────────────────

def test_cpu_usage_needs_no_dedicated_sensor():
    # cpu_usage/mem_usage live on always-collected readings → no capability
    assert needed_capabilities(_cfg(panel=["cpu_usage", "mem_usage"])) == set()


def test_item_pulls_its_capability():
    assert needed_capabilities(_cfg(panel=["disk_usage"])) == {"disk_usage"}
    assert needed_capabilities(_cfg(tooltip=["fan_speed"])) == {"fan_speed"}


def test_metric_can_need_multiple_capabilities():
    # cpu_freq shows turbo intrinsically → also needs cpu_turbo
    assert needed_capabilities(_cfg(tooltip=["cpu_freq"])) == {"cpu_freq", "cpu_turbo"}


def test_form_does_not_change_the_capability():
    # the form doesn't affect the data: cpu_usage:spark_value weighs the same as cpu_usage
    assert needed_capabilities(_cfg(tooltip=["cpu_usage:spark_value"])) == set()
    assert needed_capabilities(_cfg(tooltip=["hd_temp:pair"])) == {"hd_temp"}


def test_notification_keeps_sensor_alive_without_the_item():
    # no item, but the cpu_temp notification is on → the sensor is still read
    assert needed_capabilities(_cfg(cpu_temp=True)) == {"cpu_temp"}
    assert "disk_usage" in needed_capabilities(_cfg(disk_usage=True))


def test_unknown_token_contributes_nothing():
    assert needed_capabilities(_cfg(panel=["totally_bogus"])) == set()


def test_gpu_nvidia_metrics_share_one_capability():
    caps = needed_capabilities(_cfg(panel=["gpu_nvidia_temp", "gpu_nvidia_usage"]))
    assert caps == {"gpu_nvidia"}


# ── canonical tokens ───────────────────────────────────────────────────────────

def test_unknown_item_names_flags_bad_metric_and_bad_form():
    assert unknown_item_names(["cpu_usage", "nope"]) == {"nope"}
    assert unknown_item_names(["cpu_usage:bar", "cpu_usage:nope"]) == {"cpu_usage:nope"}
    assert unknown_item_names(["separator_small"]) == set()  # separators are valid


# ── DERIVED placement (form ∩ metric) ─────────────────────────────────────────

def test_value_metrics_live_on_both_surfaces():
    for token in ("cpu_usage", "cpu_temp", "battery_sys"):
        s = _where(token)
        assert (s & Surface.PANEL) and (s & Surface.TOOLTIP)


def test_bare_visuals_are_panel_only():
    for token in ("cpu_usage:bar", "cpu_usage:spark", "cpu_usage:braille",
                  "mem_usage:bar", "mem_usage:spark", "mem_usage:braille"):
        s = _where(token)
        assert (s & Surface.PANEL) and not (s & Surface.TOOLTIP)


def test_wide_forms_and_string_metrics_are_tooltip_only():
    for token in ("cpu_usage:spark_value", "cpu_usage:bar_spark", "hd_temp:pair",
                  "disk_smart:pair", "net_device_ip", "top_process", "uptime", "load_avg"):
        s = _where(token)
        assert (s & Surface.TOOLTIP) and not (s & Surface.PANEL)


def test_misplaced_items_flags_tooltip_only_in_panel():
    bad_panel, bad_tooltip = misplaced_items(
        panel_names={"cpu_usage", "cpu_usage:spark_value", "top_process"},
        tooltip_names={"cpu_usage", "net_device_ip"})
    assert bad_panel == {"cpu_usage:spark_value", "top_process"}
    assert bad_tooltip == set()


def test_misplaced_items_flags_panel_only_in_tooltip():
    """The bare forms (bar/spark/braille) render a trace with no label at all,
    so they belong to the panel only."""
    _, bad_tooltip = misplaced_items(
        panel_names=set(),
        tooltip_names={"cpu_usage", "cpu_usage:bar", "mem_usage:spark", "mem_usage:braille"})
    assert bad_tooltip == {"cpu_usage:bar", "mem_usage:spark", "mem_usage:braille"}


def test_misplaced_items_ignores_unknown_names():
    bad_panel, _ = misplaced_items(panel_names={"totally_bogus"}, tooltip_names=set())
    assert bad_panel == set()
