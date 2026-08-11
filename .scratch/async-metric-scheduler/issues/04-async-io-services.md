# Add async I/O services

Type: task
Status: claimed
Blocked by: 02

## Objective

Introduce bounded async command and D-Bus boundaries while preserving deterministic fakes.

## Scope

- Raise the project minimum Rust version to 1.87 across Cargo, development docs, packaging, and CI assumptions.
- Add Tokio with only required features and zbus with defaults disabled plus Tokio integration; update `DEPENDENCIES.md` and lockfile without unrelated upgrades.
- Build command-specific runtimes and route current scheduler actions through the async shell while preserving serial owner dispatch in this ticket.
- Add typed bounded command requests with at most two child processes, process groups, timeout/shutdown cancellation, concurrent stdout/stderr drain, 1 MiB retained-output cap, and truncation metadata.
- Replace flattened D-Bus strings with typed UPower/UDisks request/reply structures and persistent bounded zbus system/session services.
- Move desktop notifications to typed session D-Bus while preserving latch transition on delivery failure.
- Add UPower and logind signal streams with typed coalesced events.
- Remove `wait-timeout`, `signal-hook`, and `serde_json` only after their last production/test use disappears.

## Acceptance

- Process tests prove timeout, process-group descendant cleanup, status mapping, output draining, output truncation, and shutdown.
- Typed fake services cover all sensor and notification paths without a real bus.
- Disconnect/reconnect/backoff does not kill service tasks or storm requests.
- Production uses the new services; no unused sync/async dual boundary remains.
- Production remains `unsafe`-free.

## Validation

Run focused adapter tests, `cargo tree` dependency review, and the full Rust gates from `docs/DEVELOPMENT.md`.
