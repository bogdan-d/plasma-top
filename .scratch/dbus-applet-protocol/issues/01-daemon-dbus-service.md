# 01: Daemon serves the applet over session D-Bus

**What to build:** The daemon replaces the watched runtime-directory protocol with the session D-Bus interface described in the [spec](../spec.md). Applet requests (page step, present, dismiss, client gone, geometry) reach the async loop as typed requests on an ordered, wakeable, lossless channel; publications go to a sink that updates the interface properties. Page, leases, and geometry live in daemon memory; the persistent geometry cache stays for cold start and diagnostics. Only the production daemon owns the name. The file protocol, its CLI subcommands, and its tests are removed. The applet is migrated in ticket 02, so the widget is expected to be broken between the two tickets.

**Blocked by:** None (can start immediately)

**Status:** done

- [x] Applet requests travel on their own bounded, ordered channel the async loop selects on; rapid page steps apply immediately, in order, without loss; a full channel applies backpressure instead of dropping.
- [x] Publication goes through a sink; the production sink exposes `PanelHtml`, `TooltipHtml`, `Page`, and `PageCount` on interface `com.github.bogdan_d.PlasmaTop1` at `/com/github/bogdan_d/PlasmaTop`, announcing HTML only on byte changes and `Page`/`PageCount` together on any change, with `Page` clamped below `PageCount`.
- [x] `StepPage(i)`, `Present(t)`, `Dismiss(t)`, and `SetGeometry(ddbd)` validate arguments per the spec (including the geometry bounds), reject with `InvalidArgs` leaving state unchanged, and take client identity from the sender's unique name.
- [x] Calls are handled in receive order (no per-call spawning); `NameOwnerChanged` for a client's unique name yields `ClientGone`, subscribed before the name is advertised; requests from an already-gone client are rejected.
- [x] Leases are keyed by (client, instance); aggregate presentation drives scheduler demand as today; the one-second work-only dismissal grace is preserved; `Present` on an existing lease triggers one bounded activation refresh without toggling aggregate presentation.
- [x] Geometry in memory feeds orientation/auto-fit merge, reload, and canonical tooltip width; accepted changes update the persistent geometry cache, which is read at cold start and by `render`/`probe`.
- [x] Only the production daemon exports the interface: the interface is installed before requesting `com.github.bogdan_d.PlasmaTop` with neither replacement nor queueing; an owned name fails startup clearly within a bounded time; `render`, `probe`, and `profiling` never request it.
- [x] The daemon detects loss of its own session connection independently of notification traffic, clears all leases, keeps its last frames, reconnects with backoff, reinstalls the interface, and reacquires the name, exiting clearly if another daemon owns it.
- [x] The runtime directory, runtime HTML publication, page flock, `page`/`npages`/`geom`/lease files, their file-watch sources, and their shutdown cleanup are removed; the `page`, `present`, and `dismiss` CLI subcommands are removed; the atomic-write helper remains for remaining callers.
- [x] Profiling `--stimuli` drives the in-process applet seam instead of protocol files, preserving the config-change stimulus and its completion/timeout correlation.
- [x] Seam-1 tests (async loop with injected requests and a recording sink) cover the cases listed in the spec's Testing Decisions; the seam-2 suite under `dbus-run-session` covers name ownership, second-instance failure, method mapping, `InvalidArgs`, property values and signals, client disconnect, and daemon reconnect.
- [x] Every test launching a real daemon runs on a private session bus; tests for removed behavior are deleted, not ported.
- [x] Full fmt/check/clippy/test/doc gates from the development doc pass; touched files respect the line limits.

## Comments

- The test-only legacy synchronous loop (`src/daemon/scheduled_loop/`) and its tests were deleted with the file protocol; production never ran it.
- The atomic-write helper moved to `src/fs_atomic.rs`; it now serves the geometry cache and the profiling config stimulus.
- Closing the disconnect race: the object server and the `NameOwnerChanged` stream are separate zbus streams, so a disconnect can be handled before an earlier `Present` from the same client. On a client's first `Present`, the adapter asks the bus `NameHasOwner` after delivering it, in a separate task, and reports `ClientGone` itself when the client is already gone. The loop also rejects `Present` from clients it has seen leave.
- A call with the wrong D-Bus signature gets zbus's own error (`org.freedesktop.zbus.Error`, "Signature mismatch") rather than `InvalidArgs`. It is still rejected and changes no state. Validation failures on well-typed arguments use `InvalidArgs`.
- Coverage gaps: cold start from the persistent geometry cache is covered only by the cache round-trip test plus the one-line read in `async_loop::run`, because orientation comes from the user's real appletsrc and a hermetic loop-level check was not practical. The "another daemon owns the name after a reconnect" exit path is not exercised by a test.
- The seam-2 suite and the CLI daemon tests start their own private `dbus-daemon --session` per test (equivalent to `dbus-run-session`, with a fixed socket so the reconnect test can restart the bus at the same address).
- Codex review (`gpt-6.1-sol`, xhigh) found four issues, all confirmed and fixed:
  - Awaiting `NameHasOwner` inside the non-spawning dispatcher could deadlock once a client pipelined enough calls to fill zbus's incoming queue. The check now runs in its own task. A private-bus test sends 200 calls without waiting for replies; it timed out against the old code.
  - Calls and disconnects still buffered on a lost connection could reach the loop after `ConnectionLost`, and a restarted bus reuses unique names. All of a connection's events now pass through one gate that the connection retires when it sends `ConnectionLost`; anything later is dropped. This race has no deterministic test.
  - A cancelled acquisition attempt kept its connection alive through the disconnect forwarder. The forwarder now starts only after the name is owned; the subscription is still made before the name request.
  - `PageCount` started at 0 before the first announcement. It now starts at 1, and the property test reads the initial values.
