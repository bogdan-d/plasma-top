# Plasma configuration

- `contents/config/main.xml` defines persisted keys and defaults; `contents/config/config.qml` defines settings pages; UI files edit values exposed through `plasmoid.configuration`.
- Reuse `plasmoid/package/contents/ui/libconfig/` helpers and their `configKey` contract before adding new controls or serialization code.
- Keep settings as behavior/data. Colors belong in CSS unless they are explicit user appearance choices; labels and icons retain their repository ownership.
- Preserve default-value semantics: several controls serialize an empty value to mean “follow theme/default.” Do not collapse empty and explicit values.
- Throttled writes and binding restoration in existing controls are deliberate. Trace focus, editing, accept/reject, and external configuration updates before simplifying them.
- Show desktop-only options only for planar placement. Panel settings must not accidentally mutate desktop appearance, or vice versa.
- User-visible strings use KDE translation functions.

When adding a setting, inspect schema, helper, QML consumer, Rust/config consumer if any, install/package manifests, and last-good reload behavior end to end.
