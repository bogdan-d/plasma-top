mode=affinity-cpu0 scenario=hidden repeat=2
command=/usr/bin/time -v -o .scratch/async-metric-scheduler/runs/development/candidate/affinity-cpu0-hidden-2.time taskset -c 0 /var/mnt/xdata/code/_self/plasma-top/.scratch/async-metric-scheduler/runs/development/binaries/plasma-top-candidate-d2b357d-diff-9d492f94 profiling --config /var/mnt/xdata/code/_self/plasma-top/config/config.toml --duration 4.2 --scenario hidden 
2026-08-12T16:48:56+03:00
exit_status=0
