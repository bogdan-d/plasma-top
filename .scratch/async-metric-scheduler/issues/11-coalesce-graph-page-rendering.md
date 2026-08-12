# Coalesce graph page rendering

Type: task
Status: ready-for-agent
Blocked by: 08

## Objective

Determine why the selected graphs page starts many render jobs that become obsolete, then remove confirmed redundant work without weakening first-paint, activation-refresh, or display-deadline behavior.

## Evidence

Issue-08 steady profiling ran each graphs scenario for 4.2 seconds with a 1.5-second display cadence. The three normal runs started 44–45 `PageRender` attempts each, accepted 15–17, rejected 24–27 as obsolete, and cancelled 3. The three CPU-0 runs started 36–37 attempts, accepted 20–22, rejected 12–15, and cancelled 2. Individual renders were inexpensive and no publication SLO was missed, but the attempt volume may waste CPU and wakeups on expensive-page workloads.

Raw reports are `.scratch/async-metric-scheduler/runs/development/candidate-final/{normal,affinity-cpu0}-graphs-*.out`; methodology and host limitations are in `.scratch/async-metric-scheduler/development-validation.md`.

## Scope

- Reproduce the attempt/disposition pattern with deterministic instrumentation or tests and verify that it represents real dispatched rasterization rather than profiling-accounting duplication.
- Trace every `PageRender` trigger through graph-source completion, snapshot generation, publication, pending-follow-up coalescing, and obsolete-result rejection.
- Distinguish necessary startup/activation rendering from work superseded before it can be published.
- If churn is confirmed, coalesce invalidations so one render is in flight with at most one useful pending follow-up for the latest render input; do not add a generic actor framework or new user setting.
- Preserve immediate retained tooltip content, the 100 ms activation/event target, display-cadence updates, page cancellation after deactivation grace, and generation/source rejection.
- Keep non-graph pages and hidden tooltip demand unchanged. Do not retune freshness budgets or the 50 ms file-watch debounce in this ticket.
- Repeat normal and constrained-affinity graphs profiling after the fix and record attempt/disposition, CPU/context-switch, render, first-paint, and publication evidence. Leave laptop energy conclusions to issue 09.

## Acceptance

- The source of every observed rejected/cancelled graph render class is explained and covered by focused deterministic tests.
- Graph input bursts dispatch no more than one in-flight render and one coalesced follow-up using the newest complete render input.
- After initial activation settles, render attempts track actual page refresh/publication demand rather than individual source completions, and obsolete attempts are eliminated or justified by an unavoidable boundary race.
- First paint remains at or below 250 ms, page/control HTML remains at or below 100 ms, scheduled publication remains no more than 50 ms late, and no display deadline is skipped in repeated development-host runs.
- Measured expensive-page work improves or stays neutral; no optimization claim is made from attempt counts alone.

## Validation

Run focused scheduler/async-loop/page-render tests, timed `graphs` profiling in normal and CPU-0-affinity modes, and the full repository gates from `docs/DEVELOPMENT.md`.
