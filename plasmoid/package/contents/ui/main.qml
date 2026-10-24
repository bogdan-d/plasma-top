import QtQuick
import QtQuick.Layouts

import org.kde.kirigami as Kirigami
import org.kde.plasma.plasmoid
import org.kde.plasma.core as PlasmaCore
import org.kde.plasma.components as PlasmaComponent
import org.kde.plasma.plasma5support as Plasma5Support
import org.kde.plasma.workspace.dbus as DBus

PlasmoidItem {
	id: widget

	// https://github.com/KDE/plasma-workspace/blob/master/dataengines/executable/executable.h
	// https://github.com/KDE/plasma-workspace/blob/master/dataengines/executable/executable.cpp
	// https://github.com/KDE/plasma-framework/blob/master/src/declarativeimports/core/datasource.h
	// https://github.com/KDE/plasma-framework/blob/master/src/declarativeimports/core/datasource.cpp
	// https://github.com/KDE/plasma-framework/blob/master/src/plasma/scripting/dataenginescript.cpp
	// Runs the configured middle-click command; the daemon itself is reached over D-Bus.
	Plasma5Support.DataSource {
		id: executable
		engine: "executable"
		connectedSources: []
		onNewData: (sourceName, data) => disconnectSource(sourceName)
		function exec(cmd) {
			if (cmd) {
				connectSource(cmd)
			}
		}
	}

	// The executable DataSource keys sources by command string and drops connectSource() for one still connected, so a trailing shell comment (# n) makes each click a distinct source.
	property int execNonce: 0
	function execOnce(cmd) {
		if (cmd)
			executable.exec(cmd + " # " + (++execNonce))
	}

	function performClick() {
		execOnce(plasmoid.configuration.clickCommand)
	}

	// One wheel gesture, one page. A movement of the wheel emits a burst of events
	// (this device sends several per flick); we turn a page on the FIRST event of
	// a gesture, then stay quiet for the rest of it. A gesture is "in progress"
	// until the events stop for wheelGestureGapMs — every event pushes that idle
	// deadline out — so a long/continuous scroll is still ONE page, and paging
	// again needs a fresh gesture (a brief pause, then scroll). This is the leading
	// edge + idle-reset, not the old fixed-window debounce, which re-fired every
	// window and paged continuously while you kept scrolling. Tune wheelGestureGapMs:
	// longer groups more into one gesture (and lengthens the pause needed to page
	// again); shorter splits a bursty flick sooner. Shared by panel + pinned popup.
	readonly property int wheelGestureGapMs: 200
	property bool wheelInGesture: false
	function wheelStep(delta) {
		if (delta === 0)
			return
		if (!wheelInGesture) {
			wheelInGesture = true
			// Wheel up turns back a page. The daemon publishes the new page's tooltip on its own.
			callDaemon("StepPage", "(i)", [delta > 0 ? -1 : 1], null)
		}
		wheelGestureEnd.restart()   // any event extends the current gesture
	}
	// The gesture ends once the wheel has been idle this long; the next event then
	// starts a new gesture (and turns the next page).
	Timer {
		id: wheelGestureEnd
		interval: widget.wheelGestureGapMs
		onTriggered: widget.wheelInGesture = false
	}

	// QQuickPixmap creates the engine's image reader thread on the first load. If that first load is a tooltip graph <img>, it happens on the scene-graph render thread and the reader cannot be parented to the engine, so a 1x1 PNG forces it onto the GUI thread first.
	Image {
		visible: false
		source: "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAAAAAA6fptVAAAACklEQVR4nGNgAAAAAgABSK+kcQAAAABJRU5ErkJggg=="
	}

	Item {
		id: config
		readonly property bool clickEnabled: !!plasmoid.configuration.clickCommand
		readonly property color textColor: plasmoid.configuration.textColor || Kirigami.Theme.textColor
		readonly property color outlineColor: plasmoid.configuration.outlineColor || Kirigami.Theme.backgroundColor
		readonly property bool showOutline: plasmoid.configuration.showOutline
	}

	// The daemon's versioned applet interface (see docs/adr/0003-applet-protocol-over-session-dbus.md).
	readonly property var daemonTarget: ({
		service: "com.github.bogdan_d.PlasmaTop",
		path: "/com/github/bogdan_d/PlasmaTop",
		iface: "com.github.bogdan_d.PlasmaTop1",
	})
	// Bumped on every daemon registration; a reply from an earlier daemon is ignored.
	property int serviceLifetime: 0
	property bool ready: false

	// Calls the daemon asynchronously with an explicit signature, so the module never introspects first. `done(ok)` runs only for replies from the current daemon.
	function callDaemon(member, signature, args, done) {
		if (!daemonWatcher.registered) {
			if (done)
				done(false)
			return
		}
		var lifetime = serviceLifetime
		var settle = ok => reply => {
			if (lifetime !== widget.serviceLifetime)
				return
			if (!ok)
				console.warn("[plasma-top]", member, "failed:", reply.error.name, reply.error.message)
			if (done)
				done(ok)
		}
		DBus.SessionBus.asyncCall(Object.assign({ member: member, signature: signature, arguments: args }, daemonTarget), settle(true), settle(false))
	}

	// A daemon that (re)registers starts empty: fetch its state, resend geometry, and present again if presented. Registration stays observable because the daemon never lets another process replace its name.
	function daemonRegistered() {
		serviceLifetime += 1
		daemon.updateAll()
		sendGeometry()
		lease.restart()
	}

	DBus.DBusServiceWatcher {
		id: daemonWatcher
		busType: DBus.BusType.Session
		watchedService: widget.daemonTarget.service
		onRegisteredChanged: {
			if (registered && widget.ready)
				widget.daemonRegistered()
		}
	}

	// Received values live here; displayed text is separate, so empty values and daemon absence never blank the widget.
	DBus.Properties {
		id: daemon
		busType: DBus.BusType.Session
		service: widget.daemonTarget.service
		path: widget.daemonTarget.path
		iface: widget.daemonTarget.iface
		onPropertiesChanged: (interfaceName, changed, invalidated) => {
			if ("PanelHtml" in changed)
				widget.showPanel(daemon.properties.PanelHtml)
			if ("TooltipHtml" in changed && widget.presented)
				widget.showTooltip(daemon.properties.TooltipHtml)
		}
	}

	// Last measured panel geometry as SetGeometry arguments; null until every measurement is positive.
	property var geometry: null
	function sendGeometry() {
		if (geometry)
			callDaemon("SetGeometry", "(ddbd)", geometry, null)
	}

	// https://stackoverflow.com/questions/4842424/list-of-ansi-color-escape-sequences
	property var ansiColors: ({
		30: '#000000', // Black
		31: '#aa0000', // Red
		32: '#00aa00', // Green
		33: '#aa6500', // Yellow
		34: '#0000aa', // Blue
		35: '#aa00aa', // Magenta
		36: '#00aaaa', // Cyan
		37: '#aaaaaa', // White
		90: '#656565', // Bright Black
		91: '#ff6565', // Bright Red
		92: '#65ff65', // Bright Green
		93: '#ffff65', // Bright Yellow
		94: '#6565ff', // Bright Blue
		95: '#ff65ff', // Bright Magenta
		96: '#65ffff', // Bright Cyan
		97: '#ffffff', // Bright White
	})
	function resetState(state) {
		var out = state.closeTags.join(' ')
		state.bold = false
		state.closeTags = []
		return out
	}
	function parseAnsiCode(n, i, tokens, state) {
		if (n == 0) { // Reset
			return resetState(state)
		} else if (n == 1) {
			state.closeTags.push('</b>')
			state.bold = true
			return '<b>'
		} else if (30 <= n && n <= 37 || 90 <= n && n <= 97) {
			if (state.bold && 30 <= n && n <= 37) {
				// Bold also intensifies the colors to "Bright".
				// 30 => 90
				n += 60
			}
			var hexColor = ansiColors[n]
			state.closeTags.push('</font>')
			return '<font color="' + hexColor + '">'
		} else {
			return ''
		}
	}
	// https://stackoverflow.com/questions/4745317/converting-integers-to-hex-string-in-javascript
	function formatHexInt(n) {
		var num = Number(n)
		if (isNaN(num)) {
			return "00"
		}
		num = Math.max(0, Math.min(num, 255))
		var str = num.toString(16)
		return str.length == 1 ? '0' + str : str
	}
	function rgbToHex(r, g, b) {
		return '#' + formatHexInt(r) + formatHexInt(g) + formatHexInt(b)
	}
	function parseColorMode(i, tokens) {
		var colorMode = parseInt(tokens[++i], 10)
		if (colorMode == 2) { // RGB
			var r = parseInt(tokens[++i], 10)
			var g = parseInt(tokens[++i], 10)
			var b = parseInt(tokens[++i], 10)
			return rgbToHex(r, g, b)
		} else if (colorMode == 5) { // Preset of 256 colors
			// Logic taken from Konsole
			// https://invent.kde.org/utilities/konsole/-/blob/master/src/autotests/CharacterColorTest.cpp#L159
			var n = parseInt(tokens[++i], 10)
			if (0 <= n && n <= 7) { // Normal
				var u = n + 30
				return ansiColors[u]
			} else if (8 <= n && n <= 15) { // Bright
				var u = n - 8 + 90
				return ansiColors[u]
			} else if (16 <= n && n <= 231) { // 212
				var u = n - 16
				var r = Math.floor(((u / 36) % 6) != 0 ? (40 * ((u / 36) % 6) + 55) : 0)
				var g = Math.floor(((u / 6) % 6) != 0 ? (40 * ((u / 6) % 6) + 55) : 0)
				var b = Math.floor(((u / 1) % 6) != 0 ? (40 * ((u / 1) % 6) + 55) : 0)
				return rgbToHex(r, g, b)
			} else if (232 <= n && n <= 255) {
				var gray = Math.floor((n - 232) * 10 + 8)
				return rgbToHex(gray, gray, gray)
			}
		}
		return null
	}
	function parseAnsiEscape(codes, state) {
		var tokens = codes.split(';')
		var out = ''
		for (var i = 0; i < tokens.length; i++) {
			tokens[i] = parseInt(tokens[i], 10)
		}
		for (var i = 0; i < tokens.length; i++) {
			var token = tokens[i]
			if (token == 38) { // Set FG
				var hexColor = parseColorMode(i, tokens)
				if (hexColor) {
					state.closeTags.push('</font>')
					out += '<font color="' + hexColor + '">'
				}
			} else if (token == 48) { // Set BG
				var hexColor = parseColorMode(i, tokens)
				// Ignore
			} else {
				out += parseAnsiCode(token, i, tokens, state)
			}
		}
		return out
	}

	property string outputText: ''
	property string tooltipText: ''
	// Whether the panel widget is hovered (the hover tooltip is shown), set from the compact representation's HoverHandler, which fires reliably on the first hover.
	property bool tooltipHovered: false
	readonly property bool planar: plasmoid.formFactor === PlasmaCore.Types.Planar
	readonly property bool presented: tooltipHovered || expanded || planar

	PresentationLease {
		id: lease
		presented: widget.presented
		instanceId: Number(plasmoid.id)
		target: widget.daemonTarget
		call: widget.callDaemon
	}

	// Desktop "no background" look: with the widget transparent on the wallpaper,
	// the daemon's own base colors (grey labels, cyan titles, dark "good"/"active"
	// values) lose contrast. Force those to one flat color — white or black, the
	// user's pick — by appending a CSS rule that overrides exactly those classes.
	// It lands last inside the daemon's <style>, so at equal specificity it wins.
	// Threshold classes (.warn/.crit/.deactive) and the colored bars are left out,
	// so alerts keep their meaning. Plain values carry no color class and inherit
	// the Text color instead (set on pinText), so this rule need not name them.
	function desktopRecolor(html, color) {
		// The inactive pager dots (pages you haven't scrolled to) stay dimmer than
		// the active one: the chosen text color blended halfway to mid-grey. Kept a
		// solid 6-digit color on purpose — Qt's RichText CSS may not parse the
		// 8-digit alpha form — and it works for any picked color, not a fixed grey.
		var c = Qt.color(color)
		var dim = Qt.rgba((c.r + 0.5) / 2, (c.g + 0.5) / 2, (c.b + 0.5) / 2, 1).toString()
		// Each selector must be at least as specific as the base rule it overrides,
		// or the base wins despite ours coming later: .active is set by the base as
		// ".tooltip .active" (two classes), so a bare ".active" here would lose and
		// e.g. the SMART "OK" value would keep its default color.
		var rule = ".tooltip .title,.tooltip .label,.tooltip .aux,.good,.tooltip .active,"
			+ ".tooltip .pager .on,.tooltip .page{color:" + color + ";}"
			+ ".tooltip .pager .off{color:" + dim + ";}"
			// The rule under each section title is a thin block tinted with
			// background-color, not color, so recolor it separately to match.
			+ ".tooltip .title-rule{background-color:" + color + ";}"
		return html.replace("</style>", rule + "</style>")
	}

	function formatOutputText(stdout) {
		var formattedText = stdout

		// Newlines
		if (plasmoid.configuration.replaceAllNewlines) {
			formattedText = formattedText.replace(/\n/g, ' ').trim()
		} else if (formattedText.length >= 1 && formattedText[formattedText.length-1] == '\n') {
			formattedText = formattedText.substr(0, formattedText.length-1)
		}

		// Terminal Colors (Issue #7)
		var state = {
			html: false,
			bold: false,
			closeTags: [],
		}
		formattedText = formattedText.replace(/\033\[(\d+(;\d+)*)?m/g, function(match, p1, p2){
			state.html = true
			if (typeof p1 === 'string') {
				return parseAnsiEscape(p1, state)
			} else { // \033[m is Reset
				return parseAnsiEscape('0', state)
			}
		})
		formattedText += resetState(state)

		// Output is always rendered as rich text (see textFormat: Text.RichText
		// below), so newlines must always be encoded, and bold must be applied
		// as an explicit <b> tag: font.weight on the Text item does not reliably
		// propagate into nested <font> spans once the content contains HTML
		// (the color tags above) — same mechanism already used for the ANSI
		// bold escape (\033[1m) in parseAnsiCode().
		formattedText = formattedText.replace(/\n/g, '<br>')
		if (plasmoid.configuration.bold) {
			formattedText = '<b>' + formattedText + '</b>'
		}
		return formattedText
	}

	function showPanel(html) {
		if (html)
			widget.outputText = formatOutputText(String(html))
	}

	// Only a presented widget re-lays out tooltip RichText; a hidden one catches up when it is presented.
	function showTooltip(html) {
		if (html)
			widget.tooltipText = formatOutputText(String(html))
	}

	onPresentedChanged: {
		if (widget.presented)
			widget.showTooltip(daemon.properties.TooltipHtml)
	}

	Plasmoid.onActivated: widget.expanded = !widget.expanded

	Component.onCompleted: {
		widget.ready = true
		if (daemonWatcher.registered)
			widget.daemonRegistered()
	}

	// The hover tooltip: our HTML content fed to the shell's own tooltip area
	// (CompactApplet wraps the compact representation in a ToolTipArea bound to
	// Plasmoid.toolTipItem). This replaces the default name/description tooltip
	// and, being active only while !expanded, steps aside when the popup is
	// pinned. Content is fetched lazily — only while the panel is hovered
	// (panelHover), never on the panel's main tick.
	toolTipItem: Item {
		id: tooltipRoot
		// Width from the hidden NoWrap twin, not tooltipText.contentWidth: block
		// <div>s fill the given width, so contentWidth latches to the window and
		// never shrinks. Height shrinks fine, so it comes from contentHeight. The
		// padding is added back so the content's right/bottom edge isn't clipped;
		// pinning Layout min=max=implicit forces Plasma's shared ToolTipDialog to
		// follow the content size down as well as up.
		implicitWidth: plasmoid.configuration.useTooltipWidth ? plasmoid.configuration.tooltipWidth : (ttMeasure.contentWidth + ttText.leftPadding + ttText.rightPadding)
		implicitHeight: ttText.contentHeight + ttText.topPadding + ttText.bottomPadding
		Layout.minimumWidth: implicitWidth
		Layout.maximumWidth: implicitWidth
		Layout.preferredWidth: implicitWidth
		Layout.minimumHeight: implicitHeight
		Layout.maximumHeight: implicitHeight
		Layout.preferredHeight: implicitHeight

		Text {
			id: ttMeasure
			visible: false
			width: 1
			text: widget.tooltipText
			textFormat: Text.RichText
			wrapMode: Text.NoWrap
			elide: Text.ElideNone
			font.pointSize: plasmoid.configuration.useTooltipFontSize ? plasmoid.configuration.tooltipFontSize : plasmoid.configuration.fontSize
			font.family: plasmoid.configuration.fontFamily || Kirigami.Theme.defaultFont.family
			font.weight: plasmoid.configuration.bold ? Font.Bold : Font.Normal
			font.italic: plasmoid.configuration.italic
			font.underline: plasmoid.configuration.underline
			fontSizeMode: Text.FixedSize
		}

		Text {
			id: ttText
			anchors.fill: parent
			text: widget.tooltipText
			textFormat: Text.RichText
			color: config.textColor
			horizontalAlignment: Text.AlignLeft
			wrapMode: Text.NoWrap
			elide: Text.ElideNone
			font.pointSize: plasmoid.configuration.useTooltipFontSize ? plasmoid.configuration.tooltipFontSize : plasmoid.configuration.fontSize
			font.family: plasmoid.configuration.fontFamily || Kirigami.Theme.defaultFont.family
			font.weight: plasmoid.configuration.bold ? Font.Bold : Font.Normal
			font.italic: plasmoid.configuration.italic
			font.underline: plasmoid.configuration.underline
			fontSizeMode: Text.FixedSize
			leftPadding: 8
			rightPadding: 8
			topPadding: 8
			bottomPadding: 8
			lineHeight: plasmoid.configuration.tooltipLineHeight
			lineHeightMode: Text.ProportionalHeight
			linkColor: Kirigami.Theme.linkColor
			onLinkActivated: Qt.openUrlExternally(link)
		}
	}

	// Plasma's own widget background, on when showBackground is set (the desktop
	// "with background" look); off leaves the widget transparent on the wallpaper
	// (the "conky" look, where legibility comes from the forced text color in
	// pinText). In a panel the panel supplies the background regardless.
	Plasmoid.backgroundHints: plasmoid.configuration.showBackground ? PlasmaCore.Types.DefaultBackground : PlasmaCore.Types.NoBackground

	// Left-click toggles the pinned popup, and a left-click inside it dismisses it.
	// Keep the popup open when it loses focus so it can stay visible while you work in another window.
	hideOnWindowDeactivate: false

	compactRepresentation: Item {
		id: panelItem

		readonly property bool isHorizontal: plasmoid.formFactor == PlasmaCore.Types.Horizontal
		readonly property bool isVertical: plasmoid.formFactor == PlasmaCore.Types.Vertical
		readonly property bool isInPanel: isHorizontal || isVertical
		readonly property bool isOnDesktop: !isInPanel

		// output.width can change before isVertical settles, so the orientation change publishes again with the settled flag.
		onIsVerticalChanged: output.publishGeometry()

		readonly property int itemWidth: {
			if (isOnDesktop) {
				return Math.ceil(output.contentWidth)
			} else if (isHorizontal && plasmoid.configuration.useFixedWidth) {
				return plasmoid.configuration.fixedWidth
			} else { // isHorizontal || isVertical
				return Math.ceil(output.implicitWidth)
			}
		}
		Layout.minimumWidth: isHorizontal ? itemWidth : -1
		Layout.fillWidth: isVertical
		Layout.preferredWidth: itemWidth // Panel widget default
		// width: itemWidth // Desktop widget default
		// onItemWidthChanged: console.log('itemWidth', itemWidth, 'implicitWidth', output.implicitWidth, 'contentWidth', output.contentWidth)

		readonly property int itemHeight: {
			if (isOnDesktop) {
				return Math.ceil(output.contentHeight)
			} else if (isVertical && plasmoid.configuration.useFixedHeight) {
				return plasmoid.configuration.fixedHeight
			} else { // isHorizontal || isVertical
				return Math.ceil(output.implicitHeight)
			}
		}
		Layout.minimumHeight: isVertical ? itemHeight : -1
		Layout.fillHeight: isHorizontal
		Layout.preferredHeight: itemHeight // Panel widget default
		// height: itemHeight // Desktop widget default
		// onItemHeightChanged: console.log('itemHeight', itemHeight, 'implicitHeight', output.implicitHeight, 'contentHeight', output.contentHeight)


		// Drive the tooltip's lazy refresh off the panel-widget hover. This is
		// the reliable "the hover tooltip is (about to be) shown" signal: the
		// shell shows its ToolTipArea while the pointer is over the compact
		// representation, and HoverHandler.hovered flips on the FIRST hover too.
		// Relying on toolTipItem.onVisibleChanged alone missed that first show
		// (the item is created visible=true, so the dialog displaying it produced
		// no visible-change edge) — the tooltip stayed frozen on the first hover
		// until you left and re-entered.
		HoverHandler {
			id: panelHover
			onHoveredChanged: widget.tooltipHovered = hovered
		}

		// Note MouseArea is below the Text so
		// that we don't eat the link clicks.
		MouseArea {
			id: mouseArea
			anchors.fill: parent
			hoverEnabled: config.clickEnabled

			cursorShape: output.hoveredLink ? Qt.PointingHandCursor : Qt.ArrowCursor

			// Left-click toggles the persistent tooltip popup; middle-click runs the configured command.
			acceptedButtons: Qt.LeftButton | Qt.MiddleButton
			onClicked: (mouse) => {
				if (mouse.button === Qt.LeftButton)
					widget.expanded = !widget.expanded
				else
					widget.performClick()
			}

			// One page per notch — see widget.wheelStep.
			onWheel: (wheel) => {
				wheel.accepted = true
				widget.wheelStep(wheel.angleDelta.y || wheel.angleDelta.x)
			}
		}

		Text {
			id: output
			width: parent.width
			height: parent.height

			text: widget.outputText
			textFormat: Text.RichText

			// The panel's real geometry (the Text's usable width, one mono glyph's advance, orientation) lets the daemon auto-fit bars and width without guessing DPI or margins. TextMetrics measures the advance because the pixelSize resolved from font.pointSize isn't readable.
			TextMetrics {
				id: glyphMetrics
				font: output.font
				text: "██████████"   // 10 mono blocks
				onAdvanceWidthChanged: output.publishGeometry()
			}
			// Same advance but at the tooltip font (separate pointSize): the daemon
			// uses it to size the graphs-page PNGs to the tooltip text's real width,
			// without guessing DPI.
			TextMetrics {
				id: tooltipGlyphMetrics
				font.family: output.font.family
				font.pointSize: plasmoid.configuration.useTooltipFontSize ? plasmoid.configuration.tooltipFontSize : output.font.pointSize
				text: "██████████"   // 10 mono blocks
				onAdvanceWidthChanged: output.publishGeometry()
			}
			function publishGeometry() {
				if (output.width > 0 && glyphMetrics.advanceWidth > 0 && tooltipGlyphMetrics.advanceWidth > 0) {
					widget.geometry = [output.width, glyphMetrics.advanceWidth / 10, panelItem.isVertical, tooltipGlyphMetrics.advanceWidth / 10]
					widget.sendGeometry()
				}
			}
			onWidthChanged: publishGeometry()
			Component.onCompleted: publishGeometry()

			color: config.textColor
			style: config.showOutline ? Text.Outline : Text.Normal
			styleColor: config.outlineColor

			linkColor: Kirigami.Theme.linkColor
			onLinkActivated: Qt.openUrlExternally(link)

			font.pointSize: plasmoid.configuration.fontSize
			font.family: plasmoid.configuration.fontFamily || Kirigami.Theme.defaultFont.family
			font.weight: plasmoid.configuration.bold ? Font.Bold : Font.Normal
			font.italic: plasmoid.configuration.italic
			font.underline: plasmoid.configuration.underline
			fontSizeMode: Text.FixedSize
			horizontalAlignment: plasmoid.configuration.textAlign
			verticalAlignment: plasmoid.configuration.vertAlign
			lineHeight: plasmoid.configuration.panelLineHeight
			lineHeightMode: Text.ProportionalHeight

			property bool isFixedWidth: {
				if (plasmoid.formFactor == PlasmaCore.Types.Planar) { // Desktop Widget
					return true
				} else if (plasmoid.formFactor == PlasmaCore.Types.Horizontal) {
					return plasmoid.configuration.useFixedWidth
				} else if (plasmoid.formFactor == PlasmaCore.Types.Vertical) {
					return true
				} else {
					return false
				}
			}
			elide: Text.ElideRight
			wrapMode: isFixedWidth ? Text.Wrap : Text.NoWrap
		}

	}

	// The pinned popup shows the same tooltipText as the hover tooltip and stays open until left-clicked again.
	// Page it by scrolling the panel; scrolling the popup itself is intentionally inert.
	activationTogglesExpanded: false   // Plasmoid.onActivated toggles it for keyboard and accessibility activation.

	fullRepresentation: Item {
		id: pinItem
		// Size the popup to the tooltip content, same trick as the hover tooltip:
		// a hidden NoWrap twin gives the natural width (the visible RichText's
		// block <div>s otherwise fill and latch to the popup width). Plain
		// content-fit: paging is done on the PANEL (a stable surface), never on
		// the popup, so the popup never resizes out from under the cursor and can
		// simply follow the content of whatever page the panel selected.
		Layout.minimumWidth: pinMeasure.contentWidth + pinText.leftPadding + pinText.rightPadding
		Layout.maximumWidth: Layout.minimumWidth
		Layout.preferredWidth: Layout.minimumWidth
		Layout.minimumHeight: pinText.contentHeight + pinText.topPadding + pinText.bottomPadding
		Layout.maximumHeight: Layout.minimumHeight
		Layout.preferredHeight: Layout.minimumHeight

		// This same full representation serves two very different contexts: inline
		// on the desktop (the widget itself) and the pinned popup from a panel. The
		// desktop-only behaviors below (its own background, outlined text, wheel
		// paging) key off this — the popup keeps Plasma's dialog background and the
		// panel drives its paging.
		readonly property bool onDesktop: plasmoid.formFactor === PlasmaCore.Types.Planar

		MouseArea {
			anchors.fill: parent
			acceptedButtons: Qt.LeftButton | Qt.MiddleButton
			onClicked: (mouse) => {
				if (mouse.button === Qt.LeftButton)
					widget.expanded = false
				else
					widget.performClick()
			}
			// Wheel paging is enabled ONLY on the desktop, where this full
			// representation IS the widget and there's no panel to page from. In
			// the pinned popup it stays inert: scrolling would resize the popup out
			// from under the cursor on a shorter page (a stick, glitchy on resize),
			// so there you page from the panel and the popup follows. On the desktop
			// the widget is inline — a resize just regrows it in place, no popup to
			// lose focus.
			onWheel: (wheel) => {
				if (!pinItem.onDesktop)
					return
				wheel.accepted = true
				widget.wheelStep(wheel.angleDelta.y || wheel.angleDelta.x)
			}
		}

		Text {
			id: pinMeasure
			visible: false
			width: 1
			text: widget.tooltipText
			textFormat: Text.RichText
			wrapMode: Text.NoWrap
			elide: Text.ElideNone
			font.pointSize: plasmoid.configuration.useTooltipFontSize ? plasmoid.configuration.tooltipFontSize : plasmoid.configuration.fontSize
			font.family: plasmoid.configuration.fontFamily || Kirigami.Theme.defaultFont.family
			font.weight: plasmoid.configuration.bold ? Font.Bold : Font.Normal
			font.italic: plasmoid.configuration.italic
			font.underline: plasmoid.configuration.underline
			fontSizeMode: Text.FixedSize
		}

		Text {
			id: pinText
			anchors.fill: parent
			// In the transparent desktop mode, force the base text color (plain
			// values inherit this Text color) and recolor the classed base text via
			// the injected CSS rule; thresholds stay untouched. Elsewhere (panel
			// popup, or desktop with a background) the daemon's own colors stand.
			readonly property bool conkyMode: pinItem.onDesktop && !plasmoid.configuration.showBackground
			// Empty config means "default" (ColorField stores "" for its default),
			// so fall back to the kcfg defaults here.
			readonly property string txtColor: plasmoid.configuration.desktopTextColor || "#ffffff"
			readonly property string outlineColor: plasmoid.configuration.desktopOutlineColor || "#000000"
			text: conkyMode ? widget.desktopRecolor(widget.tooltipText, txtColor)
			                : widget.tooltipText
			textFormat: Text.RichText
			color: conkyMode ? txtColor : config.textColor
			// Optional halo in conky mode so the text reads on a busy wallpaper; it
			// rings every glyph, threshold colors included. Color is the user's pick
			// (default black, which sits well under white text).
			style: (conkyMode && plasmoid.configuration.desktopOutline) ? Text.Outline : Text.Normal
			styleColor: outlineColor
			horizontalAlignment: Text.AlignLeft
			wrapMode: Text.NoWrap
			elide: Text.ElideNone
			font.pointSize: plasmoid.configuration.useTooltipFontSize ? plasmoid.configuration.tooltipFontSize : plasmoid.configuration.fontSize
			font.family: plasmoid.configuration.fontFamily || Kirigami.Theme.defaultFont.family
			font.weight: plasmoid.configuration.bold ? Font.Bold : Font.Normal
			font.italic: plasmoid.configuration.italic
			font.underline: plasmoid.configuration.underline
			fontSizeMode: Text.FixedSize
			leftPadding: 8
			rightPadding: 8
			topPadding: 8
			bottomPadding: 8
			lineHeight: plasmoid.configuration.tooltipLineHeight
			lineHeightMode: Text.ProportionalHeight
			linkColor: Kirigami.Theme.linkColor
			onLinkActivated: Qt.openUrlExternally(link)
		}
	}
}
