---
status: accepted
---

# Serve the applet over session D-Bus with connection-owned leases

The daemon owns `com.github.bogdan_d.PlasmaTop` on the user session bus and exports one versioned interface, `com.github.bogdan_d.PlasmaTop1`, at `/com/github/bogdan_d/PlasmaTop`. Panel HTML, tooltip HTML, the selected page, and the page count are properties pushed with `PropertiesChanged`; page steps, presentation, dismissal, and geometry are method calls. A presentation lease belongs to the caller's bus connection and applet instance. The applet binds with Plasma's pure-QML `org.kde.plasma.workspace.dbus` module, so Plasma 6.8 is the minimum version.

This replaced a watched runtime directory. Every publication made plasmashell fork `cat`, and every wheel notch, presentation change, lease heartbeat, and geometry change forked another process. The file protocol also needed atomic temporaries, a flock around the page counter, a 30-second heartbeat with 90-second expiry, a bootstrap retry timer, and nonce suffixes against the executable DataSource dropping duplicate sources. The bus already delivers ordered calls, change signals, owner tracking, and per-user authentication, so each of those mechanisms disappears instead of moving.

## Consequences

- In steady state plasmashell starts no processes for display, paging, or presentation. Middle-click and the settings pages still run the CLI, only on explicit user action.
- Only the production daemon owns the name, requested without replacement or queueing. A second daemon fails at startup, and `render`, `probe`, and `profiling` never export the interface.
- Calls are dispatched in receive order onto a bounded channel that applies backpressure rather than dropping requests. Validation failures return `InvalidArgs` and change nothing.
- A lease ends on `Dismiss`, on the client's disconnect, or when the daemon loses its own connection. Leases need no heartbeat or expiry, and a crashed plasmashell releases its leases at once.
- Page, leases, and geometry live in daemon memory. A geometry cache in the user cache directory keeps the first paint after reboot width-fitted; it is not part of the protocol.
- The applet keeps displayed text separate from received values, so a daemon restart never blanks the widget. On every daemon registration it fetches all properties, resends geometry, and presents again if presented.
- Any incompatible interface change bumps the trailing version. The daemon and the applet still come from one package version; upgrades take effect at the next login.
