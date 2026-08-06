# Add event and presentation protocol

Type: task
Status: ready-for-agent
Blocked by: 05

## Objective

Eliminate daemon file polling and stop hidden tooltip work through a QML presentation lease.

## Scope

- Enable nix inotify and integrate nonblocking watches through Tokio `AsyncFd` for config, machine config, style, page, geometry, status files, and presentation leases.
- Watch stable parent directories, debounce logical sources, and rescan/re-arm on overflow, ignored watches, and directory recreation; fail clearly when required initial watches cannot be installed.
- Add validated numeric `present`/`dismiss` internal CLI commands and per-instance lease files under runtime `state/presented/`.
- Update QML hover, pin, planar/full representation, destruction, and 30 s heartbeat handling; daemon expires leases after 90 s and applies 1 s deactivation grace.
- Stop main-tooltip/page jobs and tooltip writes while hidden, retain last samples/file, perform one bounded activation refresh, and cancel disposable page work after grace.
- Require matched daemon/QML package-session restart and document the upgrade constraint.

## Acceptance

- Only `panel.html` and `tooltip.html` persist directly under runtime root.
- Multiple applet instances cannot hide work while another tooltip remains presented.
- Hover, pin, planar representation, page switching, applet removal, plasmashell crash, stale expiry, daemon restart, and login races behave correctly.
- No free-running QML polling is added beyond heartbeat while presented.

## Validation

Run Rust/QML focused tests, `tools/qml_verify.sh`, `tools/plasma_live_matrix.sh`, and relevant full gates from `docs/DEVELOPMENT.md`. Do not restart plasmashell as routine automation.
