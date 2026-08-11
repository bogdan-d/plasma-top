import QtQuick

import org.kde.plasma.plasma5support as Plasma5Support

Item {
	id: root

	required property bool presented
	required property int instanceId
	required property string backendCommand
	readonly property bool validInstanceId: instanceId > 0
	property int execNonce: 0
	property bool commandRunning: false
	property bool commandTarget: false
	property bool commandWasTarget: false
	property string commandInFlight: ""
	property bool leaseKnown: false
	property bool leaseActive: false
	property bool refreshPending: false
	property int retriesRemaining: 1

	function command(action) {
		return backendCommand + " " + action + " " + instanceId
	}

	function sync(refresh) {
		if (!validInstanceId) {
			console.warn("[plasma-top] plasmoid id must be a positive integer:", instanceId)
			return
		}
		if (commandTarget !== presented) {
			commandTarget = presented
			retriesRemaining = 1
		}
		if (refresh && presented) {
			refreshPending = true
			retriesRemaining = 1
		}
		drain()
	}

	function drain() {
		if (commandRunning)
			return
		var target = commandTarget
		if (leaseKnown && leaseActive === target && !(target && refreshPending))
			return
		commandInFlight = command(target ? "present" : "dismiss") + " # " + (++execNonce)
		commandRunning = true
		commandWasTarget = target
		if (target)
			refreshPending = false
		executable.exec(commandInFlight)
	}

	Plasma5Support.DataSource {
		id: executable
		engine: "executable"
		connectedSources: []
		onNewData: (sourceName, data) => {
			var exitCode = data["exit code"]
			var stderr = data["stderr"]
			disconnectSource(sourceName)
			if (sourceName !== root.commandInFlight)
				return
			var actionPresented = root.commandWasTarget
			root.commandRunning = false
			root.commandInFlight = ""
			root.leaseKnown = exitCode === 0
			if (exitCode === 0) {
				root.leaseActive = actionPresented
				root.retriesRemaining = 1
				if (actionPresented)
					root.refreshPending = false
			} else {
				console.warn("[plasma-top] presentation command failed:", exitCode, stderr)
			}
			if (exitCode === 0 || root.commandTarget !== actionPresented) {
				root.drain()
			} else if (root.retriesRemaining > 0) {
				root.retriesRemaining -= 1
				root.drain()
			}
		}
		function exec(command) {
			if (command)
				connectSource(command)
		}
	}

	Timer {
		interval: 30000
		repeat: true
		running: root.presented
		onTriggered: root.sync(true)
	}

	onPresentedChanged: sync(false)
	Component.onCompleted: sync(false)
	Component.onDestruction: {
		if (validInstanceId)
			executable.exec(command("dismiss") + " # " + (++execNonce))
	}
}
