# Issue 14 process comparison harness

This harness compares real release daemon processes from the fixed synchronous baseline and async candidate. It re-execs once as a `setsid` session leader, and every daemon, CLI helper, command process group, legacy wrapper sleep, and sampler stays in that one owned session. Every repetition gets a distinct directory under one `mktemp` root containing disposable `HOME`, XDG roots, `TMPDIR`, and PlasmaTop runtime state. Observation recursively reads every task's `children` file. Normal completion and trap cleanup repeatedly rescan and identity-check PID start time, session, and process group while signaling newly observed members until the session is empty or a bounded deadline expires; nothing outside the owned session or the harness leader is targeted, and the root is preserved on failure. Leak detection is recorded before cleanup and cleanup still runs after a failed check.

The baseline has no `present`/`dismiss` CLI or presentation lease directory. Its hidden row therefore means the same user-visible hidden tooltip state while the synchronous daemon continues tooltip work. Candidate presented leases are created before daemon launch. Candidate hidden startup-to-initial-intended-state readiness explicitly includes the implementation-specific `present`, tooltip-prime, and `dismiss` actions; the subsequent dismissal grace is setup, before steady measurement. The current harness records epoch and monotonic boundaries immediately before spawn and uses a synthetic zero-cumulative snapshot for the fresh process, so startup wall and CPU are launch-inclusive. It starts `/proc` sampling immediately after discovering the daemon PID; sampled RSS can miss only the pre-discovery interval, while GNU time's instrumented process-tree peak RSS covers the full lifetime. Page selection, command completion, and all other settling are excluded setup. The exact steady deadline snapshot is captured before post-window health/output validation.

The two retained evidence directories predate that launch-boundary correction. Their startup window begins only after daemon PID capture and the initial snapshot, approximately 55–59 ms after exec in the reviewed traces, so it is post-PID readiness and not full exec-inclusive startup. Candidate hidden priming is included in that retained startup window. These old measurements are disclosed as captured and are not rewritten to simulate launch-inclusive evidence.

Unavailable-service runs use safe implementation-specific injection while preserving the same user-visible unavailable system service. The baseline invokes `busctl --system`, which ignores `DBUS_SYSTEM_BUS_ADDRESS`, so only baseline runs prepend a harness-owned `busctl` wrapper to `PATH`. It logs each attempt's exact shell-quoted argv, monotonic timestamp, and PID, exits immediately with status 69, never calls the real bus, and is retained as raw evidence; at least one logged `--system` attempt is mandatory and normal process tracing counts the wrapper exec. Candidate runs retain a child-only `DBUS_SYSTEM_BUS_ADDRESS` naming a nonexistent socket under the owned root and bounded `strace` connect evidence proving the failed operation. Baseline neither receives the nonexistent address nor requires a socket connect. This internal protocol difference is recorded in each result and the manifest rather than falsely claiming identical transports.

Timeout runs prepend a harness-owned `PATH` directory containing the configured command wrapper. The wrapper receives its log path and delay through the environment, timestamps start, and `exec`s its sleep; the configured delay must be at least six seconds, so identity-observed disappearance at the required 4.5–5.7 seconds proves termination before natural exit. A visible `ss:` timeout/terminated/unavailable detail is accepted when published. Focused runs showed that both fixed implementations instead feed `ss: timed out after 5.000s` through the connections parser and visibly render only `- … :after`; some candidate publications show no failure detail. For these exact protocol limitations the harness requires direct wrapper evidence and a fresh healthy intended-page publication, preserves the post-attempt HTML, and records the implementation-specific limitation instead of treating the title as failure proof. No synthetic reload is used. No host service or command is modified.

Each copied config is transformed with a bounded TOML-section edit that changes every existing `[notifications]` assignment to `false`; source configs are untouched and source/copy hashes are retained. Both implementations receive the same transformed content for a scenario. Assets remain source-version-specific and have separate relative-path manifests rather than a false cross-version equality assertion. Exact clean Git commits, relative source manifests, toolchain/build declaration, binary hashes, and config hashes are mandatory.

At evidence initialization, each selected verified input binary is hashed, copied to `binaries/<label>-plasma-top` inside the new evidence directory, made read/execute-only with mode 0555, and checked against a second input hash. The manifest records the original input path/hash and frozen relative path/hash. Every daemon and control CLI command uses only that frozen copy while source roots and asset roots remain the exact selected commits. The harness checks each frozen hash again after the matrix and aborts if it changed, so rebuilding, touching, or replacing an original input after initialization cannot alter a run.

