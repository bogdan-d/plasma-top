# Color dialog rejection semantics

## Status

Low-priority UX issue.

## Problem

`ColorDialog.onRejected` in `plasmoid/package/contents/ui/libconfig/ColorField.qml` restores the initial color. Qt also emits `rejected` when the user clicks outside the modal, so an outside click behaves like explicit Cancel. Desired behavior is not yet proven possible with the supported Qt API.

## Handoff

1. Reproduce Cancel, Escape, window close, and outside-click behavior on each supported Plasma/Qt version.
2. Check whether Qt exposes the rejection reason or another signal that reliably distinguishes explicit cancellation.
3. Define expected outside-click behavior before editing code.
4. Keep live preview, Apply/OK, empty/default color, and alpha behavior intact.
5. If Qt cannot distinguish the cases, document the platform limitation and remove the stale source TODO instead of adding event-filter complexity.

## Done when

- Explicit Cancel restores the opening color.
- Accepted selection persists through Plasma configuration.
- Outside-click behavior is intentional and documented.
- Real settings-dialog verification and `tools/qml_verify.sh --smoke` pass.

