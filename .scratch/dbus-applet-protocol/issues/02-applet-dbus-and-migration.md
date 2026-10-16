# 02: Applet talks to the daemon over D-Bus, and the migration is finished

**What to build:** The Plasma applet consumes the daemon's D-Bus interface from ticket 01 using the pure-QML `org.kde.plasma.workspace.dbus` module, per the [spec](../spec.md): pushed properties for panel/tooltip/page state, async method calls for paging, presentation, and geometry, and a service watcher for daemon restarts. In steady state plasmashell starts no processes for display, paging, or presentation. Installer, packaging, docs, and agent instructions describe the new contract, and the end result is verified live.

**Blocked by:** 01 (Daemon serves the applet over session D-Bus)

**Status:** ready-for-agent

- [ ] One `Properties` binding receives `PanelHtml`, `TooltipHtml`, `Page`, and `PageCount`; the `cat` commands, `FolderListModel` watcher, read debounce, and bootstrap timer are removed.
- [ ] Received and displayed text are separate: panel text updates from non-empty values; tooltip text updates only while this widget is presented and catches up on presentation; empty values and daemon absence never clear displayed text.
- [ ] Method calls use `SessionBus.asyncCall` with `dbusMessage` and explicit signatures `(i)`, `(t)`, `(ddbd)`; geometry is sent only when panel width and both glyph advances are positive.
- [ ] On every daemon registration the applet starts a new service lifetime ignoring older replies, forgets cached lease state, calls `updateAll()`, resends geometry, and presents again if presented.
- [ ] The wheel calls `StepPage`; the wheel command settings are removed from config, the Appearance page, and installer substitution.
- [ ] The presentation-lease component uses `Present`/`Dismiss`, keeps one-in-flight serialization, drops the heartbeat, and still dismisses on destruction, including with a call in flight.
- [ ] Middle-click keeps its configurable command and the nonce helper; settings pages keep their existing `plasma-top config` transport.
- [ ] Applet minimum Plasma API version, README requirements, and AUR packaging say Plasma 6.8 or newer; the user-install test drops wheel-command assertions; uninstall still removes leftover runtime files from older versions; install/upgrade never delete them.
- [ ] New ADR records the session-bus protocol and connection-owned leases; the design, performance, development, and metrics-inventory docs, README, project QML skill rules, and agent-instruction load-bearing contracts are updated; glossary gains presentation lease and applet client if missing.
- [ ] The isolated `qml_verify` smoke runs daemon and applet on a private session bus and exercises all three method signatures, initial hydration, a pushed value, daemon restart while presented, and two widgets with only one presented.
- [ ] Live verification on a Plasma 6.8 session: panel and tooltip update, wheel paging, pinning, desktop mode, middle-click, multiple widgets, daemon restart, and plasmashell restart all work.
- [ ] Performance evidence: a steady-state profile shows no process spawns from plasmashell for display, paging, or presentation, and end-to-end publication latency is no worse than the old file path.
- [ ] `shellcheck` and `shfmt` pass on changed shell scripts; full Rust gates pass.
