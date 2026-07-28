# Replace Qt5Compat graphical effects

## Status

Low-priority maintenance. Current code works but imports a Qt 5 compatibility
module deprecated in Qt 6.

## Problem

`plasmoid/package/contents/ui/libconfig/ColorField.qml` uses
`Qt5Compat.GraphicalEffects.ConicalGradient` to draw the checkerboard beneath
transparent colors.

## Handoff

1. Confirm supported Plasma/Qt versions and available native Qt 6/Kirigami APIs.
2. Replace only the checkerboard implementation; avoid a new dependency.
3. Preserve circle clipping, alpha preview, border, scaling, and light/dark
   appearance.
4. Validate inside the real settings dialog, not only a browser or generic QML
   viewer.

## Done when

- `Qt5Compat.GraphicalEffects` is absent.
- Preview is visually equivalent at common scale factors and alpha values.
- `tools/qml_verify.sh --smoke` and the Qt checks in `docs/DEVELOPMENT.md` pass.

