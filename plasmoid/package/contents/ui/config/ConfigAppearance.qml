import QtQuick
import QtQuick.Layouts

import org.kde.kirigami as Kirigami

import "../libconfig" as LibConfig

LibConfig.FormKCM {

	// ── Font: shared by both surfaces ─────────────────────────
	LibConfig.Heading {
		text: i18n("Font")
	}
	LibConfig.FontFamily {
		Kirigami.FormData.label: i18n("Font Family:")
		configKey: 'fontFamily'
		monospaceOnly: true
	}
	LibConfig.TextFormat {
		boldConfigKey: 'bold'
		italicConfigKey: 'italic'
		underlineConfigKey: 'underline'
		alignConfigKey: 'textAlign'
		vertAlignConfigKey: 'vertAlign'
	}

	// ── Panel ─────────────────────────────────────────────────
	LibConfig.Heading {
		text: i18n("Panel")
	}
	LibConfig.SpinBox {
		Kirigami.FormData.label: i18n("Font Size:")
		configKey: 'fontSize'
		suffix: i18n("px")
	}
	LibConfig.SpinBox {
		Kirigami.FormData.label: i18n("Line Height:")
		configKey: 'panelLineHeight'
		decimals: 2
		minimumValue: 0.5
		maximumValue: 3.0
		stepSize: 5
	}

	// ── Tooltip ───────────────────────────────────────────────
	LibConfig.Heading {
		text: i18n("Tooltip")
	}
	RowLayout {
		Kirigami.FormData.label: i18n("Font Size:")
		spacing: 0
		LibConfig.CheckBox {
			configKey: 'useTooltipFontSize'
		}
		LibConfig.SpinBox {
			configKey: 'tooltipFontSize'
			suffix: i18n("px")
		}
	}
	LibConfig.SpinBox {
		Kirigami.FormData.label: i18n("Line Height:")
		configKey: 'tooltipLineHeight'
		decimals: 2
		minimumValue: 0.5
		maximumValue: 3.0
		stepSize: 5
	}

}
