# Add profiling and validate development host

Type: task
Status: resolved
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

## Answer

PlasmaTop now preserves one-shot cold/warm profiling and adds scheduler-aware timed profiling for hidden, main-tooltip, and configured-page demand without writing daemon runtime files. Profiling-only reports cover per-job dispositions, process and D-Bus calls, queue/run percentiles, retained source-sample age, wake and publication lateness, render timing, first paint, skipped deadlines, and bounded shutdown. The explicit `--stimuli` mode separately measures correlated page, presentation-control, and config events through the real watcher, reload, scheduler, and in-memory publication paths without contaminating steady scenarios.

Fresh release validation on the development host covered hidden, main, graphs, and processes in normal and CPU-0-affinity modes, plus isolated unavailable-D-Bus and five-second command-timeout cases. The worst sampled first paint was 106.683 ms, scheduled publication lateness was 3.317 ms, page/control/config latency was 51.885/52.330/56.036 ms, and shutdown was 8.287 ms. No display deadline was skipped, no provisional SLO was exceeded, no runtime/config mutation or process leak occurred, and no measured event-thread violation justified moving more work to the blocking lane. Full repository validation passed; raw commands, outputs, identities, and limitations are recorded in `../development-validation.md` and `../runs/development/candidate-final/`. Laptop power acceptance remains issue 09.
