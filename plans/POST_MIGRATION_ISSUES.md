# Post-migration issue log

Follow-ups discovered during the Rust rewrite but intentionally deferred until
the full migration plan is complete. These are not accepted deviations or
cutover blockers. Investigate them after P8.5 unless their impact becomes
user-visible earlier.

| ID | Status | Summary | Found in |
|---|---|---|---|
| PM001 | deferred | Plasma rejects generated `cfg_*` properties when loading `ConfigAppearance.qml` | P8.2 live stabilization, 2026-07-29 |

## PM001 — `ConfigAppearance.qml` property injection warnings

### Evidence

Plasma 6.7.3 on a KDE Wayland session logged:

```text
QML ConfigAppearance: Created graphical object was not placed in the graphics scene.
Setting initial properties failed: ConfigAppearance does not have a property called cfg_bold
Setting initial properties failed: ConfigAppearance does not have a property called cfg_clickCommand
```

The second warning repeated for generated settings covering fonts, colors,
dimensions, click/wheel commands, outlines, and backgrounds. Full evidence
context is in `handoffs/cutover-p8.2-20260729.md`.

### Known impact

Panel rendering, tooltip rendering, click, wheel paging, pinning, and backend
config/style hot reload all passed. P8.2 did not open and exercise the widget's
settings dialog, so settings-dialog impact remains unknown.

### Follow-up

After P8.5:

1. Reproduce by opening PiroStats settings on the supported Plasma version.
2. Check whether every field loads, saves, and survives a Plasma restart.
3. Capture a focused Plasma journal excerpt while opening and applying settings.
4. Compare `ConfigAppearance.qml` root properties and config-page loading with
   current Plasma 6 KConfig conventions.
5. Fix the QML integration only if reproduction confirms a PiroStats defect; do
   not add a Rust workaround for a frontend property-injection failure.

Close PM001 when the warning is understood and either fixed with settings-dialog
verification or documented as harmless upstream Plasma behavior.
