import config
from config import (
    Surface, _deep_merge, _parse_surface, _resolve_items, detect_machine, load_config,
    detect_vertical_layout, detect_panel_geometry, _auto_fit_panel, apply_canonical_width,
    Config, PanelGeometry, TOOLTIP_WIDTH_FLOOR,
)


# ── apply_canonical_width ────────────────────────────────────────────────────

def test_apply_canonical_width_sets_resolved_width():
    cfg = Config()
    apply_canonical_width(cfg, TOOLTIP_WIDTH_FLOOR + 6)
    assert cfg.display.tooltip_width == TOOLTIP_WIDTH_FLOOR + 6


def test_apply_canonical_width_does_not_ratchet():
    cfg = Config()
    apply_canonical_width(cfg, TOOLTIP_WIDTH_FLOOR + 12)
    apply_canonical_width(cfg, TOOLTIP_WIDTH_FLOOR + 4)   # canonical shrank back
    assert cfg.display.tooltip_width == TOOLTIP_WIDTH_FLOOR + 4   # follows down, not stuck


def test_apply_canonical_width_floors_at_builtin_minimum():
    cfg = Config()
    apply_canonical_width(cfg, TOOLTIP_WIDTH_FLOOR - 10)   # sparse config, below the floor
    assert cfg.display.tooltip_width == TOOLTIP_WIDTH_FLOOR


def test_apply_canonical_width_ignores_nonpositive():
    cfg = Config()
    cfg.display.tooltip_width = 42
    apply_canonical_width(cfg, 0)
    assert cfg.display.tooltip_width == 42


# ── _deep_merge ──────────────────────────────────────────────────────────────

def test_deep_merge_override_scalar():
    base = {"a": 1, "b": 2}
    assert _deep_merge(base, {"b": 3}) == {"a": 1, "b": 3}


def test_deep_merge_nested_dicts_merge_recursively():
    base = {"panel": {"cpu_usage": True, "mem_usage": True}}
    override = {"panel": {"mem_usage": False}}
    result = _deep_merge(base, override)
    assert result == {"panel": {"cpu_usage": True, "mem_usage": False}}


def test_deep_merge_does_not_mutate_base():
    base = {"a": {"x": 1}}
    _deep_merge(base, {"a": {"x": 2}})
    assert base == {"a": {"x": 1}}


def test_deep_merge_dict_replaces_non_dict():
    base = {"a": 1}
    override = {"a": {"x": 2}}
    assert _deep_merge(base, override) == {"a": {"x": 2}}


# ── detect_machine ───────────────────────────────────────────────────────────

def test_detect_machine_no_dmi_access_returns_none(monkeypatch):
    def fail_read(self, *a, **kw):
        raise OSError("no permission")
    monkeypatch.setattr("pathlib.Path.read_text", fail_read)
    assert detect_machine({"laptop": {"detect": {"board_contains": "X"}}}) is None


def test_detect_machine_board_contains_match(monkeypatch):
    texts = {"board_name": "ABC-1234 Example Board", "product_name": "Example Laptop 15"}
    monkeypatch.setattr("pathlib.Path.read_text", lambda self: texts[self.name])
    machines = {
        "laptop":  {"detect": {"board_contains": "Example"}},
        "desktop": {"detect": {"board_contains": "Z790"}},
    }
    assert detect_machine(machines) == "laptop"


def test_detect_machine_no_match_returns_none(monkeypatch):
    texts = {"board_name": "Some Board", "product_name": "Some Product"}
    monkeypatch.setattr("pathlib.Path.read_text", lambda self: texts[self.name])
    machines = {"laptop": {"detect": {"board_contains": "Example"}}}
    assert detect_machine(machines) is None


def test_detect_machine_product_contains_match(monkeypatch):
    texts = {"board_name": "Generic Board", "product_name": "ExampleVM 7"}
    monkeypatch.setattr("pathlib.Path.read_text", lambda self: texts[self.name])
    machines = {"vm": {"detect": {"product_contains": "ExampleVM"}}}
    assert detect_machine(machines) == "vm"


def test_detect_machine_ignores_non_dict_entries(monkeypatch):
    texts = {"board_name": "Some Board", "product_name": "Some Product"}
    monkeypatch.setattr("pathlib.Path.read_text", lambda self: texts[self.name])
    machines = {"not_a_machine": "oops"}
    assert detect_machine(machines) is None


# ── Surface / Section parsing ────────────────────────────────────────────────

def test_resolve_items_plain():
    assert _resolve_items({"items": ["a", "b"]}) == ["a", "b"]


