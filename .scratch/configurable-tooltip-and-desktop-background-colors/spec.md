# Configurable tooltip and desktop background colors

## Status

Feature request from live Plasma testing.

## Problem

Appearance settings can toggle the desktop widget background, but cannot choose its color. The tooltip background also follows Plasma without an applet-level color override. Users should be able to choose tooltip and desktop background colors independently, including transparency, while retaining theme-derived defaults.

## Current behavior

`showBackground` selects Plasma's desktop `DefaultBackground`. The desktop text and outline colors are configurable only when that background is disabled. No KConfig entry represents either requested background color, and the tooltip shell currently owns its background.

## Relevant files

- `plasmoid/package/contents/config/main.xml`
- `plasmoid/package/contents/ui/config/ConfigAppearance.qml`
- `plasmoid/package/contents/ui/libconfig/ColorField.qml`
- `plasmoid/package/contents/ui/main.qml`

## Handoff

1. Define empty values as “follow Plasma theme” so existing users retain current behavior.
2. Keep tooltip and desktop colors independent and do not affect panel rendering.
3. Determine whether the tooltip shell can be styled directly or needs an applet-owned background without breaking hover, pinning, sizing, or shadows.
4. Reuse `ColorField` and preserve Apply/Cancel staging.
5. Validate opaque, translucent, and fully transparent values in both light and dark Plasma themes.

## Done when

- Appearance settings expose independent tooltip and desktop background colors.
- Theme defaults remain available without guessing a color value.
- Alpha, scaling, rounded corners, borders, and tooltip geometry render correctly in real Plasma.
- QML smoke, package checks, and relevant live visual checks pass.