The retained captures predate binary freezing and contain no binaries. In particular, the retained resource matrix executed unfrozen source-tree paths; its post-run hashes match the manifest, but that does not retroactively freeze the executed inputs. Its archived raw directory is reduced and must not be filled from another run. Future resource captures retain each run's transformed config, final panel/tooltip HTML, samples, and identities in addition to command, GNU-time, state, and output files.

Issue 14 uses two complementary matrices. The existing matched `strace` matrix remains authoritative for child counts, failed-exec/connect fault proof, and unavailable-service/timeout behavior. Because ptrace perturbs CPU time and context switches, the resource matrix uses `--resource-only`, launches the daemon directly under `/usr/bin/time`, and permits only hidden, main, graphs, and processes. It retains the same lifecycle, protocol, config, publication, `/proc`, state, session-safety, and cleanup checks, but writes `NA:untraced_resource_run` for child-related metric columns and creates no `process.strace`. Resource results must not be used for child or fault claims, and traced results must not be presented as unperturbed CPU/context-switch evidence. Raw `/proc/<pid>/status` context-switch counters cover only the daemon thread-group leader, so derived columns are named `leader_thread_*` and are not process-comparable; GNU time's instrumented whole-process/tree context switches remain the valid process-level evidence.

`metrics.tsv` separates startup, excluded setup/settle, and steady `/proc` deltas for daemon user/system ticks, sampled peak RSS, leader-thread context switches, and publication changes/bytes. `summarize.awk` takes the median of each repetition's `(user_ticks + system_ticks) / SC_CLK_TCK`; it does not add independent medians. In the traced matrix every daemon is run under matched `strace -f -qq -ttt -e trace=process`; unavailable-service adds `connect` to the same trace. The parser starts at the captured daemon PID, records all completed clone/clone3/fork/vfork edges including resumed calls, and computes their transitive closure independent of trace order. In each wall-clock window, `child_exec_attempts` is the number of distinct daemon-descendant PIDs with at least one `execve` or `execveat` attempt timestamp in the window, `successful_child_execs` is the number of distinct descendant PIDs with at least one successful exec timestamp in the window, and `child_exits_zero` is the number of distinct successfully-execed descendant PIDs with a zero exit timestamp in the window; a PID discovered only through a `CLONE_THREAD` edge is excluded from `child_exits_zero`. This excludes the strace, `/usr/bin/time`, `env`, `taskset`, and daemon launch chain. Raw traces are retained, and the process-tracing perturbation is disclosed. Publication polling can undercount multiple atomic renames inside one interval. Daemon exit zero is mandatory. State checks reject atomic temporaries and paths outside the explicit retained panel/tooltip/page/presentation allowlist before owned-root deletion. Wakeups, hardware counters, and energy remain unavailable rather than inferred.

Focused checks:

```bash
.scratch/async-metric-scheduler/issue-14/test-harness.sh
PLASMA_TOP_ISSUE14_SHORT_BIN="$PWD/target/release/plasma-top" .scratch/async-metric-scheduler/issue-14/test-harness.sh
```

Expected full matrix after archiving commit `7c95731b105d704ba7b78d8cb5deb7a19f9277bf` and building both roots with identical release features:

```bash
.scratch/async-metric-scheduler/issue-14/harness.sh \
    --baseline-root /path/to/baseline-7c95731b \
    --baseline-bin /path/to/baseline-7c95731b/target/release/plasma-top \
    --candidate-root "$PWD" \
    --candidate-bin "$PWD/target/release/plasma-top" \
    --features nvml \
    --duration 30 --repeat 3 \
    --output "$PWD/.scratch/async-metric-scheduler/issue-14/evidence-full"
```

The default matrix is 72 runs (2 implementations × 2 modes × 6 scenarios × 3 repeats), approximately 45–60 minutes including startup, page readiness, five-second timeout work, shutdown, and normal host variance. It intentionally does not run issue 08 profiling or the repository's full test/matrix gates.

Run the 48-run untraced resource matrix separately against the same roots and binaries. The intended resource window is exactly `--resource-only --duration 8 --repeat 3`:

```bash
.scratch/async-metric-scheduler/issue-14/harness.sh \
    --baseline-root /path/to/baseline-7c95731b \
    --baseline-bin /path/to/baseline-7c95731b/target/release/plasma-top \
    --candidate-root "$PWD" \
    --candidate-bin "$PWD/target/release/plasma-top" \
    --features nvml \
    --resource-only --duration 8 --repeat 3 \
    --output "$PWD/.scratch/async-metric-scheduler/issue-14/evidence-resource"
```
