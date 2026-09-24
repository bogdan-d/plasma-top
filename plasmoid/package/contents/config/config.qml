import QtQuick

import org.kde.plasma.configuration

// PlasmaTop wires commands and mouse actions to the daemon, so the upstream Command and Actions pages are replaced by app-owned settings.
ConfigModel {
	ConfigCategory {
		name: i18n("Appearance")
		icon: "preferences-desktop-color"
		source: "config/ConfigAppearance.qml"
	}
	ConfigCategory {
		name: i18n("Daemon")
		icon: "preferences-system"
		source: "config/ConfigDaemon.qml"
	}
	ConfigCategory {
		name: i18n("Main tooltip")
		icon: "view-list-details"
		source: "config/ConfigTooltip.qml"
	}
	ConfigCategory {
		name: i18n("Graphs")
		icon: "view-statistics"
		source: "config/ConfigGraphs.qml"
	}
}
