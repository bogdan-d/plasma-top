// Version 4

import QtQuick

import "." as LibConfig

LibConfig.ComboBox {
	id: configFontFamily

	property bool monospaceOnly: false

	populated: false

	// Qt doesn't expose QFontDatabase::isFixedPitch() to QML. Filtering by
	// "mono" in the family name covers all monospace/coding fonts in
	// practice (Nerd Fonts, DejaVu Sans Mono, Liberation Mono, etc.).
	function isMonospace(family) {
		return family.toLowerCase().indexOf("mono") >= 0
	}

	// Based on: org.kde.plasma.digitalclock
	onPopulate: {
		var arr = [] // Use temp array to avoid constant binding stuff
		arr.push({ text: i18nc("Use default font", "Default"), value: "" })

		var fonts = Qt.fontFamilies()
		for (var i = 0; i < fonts.length; i++) {
			if (monospaceOnly && !isMonospace(fonts[i])) {
				continue
			}
			arr.push({ text: fonts[i], value: fonts[i] })
		}
		model = arr
		populated = true
	}
}
