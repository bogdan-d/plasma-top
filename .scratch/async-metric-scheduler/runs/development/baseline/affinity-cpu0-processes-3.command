mode=affinity-cpu0 scenario=processes repeat=3
working_directory=/tmp/plasma-top-sync-baseline.tmQCij
command=/usr/bin/time -v -o /var/mnt/xdata/code/_self/plasma-top/.scratch/async-metric-scheduler/runs/development/baseline/affinity-cpu0-processes-3.time taskset -c 0 /var/mnt/xdata/code/_self/plasma-top/.scratch/async-metric-scheduler/runs/development/binaries/plasma-top-baseline-7c95731 render --config /tmp/plasma-top-sync-baseline.tmQCij/config/config.toml --component tooltip --format text --page processes 
2026-08-12T16:50:41+03:00
exit_status=0
