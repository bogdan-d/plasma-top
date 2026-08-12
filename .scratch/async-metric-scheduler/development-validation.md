# Issue 08 development-host validation

Captured 2026-08-12 on the development host. This is fresh development-host scheduler and resource evidence, not laptop-power or plasmashell-layout evidence. The fixed synchronous baseline is retained as the closest honest boundary; it does not expose equivalent timed scheduler metrics, so this report does not claim an async-versus-sync performance delta.

## Identity and environment

The fresh candidate is the uncommitted worktree based at `d2b357d482a7c4138f6edbd36c700f34366f2ed7` (`docs: track plasmoidviewer repair`). Its tracked binary diff SHA-256 is `d34225bdacf74ae21523413ef8c08e76ae418f5975317bd79ac8f82b71ad940a`; the staged diff is empty. `candidate-final/worktree-tracked.diff` preserves that exact diff, and `candidate-final/candidate-input-files.sha256` fingerprints every modified or untracked source/config input not represented by the tracked diff. The release command was `cargo build --release --locked --features nvml`, using rustc `1.97.1 (8bab26f4f 2026-07-14) (Homebrew)` and cargo `1.97.1 (c980f4866 2026-06-30) (Homebrew)`.

The measured binary is `runs/development/binaries/plasma-top-candidate-final-d2b357d-diff-d34225bdacf7`, SHA-256 `6174bd27fe30a5a63f5dccf18b6bd2f00be052fea8fd8e6b0b5d4afc2ec4c8ea`. The config SHA-256 is `50260ac6747364493d1588250134bfda2fce9420ec6cc8a8994ba610efca6718`; the locked dependency hash is `28c05558909cb624261a1141ea1357ffd5e7d79bb1c0a3d5b60250a28dbf3b13`. `candidate-final/retained-binary-integrity.sha256` freshly verifies the retained fixed baseline binary (`ff5b09911406d146ef0b1bbfbcb4274852398f39e64f75f643c5ea789cebe71b`) and the historical mixed-work candidate binary (`f8f52c07a5bd5d8181044ec156fe48ff6031d67fec86d5f7dc959ccf1d60fd3c`); neither was rebuilt or replaced.

The fresh host capture in `candidate-final/host-and-services.txt` identifies `FW-DESK-BZ`, Bazzite Stable `F44.20260802`, Linux `7.1.5-ogc5.1.fc44.x86_64`, x86_64, and AMD RYZEN AI MAX+ 395 with 32 logical CPUs. The capture records the user/system D-Bus service availability, the 0–31 allowed affinity set, and available measurement tools. It records no power or energy data.

## Method and isolation

All fresh candidate reports use the preserved release binary and `config/config.toml`. The 24 steady runs use `/usr/bin/time -v -o RUN.time BIN profiling --config config/config.toml --duration 4.2 --scenario SCENARIO` with no `--stimuli`: `hidden`, `main`, `graphs`, and `processes`, three normal repeats and three `taskset -c 0` repeats each. `candidate-final/steady-assertions.txt` verifies every report says `stimuli: disabled`, contains no `stimulus_` record, and ends `final_status: ok`.

The six explicit-stimulus runs use `--duration 8.2 --scenario main --stimuli`, three normal and three CPU-0 repeats. This duration completed the serial dismiss, present, alternate-page, restore-page, alternate-config, and restore-config sequence in every run. `candidate-final/stimulus-assertions.txt` verifies `planned=6 started=6 completed=6 timed_out=0 not_started=0`, an enabled/explicit aggregate-work label, and `final_status: ok` for each report. Requested and watcher/scheduler-observed terminal state matched in all six reports: presented main, page index 0, config generation 3, and `overlay=false`.

The real runtime root was snapshotted before and after each steady, stimulus, and fault group. The copied fixture/config hashes and `/tmp/plasma-top-profile-*` directory list were also compared before and after each group. Every comparison passed: no real runtime/config/fixture mutation and no profile-temp leak. Timed profiling writes only its disposable root and suppresses runtime HTML writes.

The two fault runs use the existing isolated `fixtures/connections.toml`: unavailable system D-Bus sets only the child `DBUS_SYSTEM_BUS_ADDRESS` to a nonexistent UNIX socket; the timeout case prepends only the existing `timeout-bin/ss` wrapper, which runs `sleep 6`, to the child PATH. Both omit `--stimuli`. No host service was stopped or reconfigured. The timeout wrapper left no `sleep 6` process after completion.

## Fresh steady results

The table gives the median and worst per-run p99 across three repeats. These are steady-only results; no row includes injected page, presentation, or config workload.

| Mode | Scenario | First-paint p99 median/worst (ms) | Publication p99 median/worst (ms) | Worst wake p99 (ms) | Worst shutdown (ms) | Skipped / first-paint >250 ms / publication >50 ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| Normal | hidden | 9.498 / 9.604 | 1.450 / 1.648 | 1.961 | 3.707 | 0 / 0 / 0 |
| Normal | main | 104.893 / 105.165 | 1.673 / 1.701 | 1.901 | 6.409 | 0 / 0 / 0 |
| Normal | graphs | 103.911 / 103.986 | 1.687 / 2.244 | 1.937 | 5.972 | 0 / 0 / 0 |
| Normal | processes | 105.047 / 105.218 | 1.542 / 1.720 | 1.742 | 6.644 | 0 / 0 / 0 |
| CPU 0 | hidden | 9.590 / 9.769 | 2.150 / 3.317 | 3.076 | 4.875 | 0 / 0 / 0 |
| CPU 0 | main | 104.701 / 104.868 | 2.091 / 2.131 | 1.914 | 2.673 | 0 / 0 / 0 |
| CPU 0 | graphs | 103.648 / 104.046 | 1.235 / 1.854 | 1.867 | 5.075 | 0 / 0 / 0 |
| CPU 0 | processes | 105.059 / 106.683 | 1.488 / 1.834 | 2.060 | 3.911 | 0 / 0 / 0 |

