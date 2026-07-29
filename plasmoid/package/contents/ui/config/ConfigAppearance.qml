import QtQuick
import QtQuick.Layouts

import org.kde.kirigami as Kirigami
import org.kde.plasma.core as PlasmaCore

import "../libconfig" as LibConfig

LibConfig.FormKCM {
	id: root

	// Plasma's configuration loader initializes and monitors cfg_* properties. Plasma 6.7 still warns for generated cfg_*Default and panel-only cfg_length/cfg_expanding properties; those are upstream KDE bug 484541, not settings owned by this page.
	property int cfg_fontSize
	property string cfg_fontFamily
	property bool cfg_useTooltipFontSize
	property int cfg_tooltipFontSize
	property real cfg_panelLineHeight
	property real cfg_tooltipLineHeight
	property bool cfg_useTooltipWidth
	property int cfg_tooltipWidth
	property bool cfg_bold
	property bool cfg_italic
	property bool cfg_underline
	property int cfg_textAlign
	property int cfg_vertAlign
	property string cfg_textColor
	property string cfg_outlineColor
	property bool cfg_showOutline
	property bool cfg_showBackground
	property string cfg_desktopTextColor
	property bool cfg_desktopOutline
	property string cfg_desktopOutlineColor
	property string cfg_clickCommand
	property string cfg_mousewheelUpCommand
	property string cfg_mousewheelDownCommand
	property bool cfg_useFixedWidth
	property int cfg_fixedWidth
	property bool cfg_useFixedHeight
	property int cfg_fixedHeight
	property bool cfg_replaceAllNewlines

	// The Desktop section below only means anything for a widget placed on the
	// desktop (Planar); in a panel the panel owns the background and the widget
	// never shows its full representation there, so hide it.
	readonly property bool onDesktop: plasmoid.formFactor === PlasmaCore.Types.Planar


	// ── Font: shared by both surfaces ─────────────────────────
	LibConfig.Heading {
		text: i18n("Font")
	}
	LibConfig.FontFamily {
		Kirigami.FormData.label: i18n("Font Family:")
		configObject: root
		configKey: 'cfg_fontFamily'
		monospaceOnly: true
	}
	LibConfig.TextFormat {
		configObject: root
		boldConfigKey: 'cfg_bold'
		italicConfigKey: 'cfg_italic'
		underlineConfigKey: 'cfg_underline'
		alignConfigKey: 'cfg_textAlign'
		vertAlignConfigKey: 'cfg_vertAlign'
	}

	// ── Panel ─────────────────────────────────────────────────
	LibConfig.Heading {
		text: i18n("Panel")
	}
	LibConfig.SpinBox {
		Kirigami.FormData.label: i18n("Font Size:")
		configObject: root
		configKey: 'cfg_fontSize'
		suffix: i18n("px")
	}
	LibConfig.SpinBox {
		Kirigami.FormData.label: i18n("Line Height:")
		configObject: root
		configKey: 'cfg_panelLineHeight'
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
			configObject: root
			configKey: 'cfg_useTooltipFontSize'
		}
		LibConfig.SpinBox {
			configObject: root
			configKey: 'cfg_tooltipFontSize'
			suffix: i18n("px")
		}
	}
	LibConfig.SpinBox {
		Kirigami.FormData.label: i18n("Line Height:")
		configObject: root
		configKey: 'cfg_tooltipLineHeight'
		decimals: 2
		minimumValue: 0.5
		maximumValue: 3.0
		stepSize: 5
	}

	// ── Desktop ───────────────────────────────────────────────
	// Two looks for the widget on the desktop (ignored in a panel, which has its
	// own background). With a background it's a solid panel; without, it's
	// transparent on the wallpaper and the text is forced to one flat color for
	// legibility — white or black — with threshold colors kept.
	LibConfig.Heading {
		visible: root.onDesktop
		text: i18n("Desktop")
	}
	LibConfig.CheckBox {
		visible: root.onDesktop
		Kirigami.FormData.label: i18n("Background:")
		configObject: root
		configKey: 'cfg_showBackground'
		text: i18n("Solid panel behind the widget")
	}
	LibConfig.ColorField {
		visible: root.onDesktop
		Kirigami.FormData.label: i18n("Text color (no background):")
		configObject: root
		configKey: 'cfg_desktopTextColor'
		defaultColor: '#ffffff'
		showAlphaChannel: false
	}
	LibConfig.CheckBox {
		visible: root.onDesktop
		Kirigami.FormData.label: i18n("Outline:")
		configObject: root
		configKey: 'cfg_desktopOutline'
		text: i18n("Halo around the text")
	}
	LibConfig.ColorField {
		visible: root.onDesktop
		Kirigami.FormData.label: i18n("Outline color:")
		configObject: root
		configKey: 'cfg_desktopOutlineColor'
		defaultColor: '#000000'
		showAlphaChannel: false
	}

}
