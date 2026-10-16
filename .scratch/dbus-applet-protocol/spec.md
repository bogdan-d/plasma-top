# Applet–daemon protocol over session D-Bus

Status: ready-for-agent

## Problem Statement

The applet and daemon talk through a watched runtime directory. Every display publication makes plasmashell fork a shell to `cat` an HTML file, and every wheel notch, presentation change, lease heartbeat, and geometry change forks another process. The protocol needs atomic temporaries, a flock around the page counter, a 30-second lease heartbeat with 90-second expiry, a 1-second bootstrap retry timer, nonce suffixes to dodge the executable DataSource's duplicate-source dropping, and a rule that nothing else may persist in the watched directory. That is a lot of machinery and recurring process churn inside plasmashell for a program whose purpose is to be cheap.

## Solution

The production daemon owns a well-known name on the user session bus and exposes one versioned interface. The applet binds to it with Plasma's pure-QML D-Bus module (`org.kde.plasma.workspace.dbus`, shipped with Plasma 6.8, the minimum supported version):

- Panel and tooltip HTML, the selected page, and the page count are D-Bus properties; the daemon pushes changes with `PropertiesChanged`, and the applet's `Properties` binding receives them in-process.
- Page steps, presentation, dismissal, and geometry are asynchronous method calls through `SessionBus.asyncCall`.
- `DBusServiceWatcher` tells the applet when the daemon appears or disappears.
- A presentation lease belongs to the caller's bus connection and applet instance, so a crashed plasmashell releases its leases without a heartbeat.

In steady state, plasmashell starts no processes for PlasmaTop display, paging, or presentation.

## User Stories

1. As a laptop user, I want the panel to refresh without plasmashell forking a process each poll interval, so that PlasmaTop costs less power.
2. As a user, I want panel updates to appear as soon as the daemon publishes, so that readings feel live.
3. As a user, I want the tooltip to update while I hover or pin it, so that I can watch values change.
4. As a user, I want tooltip work to stop when no tooltip is presented, so that hidden pages cost nothing.
5. As a user, I want hovering to show the last tooltip immediately, so that I never stare at an empty popup.
6. As a user, I want one wheel gesture to turn exactly one page, applied immediately, so that paging feels precise.
7. As a user, I want rapid wheel gestures never to drop, duplicate, or reorder a page step, so that paging stays predictable.
8. As a user, I want paging to wrap in both directions, so that I can reach any page quickly.
9. As a user, I want paging to do nothing when no deep-dive pages are configured, so that the wheel is harmless.
10. As a user, I want the page indicator to reflect the selected page and the page count, so that I know where I am.
11. As a user with several PlasmaTop widgets, I want any presented widget to keep the tooltip active, so that one widget closing does not blank another.
12. As a user with several widgets, I want hidden widgets not to re-render tooltip text, so that only the presented one pays layout cost.
13. As a user, I want removing one widget to release its lease even while plasmashell keeps running, so that the daemon stops building a tooltip nobody can see.
14. As a user, I want the tooltip to stop building after the last widget dismisses, with the existing short grace for brief hover gaps, so that quick pointer movements don't cancel page jobs.
15. As a user, I want leases held by a crashed or restarted plasmashell to disappear immediately, so that the daemon does not keep building an invisible tooltip.
16. As a user, I want the widget to recover by itself when the daemon starts after plasmashell, so that login order doesn't matter.
17. As a user, I want the widget to recover by itself when the daemon restarts, even while a tooltip is presented or a call is in flight, so that a crash or upgrade restart doesn't leave a stale or dead widget.
18. As a user, I want the widget to keep showing its last panel and tooltip while the daemon is briefly gone or has not published yet, so that a restart doesn't flash empty text.
19. As a user, I want the panel auto-fit to follow panel resizes, orientation changes, and font changes, so that bars and columns fit.
20. As a user, I want the first paint after a reboot to be width-fitted, so that the panel doesn't jump once the applet reports geometry.
21. As a user, I want graphs sized to the tooltip font, so that charts fill the tooltip width without guessing DPI.
22. As a user, I want middle-click to keep launching my configured click action, so that existing habits keep working.
23. As a user, I want the light/dark theme, config hot-reload, and per-machine overrides to keep working unchanged, so that this change is invisible except for cost.
24. As a desktop-mode user, I want the planar presentation to keep the tooltip presented, so that the always-on readout keeps updating.
25. As a user, I want only one daemon to serve the applet, and a second daemon never to take over the first, so that two daemons never fight over output.
26. As a user, I want a second daemon start to fail clearly, so that I can tell why it exited.
27. As a user, I want `render`, `probe`, and `profiling` to run while the daemon is serving, so that diagnostics never disturb the live widget.
28. As a user, I want the daemon to recover its service after its own bus connection drops, so that the widget does not go dead until logout.
29. As a user, I want other local users unable to read or drive my PlasmaTop, so that my system details stay private.
30. As a user, I want malformed or hostile method arguments rejected without changing daemon state, so that a buggy client cannot crash or corrupt it.
31. As a user upgrading PlasmaTop, I want a mismatched daemon and applet to fail visibly in the log, so that I know to log out and back in.
32. As a user, I want uninstall to clean up runtime files left by older versions, so that my runtime directory stays clean.
33. As a maintainer, I want the runtime directory protocol, page flock, lease files, and heartbeat removed, so that there is less code to keep correct.
34. As a maintainer, I want the bootstrap timer and protocol-related nonce workarounds removed from the applet, so that the QML is easier to read.
35. As a maintainer, I want the protocol to be one versioned interface, so that a future break is explicit.
36. As a maintainer, I want most protocol behavior tested without a bus, so that tests stay fast and deterministic.
37. As a maintainer, I want one suite that proves the real D-Bus wiring, so that the mapping between the interface and daemon requests cannot drift.
38. As a maintainer, I want the profiling stimuli to drive the same applet requests as the real applet, so that measurements describe production behavior.
39. As a maintainer, I want the isolated QML verification and every real-daemon test to run against a private session bus, so that they never touch my live session.
40. As a maintainer, I want the docs, tools, packaging, and agent instructions to describe the new contract, so that no one preserves the old file rules.

