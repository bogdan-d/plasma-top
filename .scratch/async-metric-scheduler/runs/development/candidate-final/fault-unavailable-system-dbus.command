case=unavailable-system-dbus
scenario=main
stimuli=disabled
command=DBUS_SYSTEM_BUS_ADDRESS=unix:path=/tmp/plasma-top-profile-no-system-bus-1522868 /usr/bin/time -v -o .scratch/async-metric-scheduler/runs/development/candidate-final/fault-unavailable-system-dbus.time /var/mnt/xdata/code/_self/plasma-top/.scratch/async-metric-scheduler/runs/development/binaries/plasma-top-candidate-final-d2b357d-diff-d34225bdacf7 profiling --config /var/mnt/xdata/code/_self/plasma-top/.scratch/async-metric-scheduler/runs/development/fixtures/connections.toml --duration 4.2 --scenario main
started_at=2026-08-12T18:02:40+03:00
exit_status=0
finished_at=2026-08-12T18:02:44+03:00
