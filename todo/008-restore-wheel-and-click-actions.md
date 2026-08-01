# Restore and validate wheel and click actions

## Status

Interaction failure observed during temporary live testing. The reported correlation with the two latest QML commits is unproven because those commits did not change pointer handlers.

## Problem

Scrolling did not cycle the tooltip's three pages and clicking the widget did not run its configured action. Both paths dispatch KConfig command strings that default to `/usr/bin/plasma-top`; that binary was absent during the applet-only test, making the test environment the likely shared cause rather than two independent regressions.

## Relevant files

- `plasmoid/package/contents/ui/main.qml`
- `plasmoid/package/contents/config/main.xml`
- `src/daemon.rs`
- `src/runtime/page.rs`
- `src/page_commands.rs`
- `tools/plasma_live_matrix.sh`
- `tools/qml_verify.sh`

## Handoff

1. Reproduce first with a complete supported installation where `/usr/bin/plasma-top`, the daemon, applet, runtime root, and command defaults come from the same build.
2. Trace wheel and click command execution separately before editing QML.
3. Confirm one wheel notch selects one page in each direction, repeated gestures are serialized, and page changes wake the tooltip.
4. Confirm the intended default click action; current code launches `plasma-systemmonitor`, not the settings page.
5. Improve checkout/live validation so applet-only installs route commands to the checkout binary instead of silently testing an absent `/usr/bin/plasma-top`.
6. Preserve configurable command overrides and nonce-based watcher wakeups.

## Done when

- Wheel paging and the agreed click action work in a complete installed package.
- Checkout validation cannot silently lose wheel and click commands because the production binary path is absent.
- Real panel tests cover both wheel directions, rapid notches, hover/pinned tooltip state, and click activation.
- Focused runtime tests, QML smoke, package tests, and full gates pass.
