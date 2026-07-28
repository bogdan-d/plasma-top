# PlasmaTop QML boundary

Rust is the sole runtime implementation. QML displays published output, reports geometry, and owns direct interaction; it is not another sensor or polling runtime.

Preserve these contracts:

- Runtime root is watched protocol state. Only `panel.html` and `tooltip.html` persist directly below it; mutable state belongs under `state/`.
- Stable panel/tooltip `cat` command strings identify watched sources. Fire-and- forget wheel/click commands add a nonce so repeated empty output still wakes.
- Tooltip file reads stay gated on hover, expansion, or pinning.
- One wheel gesture changes one page; QML owns gesture grouping.
- Middle-click pinning remains QML-owned.
- Geometry publication feeds daemon orientation merge and auto-fit.
- Qt RichText, not browser HTML/CSS, is the rendering target. HTML tables are forbidden and tooltip width must cover bounded canonical readings.
- Dark/light selector and layout changes stay mirrored.

Read `main.qml`, `rust/src/page_commands.rs`, `rust/src/runtime/`, relevant render code, and design/performance docs before changing either side of a boundary. Prefer one protocol fix over compensating logic on both sides.
