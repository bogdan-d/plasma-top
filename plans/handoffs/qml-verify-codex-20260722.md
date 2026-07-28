# QML-VERIFY handoff — Codex — 2026-07-22

## Scope

- Lane: `QML-VERIFY`, Phase 6 P6.1–P6.3.
- Verified the unchanged applet against the Rust daemon.
- Changed verification tools only; no production QML, config, CSS, or runtime
  contract changed.
- Packaging P6.4/P6.5 remains owned by `PACKAGING`.

## Audit of prior untracked work

The initial scripts were not valid gate evidence:

- launched mixed-case `com.github.lucazade.piroStats`, which did not exist;
- wrote `state/geom` themselves before claiming QML geometry publication;
- invoked page commands directly instead of observing QML actions;
- swallowed failures and allowed screenshot dimension mismatches;
- used `plasmawindowed` as a panel test despite its Application form factor;
- rendered fastfetch ANSI bytes without the applet's output conversion.

The scripts were replaced rather than incrementally accepted.

## Changed paths

- `tools/qml_verify.sh`: explicitly limited to Application-form smoke.
- `tools/p6_live_matrix.sh`: disposable horizontal/vertical/planar harness using
  `plasmoidviewer`, actual QML command tracing, controlled pointer position,
  geometry/orientation checks, watcher/lazy-read checks, and strict cleanup.
- `tools/p6_qt_matrix.sh`: strict Rust/Qt screenshot matrix for panel H/V, main
  tooltip, and all deep pages under dark/light/overlay variants.
- `tools/p6_png_diff.py`: dimension/mean/max-pixel/differing-fraction comparator.
- `tools/qt_shot.py`: optional conversion matching `main.qml`'s ANSI/newline
  output path, required for faithful fastfetch screenshots.
- `plans/{STATUS,INVENTORY}.md`: integration evidence and dispositions.

## Environment

- Host: Fedora Atomic/Bazzite, Plasma 6.7.2 Wayland.
- PyQt/Qt screenshot engine: Qt 6.11.0 / PyQt 6.11.0.
- `plasmoidviewer` 6.7.3 runs from Distrobox `pirostats-plasma-sdk` because the
  immutable host lacks `plasma-sdk`; container also contains `plasma-workspace`
  for `org.kde.desktopcontainment`.
- Harness exports only disposable XDG/HOME/runtime roots into the viewer.

## Verification

```text
bash -n tools/qml_verify.sh tools/p6_live_matrix.sh tools/p6_qt_matrix.sh
python3 -m py_compile tools/qt_shot.py tools/p6_png_diff.py
tools/qml_verify.sh --smoke --no-build
tools/p6_live_matrix.sh --no-build
QT_QPA_PLATFORM=offscreen tools/p6_qt_matrix.sh --no-build
```

Results:

- Application smoke passed with correct lowercase applet id.
- Horizontal geometry: `96 10.984375 0 15`.
- Vertical geometry: `80 10.984375 1 15`.
- Panel watcher reads advanced while unhovered tooltip reads stayed stable.
- Runtime root contained only `panel.html`, `state/`, and `tooltip.html`.
- QML logs contained no load/runtime/binding errors.
- Qt matrix rendered 24 images: eight components/pages × three CSS variants.
- Rust and Python golden suites ran inside the Qt matrix before rasterization.
- Every HTML output remained table-free; every PNG decoded and had nonzero size.
- Contact sheet was inspected at `.test-artifacts/p6/qt/contact-sheet.png`.
- Human interactive pass confirmed hover/live tooltip, middle-click pin/unpin,
  wheel gesture grouping and quick reverse, resize behavior, planar transparent
  text, background/outline toggles, font resizing, config page, and desktop
  wheel paging.
- Command trace recorded real QML `page next`, `page prev`, panel/tooltip reads,
  and click dispatch under `.test-artifacts/p6/live/commands-interactive.tsv`.
- Harness cleanup left no viewer/daemon process and no production runtime writes.

## Parity and deviations

- No applet behavior workaround was needed.
- No accepted deviation.
- Existing presentation screenshots remain preserved; fixed-host P6 evidence is
  generated under ignored `.test-artifacts/p6/` rather than replacing them.

## Remaining work

- `PACKAGING`: P6.4/P6.5 install/upgrade/uninstall/rollback evidence.
- Phase 7 live hardware matrix remains blocked on unavailable Intel/NVIDIA,
  battery/UPower, and Bolt devices.

