# QML bindings, properties, and signals

- Prefer declarative bindings for derived state. An imperative assignment to a
  bound property removes its binding; use `Qt.binding()` only when restoration
  is intentional.
- Avoid binding loops and hidden dynamic-scope dependencies. Use explicit IDs
  for cross-object access and qualified model roles in delegates.
- Use QML properties for observable state. Local JavaScript variables are for
  temporary computation, not UI state expected to trigger updates.
- Keep hot bindings cheap and side-effect free. Cache stable derived values in
  `readonly property` values where that reduces repeated work.
- Check `Loader.status` or react to loading before dereferencing `Loader.item`.
- Disconnect or destroy dynamically connected objects when ownership does not
  already provide cleanup.
- Preserve existing equality semantics unless the change specifically requires
  coercion behavior to change; broad style rewrites hide functional changes.
- Use typed properties and signal parameters where practical. Keep dynamic
  `var` only for genuinely heterogeneous QML/JavaScript values.

For this applet, `plasmoid`, `Plasmoid`, `StandardPaths`, and Plasma data-engine
objects are runtime-provided context/API surfaces. Do not rewrite them into a
standalone-application architecture to satisfy generic static analysis.
