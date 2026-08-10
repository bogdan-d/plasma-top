# Build pure scheduler

Type: task
Status: resolved
Blocked by: 02

## Objective

Implement deterministic scheduling policy without Tokio or real I/O.

## Scope

- Accept typed clock, config, hardware inventory, demand, presentation, completion, failure, and lifecycle events.
- Emit typed start, cancel, publish, notify, rescan, and backoff actions.
- Implement freshness budgets, 50 ms prefetch, skip-missed-tick behavior, per-job non-overlap, bounded backoff, generation/source rejection, history deadlines, first-paint deadline, activation refresh, and 1 s deactivation grace.
- Model configured graph history as hidden demand and CPU-core history as presented-page demand.
- Use bounded/coalesced queue semantics without implementing a reusable actor framework.
- Replace the temporary synchronous cadence coordinator with this scheduler while executing emitted I/O actions serially, so the production path exercises the policy before async dispatch lands.

## Acceptance

- Pure tests cover due ordering, same-owner non-overlap, missed ticks, carried samples, invalidation, config generations, signal coalescing, failure backoff, visibility transitions, first paint, publication deadlines, notification arming, and suspend/resume actions.
- Tests use explicit fake time and no sleeping/runtime.
- Production retains one serial action executor with unchanged visible behavior; no unused parallel scheduler path exists.

## Validation

Run focused scheduler tests, then the full Rust gates from `docs/DEVELOPMENT.md`.

## Answer

Implemented in `a9cbec8` (`feat: add deterministic metric scheduler`). PlasmaTop now uses one pure typed scheduler for demand, deadlines, backoff, history, generations, cancellation, publication, notification, inventory, and lifecycle policy, with a bounded non-recursive serial production executor and deterministic fake-time coverage. Full repository validation and a fresh integration acceptance review passed.
