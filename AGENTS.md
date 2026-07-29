## Working model

PlasmaTop is a synchronous Rust daemon plus bundled Plasma applet. Rust under `rust/src/` is the sole runtime implementation. `./plasma-top` runs the Rust checkout with repository assets.

Run commands from repository root. Development setup and full gates live in `docs/DEVELOPMENT.md`.

## Architecture boundaries

- Item identity is `metric[:form]`: domain rules live in `rust/src/domain/{metric,form,item,registry}.rs`; render dispatch lives in `rust/src/render/registry.rs`.
- Pipeline: `rust/src/sensors/` -> `ReadingsSnapshot` -> `rust/src/render/{formatter,model,mono}.rs` -> table-free HTML.
- `rust/src/daemon.rs` owns lifecycle, reload, collection, publication, page wake, and shutdown. Host process/D-Bus/notification boundaries live in `rust/src/adapters.rs` and are injectable through `domain/boundary.rs`.
- Read `docs/LAYOUT.md` before changing `render/mono.rs`; real HTML tables are forbidden on render paths because Qt RichText layout is prohibitively costly.

## Load-bearing contracts

- Runtime root is a watched protocol, not scratch space. Only `panel.html` and `tooltip.html` persist directly under `<runtime>/`; atomic-write temporaries are transient. Changing state belongs under `<runtime>/state/`. See `rust/src/runtime/`.
- Keep config as data/behavior. Glyphs live in `style/icons.toml`, labels in `lang/*.toml`, colors in CSS. Mirror every selector/layout change between `style-dark.css` and `style-light.css`.
- Config merge order is defaults, detected machine, panel orientation, auto-fit. Preserve unknown/misplaced item warnings and last-good reload behavior.
- Tooltip pages are built only while active. Wheel/click commands and pinning are QML contracts; do not move them into daemon polling.
- Tooltip width is derived from maxed readings. Any new width-driving field must be bounded and covered by `canonical_width_covers_every_tooltip_item`.
- Qt RichText supports less CSS than browsers. Validate visual changes with `tools/qt_shot.py` or `tools/p6_qt_matrix.sh` before theorizing from browser CSS.
- Keep all repository text English. Comments explain invariants, not mechanics.
- Never hard-wrap prose, docs, or comments to a fixed column. Write each paragraph and comment sentence as a single line; let viewers reflow.

## Shell scripts

- Run `shellcheck` on every shell script you change, when it is installed.
- Before finishing a session that touched `*.sh`, run `shfmt` across the changed files; `.editorconfig` sets the 4-space style.
- Silence a shellcheck finding only with a directive plus a one-line invariant comment, not by weakening checks.

## Validation

Use the full Rust fmt/check/clippy/test/doc gates from `docs/DEVELOPMENT.md`. Never weaken required test evidence, lint, or dead-code checks.

Read `docs/ITEMS.md` for item behavior and `docs/PERFORMANCE.md` before poll/cache changes. Deferred work lives in `todo/`.
