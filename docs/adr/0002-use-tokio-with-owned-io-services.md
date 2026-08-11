---
status: accepted
---

# Use Tokio with owned I/O services

PlasmaTop will use a Tokio current-thread runtime for daemon orchestration, bounded blocking work for unavoidable synchronous readers and expensive rendering, typed command and D-Bus service tasks for shared resources, zbus for persistent system/session bus connections and desktop notifications, and nonblocking inotify through the existing nix dependency. Rust 1.97.1 is the project's exact current toolchain, allowing the current zbus 5 series with default features disabled and Tokio integration enabled, plus edition-2024 if-let chains where they clarify equivalent conditions.

This shape was chosen over a multithreaded runtime, scoped-thread collection, async trait objects, and a generic actor framework because the target is a low-power desktop daemon with few concurrent boundaries, cohesive state owners, and strict deterministic tests. Pure parsing, metric arithmetic, formatting, and scheduler decisions remain synchronous; only orchestration and waiting boundaries are async.

## Consequences

- Trivial CLI commands remain synchronous; daemon, probe, render, and timed profiling create a runtime only when needed.
- External commands use bounded concurrency, process-group cancellation, concurrent capped output draining, and a 1 MiB combined output limit.
- Required file watches fail clearly rather than silently falling back to polling.
- The implementation adds no notify, tokio-util, async-trait, Rayon, tracing, or generic actor dependency without later evidence.