Across all 24 fresh steady reports, the worst first-paint p99 is 106.683 ms (CPU-0 processes), the worst scheduled-publication p99 is 3.317 ms (CPU-0 hidden), the worst timer wake p99 is 3.076 ms (CPU-0 hidden), and the worst isolated shutdown is 6.644 ms (normal processes). All are below the provisional 250 ms first-paint, 50 ms scheduled-publication, and 500 ms shutdown thresholds, with zero skipped display deadlines. Full per-run `/usr/bin/time -v` resource values, internal process/D-Bus counts, job dispositions, queue/run percentiles, sample age, render times, and raw reports are retained; `candidate-final/steady-summary.tsv` and `steady-scenario-summary.tsv` are derived only from those raw files.

The fixed baseline remains at `runs/development/baseline/` with its original source/binary identity and closest-boundary commands. Its one-shot hidden collection and intentionally warmed render commands are not comparable to the 4.2-second async owner-loop metrics; therefore this validation makes no relative throughput, CPU, RSS, or latency claim against it.

## Explicit stimulus results

Stimulus aggregate job/call/render counts are intentionally workload-contaminated and are not compared with the steady table. The latency boundary begins at the isolated protocol atomic rename and ends only at the correlated in-memory tooltip publication. The control dismiss action has no HTML publication, so it completes the serial protocol action but does not create an event-latency sample; each run therefore reports two page samples, one publishing control sample, and two config samples.

| Mode | Kind | Runs | Reported samples | Over 100 ms | Worst reported p95/p99 (ms) |
| --- | --- | ---: | ---: | ---: | ---: |
| Normal | Page | 3 | 6 | 0 | 51.679 / 51.679 |
| Normal | Control | 3 | 3 | 0 | 52.330 / 52.330 |
| Normal | Config | 3 | 6 | 0 | 56.036 / 56.036 |
| CPU 0 | Page | 3 | 6 | 0 | 51.885 / 51.885 |
| CPU 0 | Control | 3 | 3 | 0 | 51.917 / 51.917 |
| CPU 0 | Config | 3 | 6 | 0 | 55.290 / 55.290 |

The all-mode total is 12 page, 6 control, and 12 config reported samples, with zero over 100 ms; the worst reported p99 values are 51.885, 52.330, and 56.036 ms respectively. Every action completed, no action remained pending or timed out, and requested versus observed final state matched. The worst explicit-stimulus shutdown was 5.286 ms, with zero over 500 ms. Thus the development-host page/control/config p99 <=100 ms and shutdown <=500 ms provisional SLOs pass for this bounded sequence. `candidate-final/stimulus-summary.tsv` holds every report value and terminal state; `stimulus-kind-summary.tsv` supplies the count and worst-per-report aggregation without pretending that per-report percentiles are a pooled raw sample distribution.

## Fault results

| Isolated case | First-paint p99 (ms) | Publication p99 (ms) | Skipped / first-paint >250 ms / publication >50 ms | Shutdown (ms) | Outcome |
| --- | ---: | ---: | ---: | ---: | --- |
| Unavailable system D-Bus | 112.831 | 1.573 | 0 / 0 / 0 | 2.244 | `ok`; failed D-Bus discovery attempts remained bounded |
| Five-second `ss` timeout | 201.478 | 2.041 | 0 / 0 / 0 | 8.287 | `ok`; one page-command attempt failed at the intended five-second boundary and no wrapper sleep remained |

Both fault reports say `stimuli: disabled`, contain no stimulus record, and have `final_status: ok`. The timeout report retains cancellation/in-flight accounting at deadline shutdown, so it is evidence of bounded degradation and cleanup, not a claim that all work completed successfully. The publication SLO remains satisfied in both cases and no confirmed production defect or blocking deadline regression was found.

## Conclusions and limitations

Issue 08 development-host acceptance is satisfied: the corrected candidate has fresh separated steady hidden/main/graphs/processes evidence under normal and CPU-0 execution; explicit page/control/config evidence is separate and complete; unavailable-service and five-second timeout behavior are bounded and cleaned up; and the full repository gates passed. This conclusion does not satisfy issue 09 and does not claim laptop power, energy, battery life, plasmashell/QML layout cost, or a general benchmark result.

The prior `runs/development/candidate/` raw files remain historical mixed-work evidence from the automatic-stimulus build and must not be used as steady results. Three repeats and per-run percentile summaries are a small sample. CPU frequency, background load, live hardware, D-Bus availability, and selected command availability remain host-dependent. `pidstat` and `perf` were unavailable, so no independent process CPU, wakeup, or hardware-counter result is claimed.

## Reproduction index

`candidate-final/build.{command,log}`, `identity.txt`, `hashes.sha256`, `worktree-*.diff*`, `candidate-input-files.*`, `host-and-services.*`, and `retained-binary-integrity.sha256` identify the candidate, input, host, and retained binaries. `steady-commands.txt`, `stimulus-commands.txt`, `fault-commands.txt`, and every corresponding `*.command`, `*.out`, `*.err`, and `*.time` preserve exact raw invocations and output. `steady-summary.tsv`, `steady-scenario-summary.tsv`, `stimulus-summary.tsv`, `stimulus-kind-summary.tsv`, and `fault-summary.tsv` are derived summaries. `steady-assertions.txt`, `stimulus-assertions.txt`, and `fault-assertions.txt` record the no-stimulus, no-mutation, cleanup, and status assertions. The historical baseline, mixed-work candidate, fixtures, and environment remain in their original sibling directories.