## Implementation Decisions

### Interface

- Well-known name `com.github.bogdan_d.PlasmaTop`, object path `/com/github/bogdan_d/PlasmaTop`, interface `com.github.bogdan_d.PlasmaTop1`. The trailing `1` is the protocol version; any incompatible change bumps it.
- Read-only properties, each announced with its new value in `PropertiesChanged`:
  - `PanelHtml` (`s`): the latest published panel bytes.
  - `TooltipHtml` (`s`): the latest published tooltip bytes. It changes only while the tooltip is presented, matching today's publication rules; between presentations it keeps the last good value. It is empty only before the daemon's first tooltip publication.
  - `Page` (`u`) and `PageCount` (`u`): the current selection. `Page` is always below `PageCount` (clamped when the count shrinks). They are announced together in one signal whenever either value changes, independently of HTML changes. Retained HTML may briefly describe the previous selection; that is accepted, and no generation field is added.
- HTML properties are announced only when the published bytes actually change, as file publication does today.
- Methods (all `()` replies; errors use D-Bus `InvalidArgs` and leave state unchanged). A successful reply means the daemon accepted the request into its ordered queue:
  - `StepPage(i delta)`: accepts only `+1` or `-1`; wraps against the page count; a no-op when the page count is at most one.
  - `Present(t instance)` and `Dismiss(t instance)`: the instance must be positive. The client identity is the message sender's unique name, never a caller-supplied value. `Dismiss` of an unknown lease is a successful no-op.
  - `SetGeometry(d usable_px, d glyph_advance, b vertical, d tooltip_advance)`: `usable_px` in `(0, 16384]`, both advances in `(0, 512]`; NaN and infinity rejected. Identical geometry is ignored.

### Daemon: request delivery

- The D-Bus adapter translates calls and client disconnects into typed applet requests (`StepPage`, `Present { client, instance }`, `Dismiss { client, instance }`, `ClientGone { client }`, `Geometry`).
- Applet requests travel on their own bounded, ordered channel that the async loop selects on, so a request wakes the loop immediately. They are not coalesced and are never silently dropped; when the channel is full, the method call waits for capacity (backpressure) rather than failing. This is deliberately different from the coalesced UPower/sleep event path.
- The interface is served without per-call task spawning, so calls from one client are handled in receive order.
- `ClientGone` comes from `NameOwnerChanged` for the client's unique name. The daemon subscribes to disconnect notifications before it advertises the well-known name. A request from a client already reported gone is rejected, so a delayed `Present` cannot recreate a dead client's lease.

### Daemon: service ownership and reconnect

