# Validation ladder

Choose checks by changed behavior, starting with the smallest sufficient rung. Required repository gates remain defined by `docs/DEVELOPMENT.md`.

1. QML/package text only: inspect diff; run relevant repository and shell gates.
2. Render/CSS/RichText: run `tools/qt_render_matrix.sh --no-build` on a host or approved environment with PyQt6 and Qt SVG support; inspect contact sheet.
3. Applet/runtime integration: run `tools/qml_verify.sh --smoke` in Plasma.
4. Interactive application-form behavior: run `tools/qml_verify.sh`.
5. Panel orientation, hover, pinning, and wheel behavior: run `tools/plasma_live_matrix.sh`; `plasmawindowed` cannot emulate panel form factors.

Optional static tools:

- `qmllint`: syntax, types, imports, bindings, deprecated APIs. Plasma modules need matching import/type metadata; review false positives manually.
- `qmlformat`: formatting aid, never a reason for unrelated whole-file churn.
- `qmlls`: editor diagnostics; same import-metadata limitation as `qmllint`.
- `qmlimportscanner`: inspect module resolution when imports are the problem.

Atomic-host policy: do not layer packages. Before creating a Distrobox, installing packages there, or exporting wrappers, list the exact image, packages, binaries, paths, and purpose and obtain approval. Prefer a project-local wrapper invoking `distrobox enter` over copying container-linked binaries onto the host.

Never restart or replace `plasmashell`, install/uninstall the user's real widget, or write production runtime/config state without explicit approval. Existing repository scripts use disposable XDG roots; prefer them.
