# QML layout and geometry

- Do not combine anchors and `Layout.*` sizing on the same layout-managed item.
- Direct children of `RowLayout`, `ColumnLayout`, or `GridLayout` should express
  negotiated size with `Layout.*`; plain `width` and `height` may be ignored.
- Use `Row`/`Column` for simple fixed arrangements and layouts when children
  must negotiate available space.
- Prefer `implicitWidth`/`implicitHeight` for reusable components. Do not bind
  parent size to child size while the child also fills the parent.
- Use Kirigami or Plasma spacing and icon-size units for Plasma chrome. Keep
  deliberate pixel dimensions when they represent measured RichText geometry,
  font pixels, device-pixel borders, or an existing protocol.
- Panel width and height mean different things by form factor. Verify horizontal
  and vertical behavior; `plasmawindowed` only proves application form.
- Hidden anchored siblings and lazy representation items may have zero size or
  not exist. Do not use them as unconditional geometry authorities.

PlasmaTop publishes usable panel geometry to `<runtime>/state/geom`. Changes to
measurement, font sizing, wrapping, or preferred dimensions must preserve that
daemon auto-fit contract.