- Only the production daemon exports the interface and owns the name. `render`, `probe`, and `profiling` never export it; diagnostics keep their current bus-absence behavior, and timed profiling drives the in-process applet seam instead of the bus.
- The interface is installed before the name is requested. The name is requested with neither replacement allowed nor queueing; if the name is owned, daemon startup fails with a clear error within a bounded time.
- The daemon watches its own session connection for loss independently of notification traffic. On loss it clears all leases (counting as all clients gone), keeps its last published frames, reconnects with the existing backoff, reinstalls the interface, and reacquires the name with the same flags. If another daemon owns the name on reconnect, it exits with a clear error rather than competing.

### Daemon: leases and presentation

- Leases are keyed by `(client unique name, instance)`. A lease lives until `Dismiss`, `ClientGone`, or loss of the daemon's own connection. Lease expiry, heartbeats, and the presented-directory scan are deleted. The one-second work-only dismissal grace stays.
- Aggregate presentation is true while any lease exists. Only aggregate transitions change scheduler demand, as today.
- New behavior: `Present` for an existing lease requests one bounded activation refresh of the tooltip without toggling aggregate presentation or affecting other leases. Today a repeated present only rewrites a file.

### Daemon: state, publication, and geometry

- The page counter lives in daemon memory and resets to zero on daemon restart.
- Geometry lives in daemon memory and feeds the same consumers that read the geometry file today: orientation and auto-fit merge, config reload, and canonical tooltip width for graph sizing.
- The persistent geometry cache in the user cache directory stays, outside the protocol. The daemon writes it when accepted geometry changes and reads it at cold start, so the first paint after reboot is width-fitted. `render` and `probe` also read it in place of the removed runtime geometry file.
- Publication goes to a publication sink instead of atomic files. The production sink updates the interface properties and emits signals; the test sink records frames. Panel and tooltip stay independent publications.
- Geometry, page, and presentation stop being file-watch sources. Config, style, theme, and Plasma config watching are unchanged.
- The runtime directory, runtime HTML publication, the page flock, and the `page`, `npages`, `geom`, and lease files are deleted, along with shutdown cleanup of those files. The atomic-write helper stays wherever other callers still need it, including the profiling config-change stimuli.
- The retained tooltip across daemon restarts moves from disk to the applet's memory. After a plasmashell restart, the first presentation shows the tooltip as soon as the bounded activation refresh publishes.

### Applet

- One `Properties` object bound to the interface receives panel HTML, tooltip HTML, page, and page count. The `cat` commands, `FolderListModel` watcher, read debounce, and bootstrap timer are deleted.
- Received values and displayed text are separate. Displayed panel text updates from every non-empty received panel value. Displayed tooltip text updates from received tooltip values only while this widget is presented, and catches up from the latest received value when it becomes presented. Empty values and daemon absence never clear displayed text.
- Method calls use `SessionBus.asyncCall` with `dbusMessage` and explicit signatures `(i)`, `(t)`, and `(ddbd)`, so the module does not introspect before each call.
- `DBusServiceWatcher` tracks the well-known name. Every time the daemon registers, the applet: starts a new service lifetime and ignores replies from earlier lifetimes; forgets cached lease state; calls `updateAll()` on the `Properties` object; resends geometry; and presents again if it is presented. This happens whether or not the module also fetched properties on its own.
- Geometry is sent only once panel width, panel glyph advance, and tooltip glyph advance are all positive.
- Wheel gestures call `StepPage` directly. The configurable mouse-wheel command settings are removed from config, the Appearance page, and installer substitution.
- The presentation-lease component calls `Present`/`Dismiss` asynchronously and keeps its one-in-flight serialization. It drops its heartbeat timer. It keeps sending `Dismiss` on destruction, including when a presentation call is still in flight, because a removed widget does not close plasmashell's shared bus connection.
- The click command setting and the `plasma-top click` CLI stay, because they launch an external program and run only on an explicit user click. The small nonce helper stays for that click path. Only the nonce use for paging and presentation is removed.
- Settings pages that run `plasma-top config …` keep their existing executable transport and handling; they run only while the settings dialog is open.
- `X-Plasma-API-Minimum-Version` becomes 6.8.

### CLI

- `page`, `present`, and `dismiss` subcommands are removed. `daemon`, `render`, `probe`, `profiling`, `list-items`, `config`, and `click` remain.
- The profiling `--stimuli` sequence drives the applet seam with typed requests instead of writing protocol files. The config-change stimulus and its completion and timeout correlation are preserved.

### Migration, packaging, and docs