def test_resolve_items_add_appends_without_dups_preserving_order():
    sec = {"items": ["a", "b"], "items_add": ["b", "c"]}
    assert _resolve_items(sec) == ["a", "b", "c"]


def test_resolve_items_remove():
    sec = {"items": ["a", "b", "c"], "items_remove": ["b"]}
    assert _resolve_items(sec) == ["a", "c"]


def test_parse_surface_order_drives_sections():
    raw = {
        "order": ["live", "io"],
        "live": {"title": "Live", "items": ["cpu_usage"]},
        "io": {"title": "I/O", "items": ["net_speed"]},
        # not in order → ignored
        "ghost": {"items": ["nope"]},
    }
    s = _parse_surface(raw)
    assert [sec.key for sec in s.sections] == ["live", "io"]
    assert s.sections[0].title == "Live"
    assert s.has("net_speed")
    assert not s.has("nope")
    assert s.item_set() == {"cpu_usage", "net_speed"}


def test_parse_surface_order_add_appends_section():
    raw = {
        "order": ["live"],
        "order_add": ["extra"],
        "live": {"items": ["cpu_usage"]},
        "extra": {"items": ["uptime"]},
    }
    s = _parse_surface(raw)
    assert [sec.key for sec in s.sections] == ["live", "extra"]


def test_surface_has_and_item_set_empty():
    s = Surface()
    assert not s.has("anything")
    assert s.item_set() == set()


# ── _drop_unknown_items ──────────────────────────────────────────────────────

def test_drop_unknown_items_removes_typos(tmp_path):
    path = tmp_path / "config.toml"
    path.write_text(
        "[tooltip]\norder = ['live']\n"
        "[tooltip.live]\nitems = ['cpu_usage', 'cpu_usage:bogus_form', 'totally_bogus']\n")
    cfg = load_config(path)
    assert cfg.tooltip.sections[0].items == ["cpu_usage"]


def test_drop_unknown_items_spares_separators(tmp_path):
    # Separators are section entries, not items: they must survive the guardrail.
    path = tmp_path / "config.toml"
    path.write_text(
        "[tooltip]\norder = ['live']\n"
        "[tooltip.live]\nitems = ['cpu_usage', 'separator_small', 'separator_big', 'nope']\n")
    cfg = load_config(path)
    assert cfg.tooltip.sections[0].items == ["cpu_usage", "separator_small", "separator_big"]


# ── _drop_misplaced_items ────────────────────────────────────────────────────

def test_drop_misplaced_items_removes_panel_only_from_the_tooltip(tmp_path):
    path = tmp_path / "config.toml"
    path.write_text(
        "[tooltip]\norder = ['live']\n"
        "[tooltip.live]\nitems = ['cpu_usage:spark', 'cpu_usage', 'mem_usage:bar']\n")
    cfg = load_config(path)
    assert cfg.tooltip.sections[0].items == ["cpu_usage"]


def test_drop_misplaced_items_removes_tooltip_only_from_the_panel(tmp_path):
    path = tmp_path / "config.toml"
    path.write_text(
        "[panel]\norder = ['live']\n"
        "[panel.live]\nitems = ['uptime', 'cpu_usage', 'net_speed']\n")
    cfg = load_config(path)
    # net_speed is an intrinsic shape valid on both surfaces; uptime is tooltip-only.
    assert cfg.panel.sections[0].items == ["cpu_usage", "net_speed"]


def test_drop_misplaced_items_leaves_a_section_empty_rather_than_absent(tmp_path):
    # An emptied section stays in the surface: the render collapses empty ones.
    path = tmp_path / "config.toml"
    path.write_text(
        "[tooltip]\norder = ['live']\n[tooltip.live]\nitems = ['cpu_usage:spark']\n")
    cfg = load_config(path)
    assert cfg.tooltip.sections[0].items == []


# ── load_config ──────────────────────────────────────────────────────────────

def test_load_config_missing_path_returns_no_machine(tmp_path):
    cfg = load_config(tmp_path / "does-not-exist.toml")
    assert cfg.machine == ""
    assert cfg.panel.sections == []


def test_load_config_section_schema(tmp_path):
    toml_text = """
[tooltip]
order = ["live", "load"]
[tooltip.live]
title = "Live"
items = ["cpu_usage:spark_value", "mem_usage:spark_value"]
[tooltip.load]
title = "Load"
items = ["uptime", "load_avg"]
"""
    path = tmp_path / "config.toml"
    path.write_text(toml_text)
    cfg = load_config(path)
    assert [s.key for s in cfg.tooltip.sections] == ["live", "load"]
    assert cfg.tooltip.has("uptime")
    assert cfg.tooltip.sections[0].title == "Live"


