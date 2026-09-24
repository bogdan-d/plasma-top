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
	property var order: []
	property var available: []
	readonly property var addable: available.filter(function(chart) { return order.indexOf(chart) === -1 })
	property string statusText: i18n("Loading Graphs settings…")

	function chartLabel(chart) {
		switch (chart) {
		case "cpu": return i18n("CPU usage")
		case "memory": return i18n("Memory usage")
		case "gpu": return i18n("GPU usage and codec")
		case "network": return i18n("Network download and upload")
		case "temperature": return i18n("CPU and GPU temperature")
		default: return i18n("Unknown chart: %1", chart)
		}
	}

	function shellQuote(value) {
		return "'" + value.split("'").join("'\"'\"'") + "'"
	}

	function moveChart(index, offset) {
		var target = index + offset
		if (target < 0 || target >= order.length)
			return
		var next = order.slice()
		var chart = next[index]
		next[index] = next[target]
		next[target] = chart
		order = next
		statusText = i18n("Unsaved changes.")
	}

	function removeChart(index) {
		var next = order.slice()
		next.splice(index, 1)
		order = next
		statusText = i18n("Unsaved changes.")
	}

	function addChart(chart) {
		if (available.indexOf(chart) === -1 || order.indexOf(chart) !== -1)
			return
		order = order.concat([chart])
		statusText = i18n("Unsaved changes.")
	}

	function execute(action, args) {
		if (busy)
			return
		busy = true
		actionInFlight = action
		commandInFlight = plasmoid.configuration.backendCommand + " config graphs " + args + " # " + (++execNonce)
		executable.exec(commandInFlight)
	}

	function load() {
		statusText = i18n("Loading Graphs settings…")
		execute("show", "show")
	}

	function save() {
		var history = Number(historyField.text)
		if (!/^[0-9]+$/.test(historyField.text) || !Number.isInteger(history) || history < 1 || history > 10000) {
			statusText = i18n("History length must be between 1 and 10000 samples.")
			return
		}
		statusText = i18n("Saving Graphs settings…")
		execute("apply", "apply " + shellQuote(JSON.stringify({ order: order, history_length: history })))
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
				root.statusText = i18n("Graphs settings failed: %1", data["stderr"].trim())
				return
			}
			if (action === "apply") {
				root.saved = true
				root.load()
				return
			}
			var response
			try {
				response = JSON.parse(data["stdout"])
			} catch (error) {
				root.statusText = i18n("The daemon returned invalid Graphs settings.")
				return
			}
			if (!Array.isArray(response.order) || !Array.isArray(response.available) || !Number.isInteger(response.history_length)) {
				root.statusText = i18n("The daemon returned invalid Graphs settings.")
				return
			}
			root.order = response.order
			root.available = response.available
			historyField.text = String(response.history_length)
			root.loaded = true
			root.statusText = root.saved
				? i18n("Saved. The daemon reloads the file automatically.")
				: i18n("Graphs settings loaded from the user config file.")
			root.saved = false
		}
		function exec(command) {
			connectSource(command)
		}
	}

	LibConfig.Heading {
		text: i18n("Graphs")
	}
	QQC2.Label {
		text: i18n("Choose which charts appear and move them into display order. GPU and network charts appear only when their sources are available. The temperature chart shows CPU and GPU sensors together where supported.")
		wrapMode: Text.WordWrap
	}
	ColumnLayout {
		Layout.fillWidth: true
		Repeater {
			model: root.order.length
			delegate: RowLayout {
				id: chartRow
				required property int index
				readonly property string chart: root.order[index]
				Layout.fillWidth: true
				QQC2.Label {
					text: root.chartLabel(chartRow.chart)
					Layout.fillWidth: true
					elide: Text.ElideRight
				}
				QQC2.Button {
					text: i18n("Up")
					Accessible.name: i18n("Move %1 up", root.chartLabel(chartRow.chart))
					enabled: root.loaded && !root.busy && chartRow.index > 0
					onClicked: root.moveChart(chartRow.index, -1)
				}
				QQC2.Button {
					text: i18n("Down")
					Accessible.name: i18n("Move %1 down", root.chartLabel(chartRow.chart))
					enabled: root.loaded && !root.busy && chartRow.index < root.order.length - 1
					onClicked: root.moveChart(chartRow.index, 1)
				}
				QQC2.Button {
					text: i18n("Remove")
					Accessible.name: i18n("Remove %1", root.chartLabel(chartRow.chart))
					enabled: root.loaded && !root.busy
					onClicked: root.removeChart(chartRow.index)
				}
			}
		}
		RowLayout {
			QQC2.ComboBox {
				id: chartPicker
				Layout.fillWidth: true
				model: root.addable.map(function(chart) { return root.chartLabel(chart) })
				enabled: root.loaded && !root.busy && root.addable.length > 0
				Accessible.name: i18n("Chart to add")
			}
			QQC2.Button {
				text: i18n("Add")
				enabled: root.loaded && !root.busy && chartPicker.currentIndex >= 0 && root.addable.length > 0
				onClicked: root.addChart(root.addable[chartPicker.currentIndex])
			}
		}
	}

	LibConfig.Heading {
		text: i18n("History")
	}
	QQC2.TextField {
		id: historyField
		Kirigami.FormData.label: i18n("Samples to keep:")
		enabled: root.loaded && !root.busy
		inputMethodHints: Qt.ImhDigitsOnly
		Accessible.description: i18n("Number of samples kept for each Graphs chart. Multiply by the history interval on the Daemon page for the approximate time span.")
		onTextEdited: root.statusText = i18n("Unsaved changes.")
	}
	RowLayout {
		QQC2.Button {
			text: i18n("Save Graphs")
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
		text: i18n("The history interval is shared with sparklines and is set on the Daemon page. Chart colors and height use the built-in style.")
		wrapMode: Text.WordWrap
	}

	Component.onCompleted: load()
}