- Uninstall still removes leftover runtime files from older versions. Install and upgrade never delete runtime files, because the old daemon and applet keep using them until logout.
- The installer and the user-install test drop the wheel-command substitution and assertions. AUR packaging declares Plasma 6.8 or newer. README requirements say Plasma 6.8 or newer.
- New ADR: the applet talks to the daemon over the session bus with pushed properties, and leases are tied to bus connections.
- Update the design, performance, development, and metrics-inventory docs; the README overview; the project QML skill rules; and the load-bearing contracts in agent instructions (drop the watched runtime root rule; add the versioned interface, daemon-only name ownership, and lease rules).
- Add glossary entries for presentation lease and applet client if the glossary lacks them.

## Testing Decisions

- Good tests assert observable protocol behavior: which frames and property values are published, the selected page and page count, whether tooltip work runs, and which requests are rejected. They do not assert internal channel shapes, call counts on private helpers, or zbus internals.
- **Seam 1: daemon applet seam (most coverage).** Drive the async loop with injected applet requests and a recording publication sink, with no bus. Cover:
  - rapid page steps applied immediately, in order, and without loss while the loop would otherwise sleep long with no owner work;
  - page stepping in both directions, wrap, single-page no-op, and `Page` clamping when the page count shrinks while hidden, announced as metadata without new tooltip HTML;
  - lease aggregation across clients and instances, `Dismiss` of one instance while another on the same client stays presented, `ClientGone` releasing only that client's leases, and a `Present` arriving after `ClientGone` being rejected;
  - dismissal grace, activation refresh on present, and the new re-present refresh not toggling aggregate presentation;
  - geometry acceptance, dedup, bounds rejection leaving accepted geometry unchanged, effect on auto-fit and canonical tooltip width, and cold start from the persistent cache;
  - existing first-paint and publication timing assertions.
  Prior art: the async loop integration and state tests and the daemon lifecycle tests, which already run the loop with fixture roots and assert published output.
- **Seam 2: D-Bus adapter suite.** Run under a private bus via `dbus-run-session`. Cover name acquisition; a second instance failing without replacing the first; each method's mapping to a request with the sender's unique name; `InvalidArgs` on bad input; property `Get` values; `PropertiesChanged` carrying new values, with `Page` and `PageCount` together; a client disconnecting right after `Present` producing `ClientGone`; and daemon connection loss followed by reconnect, interface reinstall, and name reacquisition. Prior art: the existing D-Bus adapter tests and fixture D-Bus traces.
- Every test that launches a real daemon process, including the CLI daemon tests, runs on a private session bus; disposable XDG roots no longer isolate it. Diagnostics tests assert that `render` and `probe` do not request the name.
- **QML.** Update the isolated `qml_verify` smoke to run the daemon and applet on a private session bus and exercise all three method signatures, initial property hydration, a pushed value, daemon restart recovery while presented, and two widget instances with only one presented. The existing live and plasmoidviewer validation covers panel form factors. A Rust client cannot prove Qt's argument encoding, so the QML smoke is the evidence for it.
- Delete tests for removed behavior (page flock concurrency, runtime HTML publication, runtime paths, lease file scan and expiry) rather than porting them.
- Performance evidence: a steady-state profile showing no process spawns from plasmashell for display, paging, or presentation, and end-to-end publication latency, including marshalling and QML delivery, no worse than the current file path.

## Out of Scope

- D-Bus activation of the daemon; the systemd user unit stays the launcher.
- Per-instance geometry; geometry stays last-writer-wins, as today.
- Moving the settings pages' `plasma-top config` calls or the click command to D-Bus.
- Mixed-version compatibility between daemon and applet; upgrades still require a logout.
- Plasma versions before 6.8.
- Changing rendering, HTML layout, sensors, or scheduling policy beyond the re-present refresh.

## Further Notes

- The installed module's type metadata confirms `Properties` (with `updateAll()`), `SessionBus.asyncCall`, `dbusMessage`, `DBusServiceWatcher`, and the typed scalar wrappers. KDE upstream source shows `Properties` performing an initial `GetAll` and applying pushed values directly, but the installed build's internals were not inspected. The applet's unconditional `updateAll()` on registration makes the design independent of that detail; the QML smoke confirms the rest.
- Qt documents that a direct owner replacement need not produce unregister/register signals on a service watcher. Forbidding name replacement on the daemon side keeps registration transitions observable.
- Tooltip HTML can reach hundreds of KiB with graph PNG data URIs. That is far below the bus's message limit, but the signal goes to every subscriber, so announcing only real changes and only while presented matters.
- The session bus is per-user and authenticated, so no extra access control is needed.
- Spec reviewed by Codex (`gpt-6.1-sol`, xhigh) on 2026-10-17; all findings verified against the code and incorporated.
