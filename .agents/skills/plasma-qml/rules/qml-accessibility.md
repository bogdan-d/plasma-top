# Controls, input, and accessibility

- Prefer Plasma/Kirigami/Qt Quick Controls to hand-built controls so theme, focus, keyboard, and accessibility behavior come from the platform.
- Custom interactive items need an accessible name/role, keyboard reachability, visible focus, and an activation path equivalent to pointer input.
- Use theme colors and preserve contrast. Do not communicate warning state only through color; retain text or glyph meaning.
- Use `i18n()`/`i18nc()` for Plasma-facing user text. Keep placeholders inside the translatable string rather than concatenating sentence fragments.
- Give icon-only controls tooltips or accessible names. Use symbolic theme icons where Plasma expects them.
- Treat wheel bursts as gestures when the interaction contract is one action per gesture. Do not replace the existing leading-edge plus idle-reset behavior with a fixed-window debounce.
- Keep middle-click pinning, hover behavior, and wheel actions consistent across compact and pinned surfaces.
