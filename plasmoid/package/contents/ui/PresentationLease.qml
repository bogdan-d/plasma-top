import QtQuick

import org.kde.plasma.workspace.dbus as DBus

// This widget's presentation lease on the daemon. The daemon ties the lease to plasmashell's bus connection, so a crash releases it; a removed widget does not close that shared connection, so destruction still dismisses.
Item {
	id: root

	required property bool presented
	required property int instanceId
	// The daemon's service, path and interface, as `asyncCall` message fields.
	required property var target
	// Sends one call to the current daemon and reports success through `done(ok)`; replies from an earlier daemon never arrive.
	required property var call
	readonly property bool validInstanceId: instanceId > 0
	// One call in flight at a time, so the daemon always receives this widget's latest intent last.
	property bool running: false
	// Whether leaseActive is what the daemon holds; a failed call makes it unknown.
	property bool leaseKnown: true
	property bool leaseActive: false

	// A newly registered daemon holds no lease for us: forget ours and present again if presented.
	function restart() {
		running = false
		leaseKnown = true
		leaseActive = false
		drain()
	}

	function drain() {
		if (running)
			return
		if (!validInstanceId) {
			console.warn("[plasma-top] plasmoid id must be a positive integer:", instanceId)
			return
		}
		var intent = presented
		if (leaseKnown && leaseActive === intent)
			return
		running = true
		call(intent ? "Present" : "Dismiss", "(t)", [instanceId], ok => {
			root.running = false
			root.leaseKnown = ok
			if (ok)
				root.leaseActive = intent
			// A failure waits for the next intent change or daemon registration instead of retrying in a loop.
			if (ok || root.presented !== intent)
				root.drain()
		})
	}

	onPresentedChanged: drain()
	Component.onDestruction: {
		if (validInstanceId)
			DBus.SessionBus.asyncCall(Object.assign({ member: "Dismiss", signature: "(t)", arguments: [instanceId] }, target))
	}
}
