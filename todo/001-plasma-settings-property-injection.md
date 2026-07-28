# Plasma settings property injection

## Status

Deferred. Highest-priority open issue. Requires a supported Plasma session.

## Problem

Plasma 6.7.3 logged these warnings while loading `ConfigAppearance.qml`:

```text
QML ConfigAppearance: Created graphical object was not placed in the graphics scene.
Setting initial properties failed: ConfigAppearance does not have a property called cfg_bold
Setting initial properties failed: ConfigAppearance does not have a property called cfg_clickCommand
```

The second warning repeated for generated settings covering fonts, colors,
dimensions, click/wheel commands, outlines, and backgrounds. Panel rendering,
tooltip rendering, click, wheel paging, pinning, and backend config/style reload
worked. The settings dialog itself was not exercised, so user impact is unknown.

## Relevant files

- `plasmoid/package/contents/config/config.qml`
- `plasmoid/package/contents/config/main.xml`
- `plasmoid/package/contents/ui/config/ConfigAppearance.qml`
- `plasmoid/package/contents/ui/libconfig/FormKCM.qml`
- keyed controls under `plasmoid/package/contents/ui/libconfig/`

## Handoff

1. Install or run the current checkout in a supported Plasma session.
2. Capture a focused Plasma journal while opening PlasmaTop settings.
3. Verify every visible field loads its current value, saves, and survives a
   Plasma restart.
4. Compare root properties and page loading with current Plasma 6 KConfig
   conventions.
5. Determine whether warnings come from PlasmaTop, stale configuration schema,
   or upstream Plasma behavior.
6. Fix QML integration only when reproduction proves a PlasmaTop defect. Do not
   add a Rust workaround for frontend property injection.

## Done when

- Settings load/save/restart behavior is verified.
- Warning is understood and either fixed or documented as harmless upstream
  behavior.
- `tools/qml_verify.sh --smoke` and relevant checks in `docs/DEVELOPMENT.md` pass.

