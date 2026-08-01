# Friendly color alpha controls

## Status

Feature request from live settings testing.

## Problem

`ColorField` accepts `#AARRGGBB`, but users must know that an eight-digit value is supported and manually choose the alpha byte. The current desktop color callers disable the native dialog's alpha channel, so transparency has no discoverable control.

## Relevant files

- `plasmoid/package/contents/ui/libconfig/ColorField.qml`
- `plasmoid/package/contents/ui/config/ConfigAppearance.qml`
- `plasmoid/package/contents/config/main.xml`
- `todo/005-color-dialog-rejection-semantics.md`

## Handoff

1. Choose the smallest discoverable UI supported by the Plasma color dialog, such as enabling its alpha channel or adding an explicit opacity control.
2. Keep raw hex input available and make the accepted ordering clear.
3. Preserve empty/default values, three- and six-digit input, staged Apply/Cancel behavior, and live preview.
4. Keep this work separate from rejection semantics in todo 005 unless one focused change safely resolves both.
5. Verify keyboard access and common scale factors in the real settings dialog.

## Done when

- Users can set opacity without calculating an alpha byte.
- The control visibly communicates the chosen opacity.
- Typed and picker-produced colors round-trip without silently changing alpha or default semantics.
- Real settings-dialog verification and QML smoke pass.

