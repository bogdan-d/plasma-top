# 01: Daemon serves the applet over session D-Bus

**What to build:** The daemon replaces the watched runtime-directory protocol with the session D-Bus interface described in the [spec](../spec.md). Applet requests (page step, present, dismiss, client gone, geometry) reach the async loop as typed requests on an ordered, wakeable, lossless channel; publications go to a sink that updates the interface properties. Page, leases, and geometry live in daemon memory; the persistent geometry cache stays for cold start and diagnostics. Only the production daemon owns the name. The file protocol, its CLI subcommands, and its tests are removed. The applet is migrated in ticket 02, so the widget is expected to be broken between the two tickets.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] Applet requests travel on their own bounded, ordered channel the async loop selects on; rapid page steps apply immediately, in order, without loss; a full channel applies backpressure instead of dropping.
- [ ] Publication goes through a sink; the production sink exposes `PanelHtml`, `TooltipHtml`, `Page`, and `PageCount` on interface `com.github.bogdan_d.PlasmaTop1` at `/com/github/bogdan_d/PlasmaTop`, announcing HTML only on byte changes and `Page`/`PageCount` together on any change, with `Page` clamped below `PageCount`.
- [ ] `StepPage(i)`, `Present(t)`, `Dismiss(t)`, and `SetGeometry(ddbd)` validate arguments per the spec (including the geometry bounds), reject with `InvalidArgs` leaving state unchanged, and take client identity from the sender's unique name.
- [ ] Calls are handled in receive order (no per-call spawning); `NameOwnerChanged` for a client's unique name yields `ClientGone`, subscribed before the name is advertised; requests from an already-gone client are rejected.
- [ ] Leases are keyed by (client, instance); aggregate presentation drives scheduler demand as today; the one-second work-only dismissal grace is preserved; `Present` on an existing lease triggers one bounded activation refresh without toggling aggregate presentation.
- [ ] Geometry in memory feeds orientation/auto-fit merge, reload, and canonical tooltip width; accepted changes update the persistent geometry cache, which is read at cold start and by `render`/`probe`.
- [ ] Only the production daemon exports the interface: the interface is installed before requesting `com.github.bogdan_d.PlasmaTop` with neither replacement nor queueing; an owned name fails startup clearly within a bounded time; `render`, `probe`, and `profiling` never request it.
- [ ] The daemon detects loss of its own session connection independently of notification traffic, clears all leases, keeps its last frames, reconnects with backoff, reinstalls the interface, and reacquires the name, exiting clearly if another daemon owns it.
- [ ] The runtime directory, runtime HTML publication, page flock, `page`/`npages`/`geom`/lease files, their file-watch sources, and their shutdown cleanup are removed; the `page`, `present`, and `dismiss` CLI subcommands are removed; the atomic-write helper remains for remaining callers.
- [ ] Profiling `--stimuli` drives the in-process applet seam instead of protocol files, preserving the config-change stimulus and its completion/timeout correlation.
- [ ] Seam-1 tests (async loop with injected requests and a recording sink) cover the cases listed in the spec's Testing Decisions; the seam-2 suite under `dbus-run-session` covers name ownership, second-instance failure, method mapping, `InvalidArgs`, property values and signals, client disconnect, and daemon reconnect.
- [ ] Every test launching a real daemon runs on a private session bus; tests for removed behavior are deleted, not ported.
- [ ] Full fmt/check/clippy/test/doc gates from the development doc pass; touched files respect the line limits.
