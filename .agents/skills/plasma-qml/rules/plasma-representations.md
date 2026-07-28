# Plasma representations and form factors

- `Plasmoid.compactRepresentation` is panel/icon-scale UI; `Plasmoid.fullRepresentation` is popup/full UI. Plasma may instantiate them lazily and switch behavior by containment.
- Use `plasmoid.formFactor` and `plasmoid.location` for actual placement rules, not window dimensions guessed from `plasmawindowed`.
- Desktop planar mode differs from panel mode: it can be resized directly and owns different background behavior.
- Preferred and minimum dimensions affect panels, popups, desktop widgets, and system-tray containment differently. Test the target containment.
- Do not move pinning into daemon polling. `Plasmoid.hideOnWindowDeactivate`, expansion, and popup interaction remain QML/Plasma concerns.

For PlasmaTop, compact display, hover tooltip, pinned popup, and desktop display share daemon output but have distinct ownership and geometry. Validate every surface touched by a change; application-form smoke cannot prove panel behavior.
