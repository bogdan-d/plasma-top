# Issue 14 synchronous/async comparison

## Conclusion

The async daemon is not generally faster on this host. Across three normal and three CPU-0 eight-second repetitions, whole-process CPU was equal for hidden and main in normal mode and 0.01 seconds higher for most graph/process and CPU-0 groups. Whole-process peak RSS was effectively neutral: medians differed by less than 0.8 MiB. The async daemon's clear benefit is demand scoping: hidden steady publications fell from 10 to 1–3 per window because it stops tooltip work, while the synchronous daemon has no presentation protocol and continues that work.

Both daemons stayed healthy, published valid panel and tooltip output, entered every requested state, handled unavailable-service and five-second timeout cases within bounds, exited zero without forced termination, and left no owned children or temporary state. The comparison therefore found no blocker for laptop validation, but also no basis for claiming a general speedup.

## Median whole-process results

| Mode | Scenario | CPU baseline → async | Peak RSS baseline → async | Context switches baseline → async |
| --- | --- | ---: | ---: | ---: |
| normal | hidden | 0.02 → 0.02 s | 14.2 → 14.6 MiB | 148 → 175 |
| normal | main | 0.03 → 0.03 s | 14.6 → 14.6 MiB | 144 → 225 |
| normal | graphs | 0.04 → 0.05 s | 14.6 → 14.4 MiB | 140 → 285 |
| normal | processes | 0.08 → 0.09 s | 14.1 → 14.9 MiB | 142 → 252 |
| CPU 0 | hidden | 0.02 → 0.02 s | 14.9 → 14.5 MiB | 136 → 214 |
| CPU 0 | main | 0.02 → 0.03 s | 14.6 → 14.4 MiB | 137 → 275 |
| CPU 0 | graphs | 0.03 → 0.04 s | 14.9 → 14.7 MiB | 137 → 319 |
| CPU 0 | processes | 0.07 → 0.08 s | 14.5 → 14.6 MiB | 137 → 314 |

## Reproduction and limits

The baseline is `7c95731b105d704ba7b78d8cb5deb7a19f9277bf`; the async candidate is `ae53de61ac9e29650cebc7e77da8a6cce2350c4a`. Both were release builds with `--locked --features nvml`. `.scratch/async-metric-scheduler/issue-14/evidence-resource/` contains the untraced 48-run CPU/RSS matrix and `.scratch/async-metric-scheduler/issue-14/evidence-traced/` contains the 72-run child/fault matrix. Each manifest records exact commands, hashes, environment, protocol differences, and measurement scope; release binaries are intentionally not retained.

The traced matrix supports only child and fault claims because ptrace perturbs timing. `/proc/<pid>/status` context counts cover only the leader thread; the table uses GNU time's whole-process totals. Publication polling may undercount multiple writes between samples. `perf`, usable per-process hardware counters, scheduler wakeups, and energy measurements were unavailable. No energy conclusion is inferred; issue 09 still requires the real laptop.
