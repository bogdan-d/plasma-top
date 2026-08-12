# Fix plasmoidviewer validation scripts

Type: task
Status: needs-info

## Objective

Make `tools/plasma_live_matrix.sh` use KDE's `plasmoidviewer` correctly and produce trustworthy horizontal, vertical, and planar Plasma applet validation evidence.

## Previous implementation

Commit `0d850ec9b3b0387b08bb780a99dc6ff709aaf032` disables the unreliable workflow and preserves its removal diff. Inspect the previous script with `git show 0d850ec^:tools/plasma_live_matrix.sh` or the exact removal with `git show 0d850ec -- tools/plasma_live_matrix.sh`.

## Scope

- Capture the current failure mode, host session type, Plasma/Plasma SDK versions, launch command, Distrobox configuration when applicable, and logs before changing the integration.
- Verify the supported `plasmoidviewer` invocation and form-factor arguments against the installed Plasma SDK version instead of assuming the current command shape is valid.
- Fix host and Distrobox environment forwarding, package/runtime paths, process ownership, readiness detection, interaction coordinates, and cleanup only where reproduced evidence shows they are wrong.
- Keep all test state in disposable XDG roots and preserve the existing production runtime, installed applet, and Plasma session.
- Restore automatic assertions only when they observe real applet behavior reliably; mark session-specific limitations as skipped rather than passing them implicitly.
- Re-enable the live-matrix workflow in `docs/DEVELOPMENT.md` only after horizontal, vertical, and planar runs succeed and their artifacts have been inspected.
- Do not add another applet harness or treat `plasmawindowed` application form as panel-form-factor evidence.

## Acceptance

- The original failure is reproducible and documented with enough environment detail to distinguish script defects from unsupported Plasma SDK behavior.
- Host and approved Distrobox execution either work consistently or fail early with an actionable, accurate prerequisite error.
- Horizontal, vertical, and planar launches use the intended representation and remain alive through readiness checks.
- Presentation leases, geometry publication, watcher refresh, lazy tooltip reads, multiple instances, and runtime-root discipline are asserted only where the harness can observe them reliably.
- Cleanup terminates only processes started by the run and leaves no disposable package, runtime, or viewer process behind.
- Current development guidance accurately describes the repaired workflow and its remaining Wayland/X11 limitations.

## Validation

Run focused script checks on the affected Plasma session, inspect the generated logs and artifacts, then run `shellcheck`, `shfmt`, `tools/qml_verify.sh --smoke`, and the full repository gates from `docs/DEVELOPMENT.md`. Do not restart `plasmashell` or modify the user's installed widget as routine automation.

## Needed information

- A failing command and its complete output from the environment where the current script does not work.
- Host Plasma version, session type, Plasma SDK version, and whether `plasmoidviewer` is native, exported, or run through Distrobox.
