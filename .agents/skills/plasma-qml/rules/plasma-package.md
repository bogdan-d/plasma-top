# Plasma 6 package and imports

Expected package contract:

- `plasmoid/package/metadata.json`
- `KPackageStructure: "Plasma/Applet"`
- unique `KPlugin.Id`
- `X-Plasma-API-Minimum-Version: "6.0"`
- `contents/ui/main.qml` rooted at `PlasmoidItem`

Use Qt 6 and Plasma 6 imports without version numbers unless a verified module requires one. Preserve aliases that distinguish Qt, Kirigami, Plasma Core, Plasma Components, and Plasma 5 compatibility APIs.

`org.kde.plasma.plasma5support` is intentionally used for the executable `DataSource`; its name does not make it accidental Plasma 5 code. Verify a replacement against the current Plasma runtime before removing it.

Do not add standalone Qt application structure, CMake QML modules, `qmldir`, or C++ registration unless the package architecture actually changes. Keep package metadata and install paths aligned with `install.sh` and package tests.
