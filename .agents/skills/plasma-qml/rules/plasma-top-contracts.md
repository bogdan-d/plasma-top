# PlasmaTop QML boundary

Rust is the sole runtime implementation. QML displays published output, reports geometry, and owns direct interaction; it is not another sensor or polling runtime.

Preserve these contracts:

- The daemon is reached only through the versioned session-bus interface `com.github.bogdan_d.PlasmaTop1` (ADR 0003). One `Properties` binding receives `PanelHtml`, `TooltipHtml`, `Page`, and `PageCount`; methods use `SessionBus.asyncCall` with explicit signatures `(i)`, `(t)`, and `(ddbd)` so nothing introspects before a call.
- Received values and displayed text stay separate. Empty values and daemon absence never clear displayed text; tooltip text updates only while presented and catches up on presentation.
- On every daemon registration the applet starts a new service lifetime that ignores older replies, calls `updateAll()`, resends geometry, and presents again if presented.
- The presentation lease keeps one call in flight, has no heartbeat, and dismisses on destruction even with a call in flight, because a removed widget does not close plasmashell's shared bus connection.
- Middle-click and the settings pages keep their CLI transport; the click command adds a nonce so the executable DataSource never drops a repeated source.
- One wheel gesture changes one page; QML owns gesture grouping.
- Middle-click pinning remains QML-owned.
- Geometry is sent only when panel width and both glyph advances are positive; it feeds daemon orientation merge and auto-fit.
- Qt RichText, not browser HTML/CSS, is the rendering target. HTML tables are forbidden and tooltip width must cover bounded canonical readings.
- Dark/light selector and layout changes stay mirrored.

Read `main.qml`, `PresentationLease.qml`, `src/adapters/applet.rs`, `src/daemon/async_loop/applet.rs`, relevant render code, and design/performance docs before changing either side of a boundary. Prefer one protocol fix over compensating logic on both sides.
