## Working model

PiroStats is a synchronous Rust daemon plus bundled Plasma applet. Rust under
`rust/src/` is production-authoritative; Python under `src/` remains only as the
temporary behavioral oracle during Phase 8 stabilization. `./pirostats` runs the
Rust checkout with repository assets.

Run commands from repository root. Development setup and full gates live in
`docs/DEVELOPMENT.md`; `python3 tools/python_oracle.py ...` is the supported
developer-facing Python oracle entrypoint.

## Architecture boundaries

- Item identity is `metric[:form]`: domain rules live in
  `rust/src/domain/{metric,form,item,registry}.rs`; render dispatch lives in
  `rust/src/render/registry.rs`.
- Pipeline: `rust/src/sensors/` -> `ReadingsSnapshot` ->
  `rust/src/render/{formatter,model,mono}.rs` -> table-free HTML.
- `rust/src/daemon.rs` owns lifecycle, reload, collection, publication, page
  wake, and shutdown. Host process/D-Bus/notification boundaries live in
  `rust/src/adapters.rs` and are injectable through `domain/boundary.rs`.
- Read `docs/LAYOUT.md` before changing `render/mono.rs`; real HTML tables are
  forbidden on render paths because Qt RichText layout is prohibitively costly.

## Load-bearing contracts

- Runtime root is a watched protocol, not scratch space. Only `panel.html` and
  `tooltip.html` persist directly under `<runtime>/`; atomic-write temporaries are
  transient. Changing state belongs under `<runtime>/state/`. See `rust/src/runtime/`.
- Keep config as data/behavior. Glyphs live in `style/icons.toml`, labels in
  `lang/*.toml`, colors in CSS. Mirror every selector/layout change between
  `style-dark.css` and `style-light.css`.
- Config merge order is defaults, detected machine, panel orientation, auto-fit.
  Preserve unknown/misplaced item warnings and last-good reload behavior.
- Tooltip pages are built only while active. Wheel/click commands and pinning are
  QML contracts; do not move them into daemon polling.
- Tooltip width is derived from maxed readings. Any new width-driving field must
  be bounded and covered by `canonical_width_covers_every_tooltip_item`.
- Qt RichText supports less CSS than browsers. Validate visual changes with
  `tools/qt_shot.py` or the Phase 6 QML matrix before theorizing from browser CSS.
- Keep all repository text English. Comments explain invariants, not mechanics.

## Validation

Use full Rust fmt/check/clippy/test/doc plus retained Python pytest/ruff/vulture
gates from `docs/DEVELOPMENT.md`. For intended HTML changes, update Python
goldens first with `UPDATE_GOLDEN=1 python3 -m pytest tests/test_golden_render.py`,
then prove Rust parity. Never weaken parity, lint, dead-code, or inventory gates.

Read `docs/ITEMS.md` for item behavior, `docs/PERFORMANCE.md` before poll/cache
changes, and `plans/STATUS.md` before migration/cutover work.
