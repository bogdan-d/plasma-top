case=process-syscall-sampling
command=strace -f -c -e trace=process -o /var/mnt/xdata/code/_self/plasma-top/.scratch/async-metric-scheduler/runs/development/candidate/main-process-trace.strace /var/mnt/xdata/code/_self/plasma-top/.scratch/async-metric-scheduler/runs/development/binaries/plasma-top-candidate-d2b357d-diff-9d492f94 profiling --config /var/mnt/xdata/code/_self/plasma-top/config/config.toml --duration 4.2 --scenario main
limitation=strace perturbs timing; this run is process evidence only and excluded from SLO/resource summaries
exit_status=0
