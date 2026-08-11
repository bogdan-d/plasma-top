# Add profiling and validate development host

Type: task
Status: ready-for-agent
Blocked by: 06, 07

## Objective

Make scheduler latency, work counts, and freshness measurable, then compare the async candidate against the fixed synchronous baseline on the development host.

## Scope

- Preserve one-shot cold/warm profiling and add `--duration` plus `--scenario hidden|main|<page>` with `main` default.
- Report per-job attempt/outcome counts, process/D-Bus counts, queue/run p50/p95/p99, maximum sample age, missed deadlines, render/write time, and publication lateness without runtime-file writes.
- Run release A/B scenarios under normal execution and constrained CPU affinity; record host, kernel, config, commands, pages, services, and commit IDs.
- Promote measured event-thread budget violators to the blocking lane and fix deadline regressions before laptop validation.

## Acceptance

- Profiling detail has no steady production cost outside profiling mode.
- Development-host A/B report distinguishes hidden, main, expensive-page, failure, and timeout behavior.
- Provisional deadline SLOs pass or deviations are documented as blockers.

## Validation

Run profiling CLI tests, release scenario scripts, and the full gates from `docs/DEVELOPMENT.md`.