def test_load_config_machine_items_add(tmp_path, monkeypatch):
    # The matching machine block (from the sibling machines.toml) merges its
    # items_add over the base section. Force the match — detection itself is
    # covered by the detect_machine tests above.
    monkeypatch.setattr("config.detect_machine", lambda machines: "desktop")
    (tmp_path / "config.toml").write_text("""
[tooltip]
order = ["live"]
[tooltip.live]
items = ["cpu_usage:spark_value"]
""")
    (tmp_path / "machines.toml").write_text("""
[desktop.tooltip.live]
items_add = ["fan_speed"]
""")
    cfg = load_config(tmp_path / "config.toml")
    live = cfg.tooltip.sections[0]
    assert cfg.machine == "desktop"
    assert live.items == ["cpu_usage:spark_value", "fan_speed"]


def test_load_config_machine_order_add_new_section(tmp_path, monkeypatch):
    monkeypatch.setattr("config.detect_machine", lambda machines: "mymachine")
    (tmp_path / "config.toml").write_text("""
[panel]
order = ["live"]
[panel.live]
items = ["cpu_usage"]
""")
    (tmp_path / "machines.toml").write_text("""
[mymachine.panel]
order_add = ["drives"]
[mymachine.panel.drives]
items = ["disk_usage"]
""")
    cfg = load_config(tmp_path / "config.toml")
    assert [s.key for s in cfg.panel.sections] == ["live", "drives"]
    assert cfg.panel.has("disk_usage")


# ── Item name validation vs. registry (canonical set) ─────────────────────────

def test_unknown_item_names_flags_only_unknowns():
    from registry import unknown_item_names
    assert unknown_item_names(["cpu_usage", "disk_usage", "bogus_item"]) == {"bogus_item"}
    assert unknown_item_names(["cpu_usage", "hd_temp"]) == set()


def test_default_config_has_no_unknown_items():
    """Every item listed in the default config.toml must resolve to a valid item
    (metric:form) — a guardrail against half-added tokens or typos. The canonical
    validator lives in registry."""
    from registry import unknown_item_names
    cfg = load_config(None)
    configured = cfg.panel.item_set() | cfg.tooltip.item_set()
    assert unknown_item_names(configured) == set()


def test_load_config_warns_on_unknown_item(tmp_path, capsys):
    path = tmp_path / "config.toml"
    path.write_text(
        "[panel]\norder = ['main']\n\n"
        "[panel.main]\nitems = ['cpu_usage', 'totally_not_an_item']\n"
    )
    load_config(path)
    assert "totally_not_an_item" in capsys.readouterr().err


# ── Panel orientation: detection + [panel_horizontal]/[panel_vertical] override ─

def test_detect_vertical_layout_defaults_vertical_without_appletsrc(monkeypatch):
    def fail_read(self, *a, **kw):
        raise OSError("no appletsrc")
    monkeypatch.setattr("pathlib.Path.read_text", fail_read)
    assert detect_vertical_layout() is True


def test_detect_vertical_layout_reads_panel_edge(monkeypatch):
    # containment with the pirostats applet at location=4 (bottom) → horizontal.
    appletsrc = (
        "[Containments][2]\n"
        "location=4\n"
        "[Containments][2][Applets][7]\n"
        "plugin=com.github.lucazade.pirostats\n"
    )
    monkeypatch.setattr("pathlib.Path.read_text", lambda self, *a, **kw: appletsrc)
    assert detect_vertical_layout() is False


# ── Panel geometry: width/advance published by the plasmoid ──────────────────

_APPLETSRC = (
    "[Containments][2]\n"
    "location=5\n"                       # left → vertical
    "[Containments][2][Applets][25]\n"
    "plugin=com.github.lucazade.pirostats\n"
)


def _patch_plasma(monkeypatch, geom=None, appletsrc=_APPLETSRC):
    """Patches Path.read_text: the plasmoid's geometry file (geom, None =
    absent) and the appletsrc (orientation fallback)."""
    def read(self, *a, **kw):
        s = str(self)
        if s == str(config.GEOM_FILE):
            if geom is None:
                raise OSError("no geom file")
            return geom
        if "appletsrc" in s:
            return appletsrc
        raise OSError("unexpected read")
    monkeypatch.setattr("pathlib.Path.read_text", read)


def test_detect_panel_geometry_reads_geom_file(monkeypatch):
    _patch_plasma(monkeypatch, geom="42 6.59375 1\n")
    geo = detect_panel_geometry()
    assert geo.vertical is True
    assert geo.usable_px == 42.0
    assert geo.glyph_adv == 6.59375


