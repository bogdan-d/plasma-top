import QtQuick

import org.kde.plasma.configuration

// PlasmaTop wires the commands and mouse actions to the daemon (see main.xml
// defaults), so the upstream "Command" and "Actions" pages are gone — only the
// visual settings remain.
ConfigModel {
	ConfigCategory {
		name: i18n("Appearance")
		icon: "preferences-desktop-color"
		source: "config/ConfigAppearance.qml"
	}
}
