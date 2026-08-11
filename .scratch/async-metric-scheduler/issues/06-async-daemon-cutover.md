# Cut daemon orchestration over to async owners

Type: task
Status: resolved
Blocked by: 03, 04, 05

## Objective

Enable independent owner dispatch and deadline publication in the Tokio shell after the scheduler and async boundaries already run in production.

## Scope

- Wire bounded owner/service channels, critical-task supervision, one bounded blocking lane, and measured inline/offload classification.
- Implement progressive hardware discovery, 200 ms first-paint wait, scheduled display assembly, immediate post-first-paint notification arming, completion-driven notifications, unchanged-byte suppression, and bounded runtime shutdown.
- Preserve last-good config reload, unknown/misplaced-item warnings, canonical-width coverage, page state, atomic publication, and runtime-root contracts.
- Implement suspend/resume pause, rate-baseline invalidation, volatile discovery, and immediate demanded refresh from logind events.
- Remove the temporary synchronous coordinator so one production orchestration path remains.

## Acceptance

- No sensor wait lies on scheduled publication critical path.
- Stateful owners never overlap or commit out of order.
- First-paint, publication, event, and shutdown SLOs are observable and pass deterministic/fault-injection tests.
- Existing render, notification, sensor, reload, page, and daemon fixture behavior remains compatible except explicitly changed semantics in the spec.

## Validation

Run focused paused-time shell tests and the full gates from `docs/DEVELOPMENT.md`.

## Answer

Production daemon orchestration now runs on the owned current-thread Tokio shell with bounded per-owner channels, a bounded completion queue, coalesced pending work, one semaphore-bounded blocking lane, transactional completion commits, progressive discovery, deadline-first publication, independent panel and tooltip surfaces, completion-driven notifications, correlated suspend/resume reconciliation, and bounded critical-task shutdown. The synchronous coordinator remains test-only.

Focused async-loop, scheduler, catalog, shell, CLI daemon, and legacy production-executor validation passed. Every gate in `docs/DEVELOPMENT.md` passed with Homebrew Rust 1.97.1; because the required dependency update intentionally leaves `Cargo.lock` uncommitted, lock validation compared its SHA-256 before and after `cargo fetch --locked` and confirmed no mutation.
