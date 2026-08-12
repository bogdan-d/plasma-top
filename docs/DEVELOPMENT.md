# Development setup

Rust is the sole runtime implementation.

## Requirements

- Rust 1.97.1 with Cargo, rustfmt, and Clippy
- Python 3 for optional Qt/QML verification; PyQt6 and Qt SVG support for screenshot tools
- `kpackagetool6` and `plasmawindowed` for isolated applet verification

`tools/plasma_live_matrix.sh` is temporarily disabled while its Plasma SDK integration is repaired under `.scratch/plasmoidviewer-validation/`. Use the automated application-form checks below and perform panel or desktop representation checks only in an explicitly approved real Plasma session.

## Verification

```bash
cargo fetch --locked
git diff --exit-code -- Cargo.lock
cargo fmt -- --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo doc --no-deps
tools/repository_gate.sh
bash -n install.sh uninstall.sh packaging/aur/PKGBUILD \
  packaging/aur/plasma-top.install plasma-top tools/*.sh scripts/*.sh
tools/user_install_test.sh
tools/package_layout_test.sh
```

## Plasma and Qt verification

Use the smallest check that exercises the changed behavior:

| Change | Verification |
|---|---|
| Render, CSS, or Qt RichText | `tools/qt_render_matrix.sh --no-build` |
| Applet/runtime integration | `tools/qml_verify.sh --smoke` |
| Interactive application-form behavior | `tools/qml_verify.sh` |
| Panel orientation, geometry, hover, pinning, or wheel behavior | Manual real-session verification with explicit approval |
| Desktop representation or appearance settings | Manual real-session verification with explicit approval |

`plasmawindowed` cannot emulate panel form factors, so do not treat an application-form pass as panel evidence. The automated scripts use disposable XDG roots and do not modify the installed widget, production runtime, or real Plasma configuration.

Run `tools/qt_render_matrix.sh --no-build` on a host with PyQt6 and Qt SVG support for render/QML changes or release verification. Set `PYTHON` when PyQt6 is not available through `python3`. On immutable hosts, run it without layering packages:

```bash
uv run --with PyQt6 -- bash -c \
  'PYTHON="$(command -v python)" exec tools/qt_render_matrix.sh --no-build'
```

The Qt matrix writes rendered HTML, PNGs, logs, an environment manifest, and a contact sheet under `.test-artifacts/plasma/qt/`. Inspect `.test-artifacts/plasma/qt/contact-sheet.png`; a passing rasterization is not a visual review.

Optional `qmllint` checks can find syntax, import, type, binding, and deprecated-API problems, but Plasma metadata and dynamic context properties can produce false positives. Lint is not a substitute for runtime verification.

For a release candidate, perform a separate real-session pass only when modifying the user's installed widget is acceptable. Check the relevant panel and desktop placements, tooltip alignment, hover and pinning, wheel paging, resizing, settings persistence, light/dark theme changes, font and display scale, multi-monitor placement when available, and login/service recovery when installer behavior changed. Do not restart `plasmashell` as part of routine development verification.

`./plasma-top` runs the Rust checkout with repository assets:

```bash
./plasma-top render
./plasma-top probe --config config/config.toml
./plasma-top list-items
```

Scheduler profiling keeps the existing cold/warm one-shot command and adds a runtime-file-free timed async mode:

```bash
./plasma-top profiling --config config/config.toml
./plasma-top profiling --config config/config.toml --duration 30 --scenario hidden
./plasma-top profiling --config config/config.toml --duration 30 --scenario graphs
./plasma-top profiling --config config/config.toml --duration 5 --scenario main --stimuli
```

Timed mode without `--stimuli` is a steady scenario and performs no page, presentation, or config mutations. `--stimuli` explicitly opts into one serial bounded page/presentation/config sequence through the normal inotify, reload, scheduler, and in-memory publication paths; aggregate counters in that report include stimulus work. The user's config and runtime are not modified. Slow and short runs leave an action pending until its exact watcher/scheduler-observed state and correlated tooltip publication complete, then report timeout state separately as protocol-requested and watcher/scheduler-observed values.

`install.sh` and `packaging/aur/PKGBUILD` both build with `--locked`; package launchers set `PLASMA_TOP_CODE_ROOT` and execute the installed native binary. `tools/package_layout_test.sh` verifies their shared manifest, legacy-layout upgrade, repeat upgrade, uninstall, and user-file preservation contracts.

Python is not a runtime, build, lint, or baseline CI dependency. Optional Qt/QML verification uses Python 3; `tools/qt_shot.py` additionally needs PyQt6. Fixed compatibility snapshots and fixtures live under `tests/`.

Live hardware and safe mutation coverage that cannot be reproduced in fixtures is tracked in `.scratch/live-hardware-and-mutation-validation/spec.md`.