def test_detect_panel_geometry_falls_back_to_appletsrc_orientation(monkeypatch):
    # No geometry file: orientation from appletsrc, no auto-fit.
    _patch_plasma(monkeypatch, geom=None)
    geo = detect_panel_geometry()
    assert geo.vertical is True
    assert geo.usable_px is None and geo.glyph_adv is None


def test_detect_panel_geometry_ignores_degenerate_geom_file(monkeypatch):
    # Zeroed file (startup) or malformed: falls back to orientation, not a nonsensical fit.
    _patch_plasma(monkeypatch, geom="0 0 1\n")
    assert detect_panel_geometry().usable_px is None
    _patch_plasma(monkeypatch, geom="garbage\n")
    assert detect_panel_geometry().usable_px is None


def test_detect_panel_geometry_stale_geom_orientation_uses_appletsrc(monkeypatch):
    # geom for a different orientation (the widget republishes asynchronously
    # after a move): the orientation stays the appletsrc's (vertical, location=5)
    # and the measurements — for the other axis — are ignored until it aligns.
    _patch_plasma(monkeypatch, geom="42 6.59375 0\n")   # horizontal flag, vertical panel
    geo = detect_panel_geometry()
    assert geo.vertical is True
    assert geo.usable_px is None and geo.glyph_adv is None


def test_detect_panel_geometry_defaults_when_unreadable(monkeypatch):
    def fail(self, *a, **kw):
        raise OSError("no plasma")
    monkeypatch.setattr("pathlib.Path.read_text", fail)
    assert detect_panel_geometry() == PanelGeometry(vertical=True)


# ── Geometry cache: seeds a fitted first paint after the tmpfs GEOM_FILE is wiped ──

def test_read_geom_falls_back_to_cache_when_live_absent(monkeypatch, tmp_path):
    import config
    cache = tmp_path / "geom_cache"
    cache.write_text("42 6.59375 1\n")
    monkeypatch.setattr(config, "GEOM_FILE", tmp_path / "absent")
    monkeypatch.setattr(config, "GEOM_CACHE", cache)
    geo = config._read_geom_file()
    assert geo is not None
    assert geo.usable_px == 42.0 and geo.glyph_adv == 6.59375 and geo.vertical is True


def test_read_geom_prefers_live_over_cache(monkeypatch, tmp_path):
    import config
    live = tmp_path / "live"
    live.write_text("100 5 1\n")
    cache = tmp_path / "geom_cache"
    cache.write_text("42 6.59375 1\n")
    monkeypatch.setattr(config, "GEOM_FILE", live)
    monkeypatch.setattr(config, "GEOM_CACHE", cache)
    assert config._read_geom_file().usable_px == 100.0


def test_read_geom_none_when_live_absent_and_no_cache(monkeypatch, tmp_path):
    import config
    monkeypatch.setattr(config, "GEOM_FILE", tmp_path / "absent")
    monkeypatch.setattr(config, "GEOM_CACHE", tmp_path / "also-absent")
    assert config._read_geom_file() is None


def test_cache_live_geom_persists_valid_live(monkeypatch, tmp_path):
    import config
    live = tmp_path / "live"
    live.write_text("100 5 1\n")
    cache = tmp_path / "sub" / "geom_cache"   # parent created by cache_live_geom
    monkeypatch.setattr(config, "GEOM_FILE", live)
    monkeypatch.setattr(config, "GEOM_CACHE", cache)
    config.cache_live_geom()
    assert cache.read_text() == "100 5 1\n"


def test_cache_live_geom_ignores_degenerate_and_absent(monkeypatch, tmp_path):
    import config
    cache = tmp_path / "geom_cache"
    monkeypatch.setattr(config, "GEOM_CACHE", cache)
    # Degenerate live geom: not persisted (would seed a nonsensical fit).
    live = tmp_path / "live"
    live.write_text("0 0 1\n")
    monkeypatch.setattr(config, "GEOM_FILE", live)
    config.cache_live_geom()
    assert not cache.exists()
    # Absent live geom: nothing to persist, cache untouched.
    monkeypatch.setattr(config, "GEOM_FILE", tmp_path / "absent")
    config.cache_live_geom()
    assert not cache.exists()


