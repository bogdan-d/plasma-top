# QML and Qt Quick performance

- Measure before optimizing. For this repo, inspect `docs/PERFORMANCE.md` before changing polling, watching, caching, RichText parsing, or publication cadence.
- Avoid unnecessary wrappers, clipping, layers, subtree opacity, shader effects, and frequently repainted `Canvas` content. Each can add traversal or offscreen rendering work.
- Keep delegates and frequently evaluated bindings small. Do not call shell commands, parse large text, or allocate large arrays from a hot binding.
- Pause invisible animation and timer work explicitly.
- Prefer existing watched-file wakeups over adding free-running timers.
- Do not infer Qt RichText behavior from browsers. Unsupported CSS may be ignored, while expensive constructs can still trigger costly layout.

PlasmaTop renders table-free, monospace-aligned HTML because real HTML tables caused severe Qt RichText CPU cost. Never add `<table>` on a render path. Mirror CSS selector/layout changes in dark and light styles and prove visual behavior with `tools/p6_qt_matrix.sh` or `tools/qt_shot.py`.
