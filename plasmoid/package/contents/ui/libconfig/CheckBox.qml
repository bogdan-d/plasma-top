// Version 5

import QtQuick
import QtQuick.Controls as QQC2

QQC2.CheckBox {
	id: configCheckBox

	property var configObject: plasmoid.configuration
	property string configKey: ''
	checked: configObject[configKey]
	onClicked: configObject[configKey] = !configObject[configKey]
}
