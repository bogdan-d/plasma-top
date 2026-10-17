# QML lifecycle

- Plasma creates full representations lazily. `fullRepresentationItem` may be null until opened; do not treat it as always alive.
- Use `Loader.active: false` to release optional dialogs or heavy UI. Let loader ownership handle destruction unless a different lifetime is required.
- Stop timers, watchers, animations, and polling when their work is inactive. Visibility alone does not guarantee that ticking stops.
- Keep applet-wide interaction state on the root when compact, full, and pinned surfaces must share it. Keep representation-only state inside its component.
- Guard asynchronous or externally delivered callbacks against object teardown and stale state. A callback can arrive after visibility or expansion changes.
- Prefer signal handlers over polling when Plasma or the daemon's property signals already publish the event.

PlasmaTop-specific lifecycle: tooltip text is laid out only while hovered, expanded, pinned, or planar; tooltip page generation remains daemon-side only while active. Wheel gestures and clicks remain QML-owned and event-driven.
