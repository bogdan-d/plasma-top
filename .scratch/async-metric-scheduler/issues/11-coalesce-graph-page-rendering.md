# Coalesce graph page rendering

Type: task
Status: resolved
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

## Answer

`PageRender` attempts were real dispatches rather than duplicated accounting: accepted and rejected completions had executed graph PNG rasterization, while cancellation could still remove queued work before execution. The churn came from advancing one broad render generation and requesting a render after every accepted non-page completion. Rendering now invalidates only for graph-consumed input, relevant inventory/config/style/width changes, and explicit graph-input removal; publication coalesces that dirty state into one in-flight render and at most one newest-input follow-up.

Activation preserves a truthful retained graph until its fresh replacement, page changes retain placeholder semantics, config page reordering reconciles selected demand, and cancellation clears only the matching run reservation. Focused tests cover source bursts, obsolete style/input rejection, graph-input invalidation, source replacement, deactivation grace, shutdown, reload page reordering, rollover, and unrelated completions.

Three normal and three CPU-0 release profiles reduced attempts from 36–37 to 8 normal and 32–37 to 7 CPU-0. Rejections fell from 19–20 to 3 normal and 11–14 to 2 CPU-0; each final run had one shutdown cancellation. User CPU and context switches improved in every mode, render time remained neutral or improved, worst first paint was 104.429 ms, publication p99 2.159 ms, shutdown 5.049 ms, and no deadline was skipped. Six explicit-stimulus runs also kept page/control/config event p99 below 100 ms. Full repository gates passed. Reproducible commands, hashes, raw reports, summaries, diagnosis, and host limitation are in `.scratch/async-metric-scheduler/runs/issue-11/`.
