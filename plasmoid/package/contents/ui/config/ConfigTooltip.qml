pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls as QQC2
import QtQuick.Layouts

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
	property var sections: []
	property var availableItems: []
	property string statusText: i18n("Loading main tooltip settings…")

	function cloneSections() {
		return sections.map(function(section) {
			return { key: section.key, title: section.title, enabled: section.enabled, items: section.items.slice() }
		})
	}

	function setSectionEnabled(index, enabled) {
		var next = cloneSections()
		next[index].enabled = enabled
		sections = next
		statusText = i18n("Unsaved changes.")
	}

	function moveSection(index, offset) {
		var next = cloneSections()
		var target = index + offset
		if (target < 0 || target >= next.length)
			return
		var section = next[index]
		next[index] = next[target]
		next[target] = section
		sections = next
		statusText = i18n("Unsaved changes.")
	}

	function moveItem(sectionIndex, itemIndex, offset) {
		var next = cloneSections()
		var items = next[sectionIndex].items
		var target = itemIndex + offset
		if (target < 0 || target >= items.length)
			return
		var item = items[itemIndex]
		items[itemIndex] = items[target]
		items[target] = item
		sections = next
		statusText = i18n("Unsaved changes.")
	}

	function removeItem(sectionIndex, itemIndex) {
		var next = cloneSections()
		next[sectionIndex].items.splice(itemIndex, 1)
		sections = next
		statusText = i18n("Unsaved changes.")
	}

	function addItem(sectionIndex, token) {
		var next = cloneSections()
		if (next[sectionIndex].items.indexOf(token) !== -1 && !token.startsWith("separator_")) {
			statusText = i18n("That reading is already in this section.")
			return
		}
		next[sectionIndex].items.push(token)
		sections = next
		statusText = i18n("Unsaved changes.")
	}

	function itemLabel(token) {
		var names = token.replace(/_/g, " ").replace(":", " · ").split(" ")
		var abbreviations = ["cpu", "gpu", "amd", "ip", "io", "hd", "smart", "ssid"]
		return names.map(function(word, index) {
			if (abbreviations.indexOf(word.toLowerCase()) !== -1)
				return word.toUpperCase()
			return index === 0 ? word.charAt(0).toUpperCase() + word.slice(1) : word
		}).join(" ")
	}

	function shellQuote(value) {
		return "'" + value.split("'").join("'\"'\"'") + "'"
	}

	function execute(action, args) {
		if (busy)
			return
		busy = true
		actionInFlight = action
		commandInFlight = plasmoid.configuration.backendCommand + " config tooltip " + args + " # " + (++execNonce)
		executable.exec(commandInFlight)
	}

	function load() {
		statusText = i18n("Loading main tooltip settings…")
		execute("show", "show")
	}

	function save() {
		statusText = i18n("Saving main tooltip settings…")
		execute("apply", "apply " + shellQuote(JSON.stringify({ sections: sections })))
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
				root.statusText = i18n("Main tooltip settings failed: %1", data["stderr"].trim())
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
				root.statusText = i18n("The daemon returned invalid tooltip settings.")
				return
			}
			if (!Array.isArray(response.sections) || !Array.isArray(response.available)) {
				root.statusText = i18n("The daemon returned invalid tooltip settings.")
				return
			}
			root.sections = response.sections
			root.availableItems = response.available
			root.loaded = true
			root.statusText = root.saved
				? i18n("Saved. The daemon reloads the file automatically.")
				: i18n("Main tooltip settings loaded from the user config file.")
			root.saved = false
		}
		function exec(command) {
			connectSource(command)
		}
	}

	LibConfig.Heading {
		text: i18n("Main tooltip")
	}
	QQC2.Label {
		text: i18n("Choose the sections and readings on the first tooltip page. Rows without a usable reading may be hidden. Use Up and Down to set display order.")
		wrapMode: Text.WordWrap
	}
	ColumnLayout {
		Layout.fillWidth: true
		Repeater {
			model: root.sections.length
			delegate: QQC2.Frame {
				id: sectionFrame
				required property int index
				readonly property var section: root.sections[index]
				Layout.fillWidth: true
				contentItem: ColumnLayout {
					RowLayout {
						QQC2.CheckBox {
							text: sectionFrame.section.title || sectionFrame.section.key
							checked: sectionFrame.section.enabled
							enabled: root.loaded && !root.busy
							Layout.fillWidth: true
							onClicked: root.setSectionEnabled(sectionFrame.index, checked)
						}
						QQC2.Button {
							text: i18n("Up")
							enabled: root.loaded && !root.busy && sectionFrame.index > 0
							onClicked: root.moveSection(sectionFrame.index, -1)
						}
						QQC2.Button {
							text: i18n("Down")
							enabled: root.loaded && !root.busy && sectionFrame.index < root.sections.length - 1
							onClicked: root.moveSection(sectionFrame.index, 1)
						}
					}
					Repeater {
						model: sectionFrame.section.items.length
						delegate: RowLayout {
							id: itemRow
							required property int index
							readonly property string token: sectionFrame.section.items[index]
							QQC2.Label {
								text: root.itemLabel(itemRow.token)
								Layout.fillWidth: true
								elide: Text.ElideRight
								Accessible.description: itemRow.token
							}
							QQC2.ToolButton {
								text: "↑"
								Accessible.name: i18n("Move %1 up", root.itemLabel(itemRow.token))
								enabled: root.loaded && !root.busy && itemRow.index > 0
								onClicked: root.moveItem(sectionFrame.index, itemRow.index, -1)
							}
							QQC2.ToolButton {
								text: "↓"
								Accessible.name: i18n("Move %1 down", root.itemLabel(itemRow.token))
								enabled: root.loaded && !root.busy && itemRow.index < sectionFrame.section.items.length - 1
								onClicked: root.moveItem(sectionFrame.index, itemRow.index, 1)
							}
							QQC2.ToolButton {
								text: "×"
								Accessible.name: i18n("Remove %1", root.itemLabel(itemRow.token))
								enabled: root.loaded && !root.busy
								onClicked: root.removeItem(sectionFrame.index, itemRow.index)
							}
						}
					}
					RowLayout {
						QQC2.ComboBox {
							id: itemPicker
							Layout.fillWidth: true
							model: root.availableItems.map(function(token) { return root.itemLabel(token) })
							enabled: root.loaded && !root.busy
							Accessible.name: i18n("Reading to add to %1", sectionFrame.section.title || sectionFrame.section.key)
						}
						QQC2.Button {
							text: i18n("Add")
							enabled: root.loaded && !root.busy && itemPicker.currentIndex >= 0
							onClicked: root.addItem(sectionFrame.index, root.availableItems[itemPicker.currentIndex])
						}
					}
				}
			}
		}
	}
	RowLayout {
		QQC2.Button {
			text: i18n("Save main tooltip")
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
		text: i18n("Section titles and machine-specific overrides remain editable in config.toml and machines.toml.")
		wrapMode: Text.WordWrap
	}

	Component.onCompleted: load()
}
