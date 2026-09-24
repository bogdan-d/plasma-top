pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

import org.kde.kirigami as Kirigami
import org.kde.plasma.plasma5support as Plasma5Support

import "../libconfig" as LibConfig

LibConfig.FormKCM {
	id: root

	property bool loaded: false
	property bool busy: false
	property bool saved: false
	property int execNonce: 0
	property string commandInFlight: ""
	property string actionInFlight: ""
	property string pageFlags: "00000"
	property string notificationFlags: "00000000000"
	readonly property var pageLabels: [
		i18n("Graphs"), i18n("Top processes"), i18n("CPU cores"),
		i18n("Connections"), i18n("System information")
	]
	readonly property var notificationLabels: [
		i18n("Disk usage"), i18n("SMART health"), i18n("CPU temperature"),
		i18n("Disk temperature"), i18n("NVIDIA temperature"),
		i18n("AMDGPU temperature"), i18n("System battery"),
		i18n("Mouse battery"), i18n("Keyboard battery"),
		i18n("Load average"), i18n("Server check")
	]
	property string statusText: i18n("Loading daemon settings…")

	function replaceFlag(flags, index, enabled) {
		return flags.slice(0, index) + (enabled ? "1" : "0") + flags.slice(index + 1)
	}

	function execute(action, args) {
		if (busy)
			return
		busy = true
		actionInFlight = action
		commandInFlight = plasmoid.configuration.backendCommand + " config " + args + " # " + (++execNonce)
		executable.exec(commandInFlight)
	}

	function load() {
		statusText = i18n("Loading daemon settings…")
		execute("show", "show")
	}

	function save() {
		var poll = Number(pollField.text.replace(",", "."))
		var history = Number(historyField.text.replace(",", "."))
		if (!isFinite(poll) || !isFinite(history) || poll < 0.1 || history < 0.1) {
			statusText = i18n("Intervals must be numbers of at least 0.1 seconds.")
			return
		}
		statusText = i18n("Saving daemon settings…")
		execute("apply", "apply " + poll + " " + history + " " + pageFlags + " " + notificationFlags)
	}

	Plasma5Support.DataSource {
		id: executable
		engine: "executable"
		connectedSources: []
		onNewData: (sourceName, data) => {
			disconnectSource(sourceName)
			if (sourceName !== root.commandInFlight)
				return
			var action = root.actionInFlight
			root.commandInFlight = ""
			root.busy = false
			if (data["exit code"] !== 0) {
				root.statusText = i18n("Daemon settings failed: %1", data["stderr"].trim())
				return
			}
			if (action === "apply") {
				root.saved = true
				root.load()
				return
			}
			var fields = data["stdout"].trim().split("\t")
			if (fields.length !== 4 || !/^[01]{5}$/.test(fields[2]) || !/^[01]{11}$/.test(fields[3])) {
				root.statusText = i18n("The daemon returned an invalid settings response.")
				return
			}
			pollField.text = fields[0]
			historyField.text = fields[1]
			root.pageFlags = fields[2]
			root.notificationFlags = fields[3]
			root.loaded = true
			root.statusText = root.saved
				? i18n("Saved. The daemon reloads the file automatically.")
				: i18n("Settings loaded from the user config file.")
			root.saved = false
		}
		function exec(command) {
			connectSource(command)
		}
	}

	LibConfig.Heading {
		text: i18n("Display")
	}
	QQC2.Label {
		text: i18n("Daemon settings are shared by all PlasmaTop widgets.")
		wrapMode: Text.WordWrap
	}
	QQC2.TextField {
		id: pollField
		Kirigami.FormData.label: i18n("Refresh interval:")
		enabled: root.loaded && !root.busy
		inputMethodHints: Qt.ImhFormattedNumbersOnly
		placeholderText: i18n("Seconds")
		Accessible.description: i18n("How often the daemon publishes panel and visible tooltip readings, in seconds.")
	}
	QQC2.TextField {
		id: historyField
		Kirigami.FormData.label: i18n("History interval:")
		enabled: root.loaded && !root.busy
		inputMethodHints: Qt.ImhFormattedNumbersOnly
		placeholderText: i18n("Seconds")
		Accessible.description: i18n("How often the daemon samples graph and sparkline history, in seconds.")
	}

	LibConfig.Heading {
		text: i18n("Tooltip pages")
	}
	ColumnLayout {
		Kirigami.FormData.label: i18n("Enabled pages:")
		Repeater {
			model: root.pageLabels.length
			delegate: QQC2.CheckBox {
				required property int index
				text: root.pageLabels[index]
				enabled: root.loaded && !root.busy
				checked: root.pageFlags.charAt(index) === "1"
				onClicked: root.pageFlags = root.replaceFlag(root.pageFlags, index, checked)
			}
		}
	}
	QQC2.Label {
		text: i18n("Existing page order is kept. Newly enabled pages are added at the end.")
		wrapMode: Text.WordWrap
	}

	LibConfig.Heading {
		text: i18n("Notifications")
	}
	ColumnLayout {
		Kirigami.FormData.label: i18n("Enabled alerts:")
		Repeater {
			model: root.notificationLabels.length
			delegate: QQC2.CheckBox {
				required property int index
				text: root.notificationLabels[index]
				enabled: root.loaded && !root.busy
				checked: root.notificationFlags.charAt(index) === "1"
				onClicked: root.notificationFlags = root.replaceFlag(root.notificationFlags, index, checked)
			}
		}
	}

	RowLayout {
		QQC2.Button {
			text: i18n("Save daemon settings")
			enabled: root.loaded && !root.busy
			onClicked: root.save()
		}
		QQC2.Button {
			text: i18n("Reload")
			enabled: !root.busy
			onClicked: root.load()
		}
	}
	QQC2.Label {
		text: root.statusText
		textFormat: Text.PlainText
		wrapMode: Text.WordWrap
		Accessible.role: Accessible.StaticText
	}
	QQC2.Label {
		text: i18n("Other daemon settings remain available in config.toml. Machine-specific overrides may take precedence.")
		wrapMode: Text.WordWrap
	}

	Component.onCompleted: load()
}