def test_auto_fit_panel_derives_knobs_from_geometry():
    cfg = Config(vertical=True)
    cfg.bar_panel.height = 3
    _auto_fit_panel(cfg, PanelGeometry(vertical=True, usable_px=42.0, glyph_adv=6.59375))
    # cols = floor(42/6.59375) = 6; width = floor((42-1)/(3*0.6)) = 22; pfs = round(22*3/6) = 11
    assert cfg.display.panel_min_width == 6
    assert cfg.bar_panel.width == 22
    assert cfg.display.panel_font_size == 11
    # the bar's footprint lands on cols → shared right edge, no wrap
    assert round(cfg.bar_panel.width * cfg.bar_panel.height / cfg.display.panel_font_size) == 6
    # panel spark/braille fill the width like the bar: length = cols
    assert cfg.spark_panel.cpu_spark_length == 6
    assert cfg.spark_panel.mem_spark_length == 6
    assert cfg.braille_panel.cpu_braille_length == 6
    assert cfg.braille_panel.mem_braille_length == 6


def test_auto_fit_bar_height_zero_uses_main_advance():
    # height 0 = bar at the plasmoid's font: width = cols, no custom divisor.
    cfg = Config(vertical=True)
    cfg.bar_panel.height = 0
    pfs_before = cfg.display.panel_font_size
    _auto_fit_panel(cfg, PanelGeometry(vertical=True, usable_px=42.0, glyph_adv=6.59375))
    assert cfg.display.panel_min_width == 6
    assert cfg.bar_panel.width == 6            # floor((42-1)/6.59375)
    assert cfg.display.panel_font_size == pfs_before  # untouched with height 0


def test_auto_fit_horizontal_sizes_column_height():
    cfg = Config(vertical=False)
    before = (cfg.display.panel_min_width, cfg.bar_panel.width)
    # main_px = 6.59375/0.6 = 10.99; column height = round(10.99*0.612) = 7 (digit height)
    _auto_fit_panel(cfg, PanelGeometry(vertical=False, usable_px=138.0, glyph_adv=6.59375))
    assert cfg.column_panel.height == 7
    # the vertical panel's knobs aren't touched in horizontal
    assert (cfg.display.panel_min_width, cfg.bar_panel.width) == before


def test_auto_fit_noop_when_geometry_unpublished():
    # no glyph_adv (plasmoid hasn't published): no orientation touches the config
    for vert in (True, False):
        cfg = Config(vertical=vert)
        snap = (cfg.display.panel_font_size, cfg.display.panel_min_width,
                cfg.bar_panel.width, cfg.column_panel.height)
        _auto_fit_panel(cfg, PanelGeometry(vertical=vert))
        assert (cfg.display.panel_font_size, cfg.display.panel_min_width,
                cfg.bar_panel.width, cfg.column_panel.height) == snap
    # vertical with glyph_adv but no usable_px: the fallback stands (width is needed)
    vcfg = Config(vertical=True)
    before_v = (vcfg.display.panel_min_width, vcfg.bar_panel.width)
    _auto_fit_panel(vcfg, PanelGeometry(vertical=True, glyph_adv=6.59375))
    assert (vcfg.display.panel_min_width, vcfg.bar_panel.width) == before_v


_ORIENT_TOML = """
[panel]
order = ["cpumem"]
[panel.cpumem]
items = ["cpu_usage", "mem_usage"]
[panel_horizontal.cpumem]
items = ["cpu_usage", "cpu_usage:spark", "mem_usage", "mem_usage:spark"]
[panel_vertical.cpumem]
items = ["cpu_usage", "cpu_usage:bar", "mem_usage", "mem_usage:bar"]
"""


def test_orientation_override_horizontal_picks_column(tmp_path):
    path = tmp_path / "config.toml"
    path.write_text(_ORIENT_TOML)
    cfg = load_config(path, vertical=False)   # forced horizontal
    assert cfg.panel.has("cpu_usage:spark") and cfg.panel.has("mem_usage:spark")
    assert not cfg.panel.has("cpu_usage:bar")
    assert cfg.vertical is False


def test_orientation_override_vertical_picks_bar(tmp_path):
    path = tmp_path / "config.toml"
    path.write_text(_ORIENT_TOML)
    cfg = load_config(path, vertical=True)    # forced vertical
    assert cfg.panel.has("cpu_usage:bar") and cfg.panel.has("mem_usage:bar")
    assert not cfg.panel.has("cpu_usage:spark")
    assert cfg.vertical is True


def test_column_panel_width_loads(tmp_path):
    # width is the column's manual knob (height is auto in horizontal Plasma,
    # so it isn't a value that just loads from the TOML as-is).
    path = tmp_path / "config.toml"
    path.write_text("[column_panel]\nwidth = 3\n")
    assert load_config(path).column_panel.width == 3
