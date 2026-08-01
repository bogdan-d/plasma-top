# Revisit default monospace font and tooltip size

## Status

UX request from live Plasma testing.

## Problem

The applet defaults to `NotoSansM Nerd Font Mono`, panel size 8, and tooltip size 11. The tooltip appears too large, and the explicit font may not be the most broadly available monospace choice across supported KDE distributions.

## Constraint

PlasmaTop uses Nerd Font glyphs, so switching to a common distro monospace font without a glyph strategy can produce missing icons. A generic `monospace` fallback is portable but does not itself guarantee Nerd Font coverage.

## Relevant files

- `plasmoid/package/contents/config/main.xml`
- `plasmoid/package/contents/ui/config/ConfigAppearance.qml`
- `plasmoid/package/contents/ui/libconfig/FontFamily.qml`
- `plasmoid/package/contents/ui/main.qml`
- `README.md`
- `src/diagnostics.rs`

## Handoff

1. Survey default fixed-width fonts available on supported KDE distributions instead of assuming one host's font set.
2. Decide whether to use a portable family, a fallback strategy, or the current Nerd Font requirement while preserving every glyph.
3. Compare smaller tooltip sizes in horizontal and vertical panels, desktop placement, pinned popups, and all tooltip pages.
4. Verify canonical width, line height, clipping, pager alignment, and high-DPI rendering after changing defaults.
5. Apply new defaults only to fresh applet configurations unless an explicit migration is justified.

## Done when

- The chosen default is documented, broadly available under the supported installation story, and renders all required glyphs.
- Tooltip text is smaller while remaining readable and unclipped at common scale factors.
- README requirements, diagnostics fallback, KConfig defaults, visual checks, and package tests agree.

