# Compare synchronous and async daemon runtime

Type: task
Status: resolved
Blocked by: 11, 12, 13

## Objective

Produce an honest process-level performance and resource comparison between the fixed synchronous daemon and the final async daemon on the development host before laptop power validation.

## Current gap

Issue 08 compared the synchronous commit's one-shot profiling/render boundaries with timed async scheduler profiling. Those commands have different lifetimes, warm-ups, publication behavior, and workloads, so `.scratch/async-metric-scheduler/development-validation.md` correctly makes no async-versus-sync CPU, RSS, context-switch, wakeup, or throughput claim. Equivalent timed scheduler instrumentation does not exist in the synchronous commit, but actual isolated daemon processes can still be compared at the user-visible scenario boundary.

## Fixed inputs

- Synchronous baseline commit: `7c95731b105d704ba7b78d8cb5deb7a19f9277bf`.
- Async candidate: the resolved issue-11, issue-12, and issue-13 branch head.
- Use release builds, identical relevant Cargo features, shipped assets, config content, host services, CPU affinity, environment, scenario duration, and measurement tooling.
- Build the synchronous commit in an isolated worktree or archive without changing the active checkout. Record source, lockfile, config, binary, toolchain, host, kernel, and harness hashes; do not commit built binaries.

## Scope

- Build a bounded harness that launches each real daemon with disposable XDG config, cache, data, home, and runtime roots and cleans up only processes and files it created.
- Measure matched startup and steady windows separately so process startup does not masquerade as steady runtime cost.
- Run hidden, main tooltip, graphs, processes, unavailable-service, and timeout scenarios with at least three normal and three CPU-0-affinity repetitions where the scenario is meaningful.
- Drive the async daemon through presentation leases and page protocol state. Drive the synchronous daemon through its supported page state; for hidden, explicitly record that the legacy daemon continues tooltip work because it has no presentation protocol. Compare user-visible scenarios rather than pretending the internal demand sets are identical.
- Verify both daemons remain healthy, publish truthful panel/tooltip output, enter the intended page/presentation state, terminate cleanly, and leave no child or temporary state behind.
- Record normalized wall duration, user/system CPU time, peak and sampled RSS, voluntary/involuntary context switches, child-process attempts, successful children, publication/write counts or bytes, and available wakeup or hardware-counter evidence. Use installed tools and `/proc`; mark unavailable measurements rather than adding dependencies or estimating them.
- Preserve exact commands, minimal raw outputs, derived tables, environmental limitations, and protocol differences under a scoped `.scratch/async-metric-scheduler/` evidence directory. Do not duplicate issue-08 binaries or historical raw data.
- Attribute material deltas to demand scoping, cadence, command/D-Bus behavior, rendering/publication, polling removal, or harness limitations where evidence permits. Do not infer energy from CPU or wakeups.
- Fix confirmed async deadline or resource regressions before resolving this ticket. Record non-material or unexplained noise honestly instead of tuning from a small sample.

## Acceptance

- Every compared row uses equal process duration and equivalent user-visible state, with unavoidable protocol/workload differences stated beside the result.
- Hidden, main, graphs, and processes have repeated normal and constrained-affinity CPU, RSS, context-switch, child-process, and publication evidence for both daemons.
- Safe unavailable-service and timeout cases demonstrate bounded failure behavior without modifying host services or leaking descendants.
- The report distinguishes startup cost, steady rates, and one-time work and does not compare the issue-08 one-shot baseline directly with timed async profiling.
- Any meaningful async regression is either fixed and remeasured or documented as a blocker for issue 09.
- The result provides a reproducible development-host resource baseline for issue 09 without claiming laptop energy or power acceptance.

## Validation

Run focused harness tests, `shellcheck` and `shfmt` for any changed shell scripts, representative short safety runs, the complete normal and CPU-0 matrix, artifact-integrity checks, and the full repository gates from `docs/DEVELOPMENT.md`. Do not restart plasmashell or use the disabled plasmoidviewer workflow.

## Answer

Compared fixed baseline `7c95731b105d704ba7b78d8cb5deb7a19f9277bf` with async candidate `ae53de61ac9e29650cebc7e77da8a6cce2350c4a` using isolated release daemons, matched assets/configuration, three normal and three CPU-0 repetitions, and separate traced and untraced matrices. All 120 runs remained healthy, published valid intended output, exited cleanly, and left no harness-owned children or state.

The async daemon is not generally faster: whole-process CPU was equal in normal hidden/main and usually 0.01 seconds higher per eight-second run for graphs, processes, and CPU-0 groups. Whole-process peak RSS remained within 0.8 MiB. Its clear improvement is hidden demand scoping, reducing steady publications from 10 to 1–3 while the synchronous daemon necessarily continues tooltip work. No development-host blocker remains for issue 09, but laptop energy, wakeups, and hardware counters remain unavailable and must be measured there. Commands, hashes, raw evidence, tables, protocol limits, and the concise conclusion are in `.scratch/async-metric-scheduler/issue-14/REPORT.md` and the two scoped evidence directories.
