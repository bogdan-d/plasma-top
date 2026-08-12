# Diagnose disk-I/O discovery failures

Type: task
Status: ready-for-agent
Blocked by: 08

## Objective

Explain the repeated root disk-I/O inventory failures observed during profiling and ensure stable unsupported or absent device topologies are not treated as transient boundary failures.

## Evidence

Every 4.2-second steady main, graphs, and processes profile dispatched six `Discovery:HardwareDiscovery:Inventory(DiskIo)` attempts and recorded six failures. Individual attempts were inexpensive at roughly 0.17–0.53 ms and bounded backoff protected publication deadlines, but the reports do not expose the typed cause and cannot distinguish an unsupported root-device mapping from a genuinely transient `/proc` or `/sys` failure.

Raw reports are `.scratch/async-metric-scheduler/runs/development/candidate-final/{normal,affinity-cpu0}-{main,graphs,processes}-*.out`; the measured host is Bazzite with its root filesystem topology recorded in `.scratch/async-metric-scheduler/runs/development/candidate-final/host-and-services.txt`.

## Scope

- Reproduce the development-host failure and capture its exact typed path/cause without flattening boundary errors into a generic absence.
- Trace root mount enumeration, mount-source parsing, mapper or layered-device resolution, and whole-disk sysfs lookup.
- Add deterministic fixtures for the reproduced topology plus ordinary block devices, `/dev/mapper` devices, stable unsupported sources, missing mount enumeration, and transient unreadable sysfs/procfs boundaries as applicable.
- Return confirmed absence for a successfully inspected but unsupported or unmappable stable topology; retain failure and bounded retry for genuine boundary errors.
- Preserve 60-second inventory reconciliation so a later supported identity can be adopted, and immediately schedule disk-I/O sampling when one appears.
- Add only the narrow diagnostic evidence needed to classify this family; do not turn normal profiling into an unbounded error log.

## Acceptance

- The six-failure development-host pattern has one documented root cause backed by a deterministic fixture or an explicitly recorded host-only limitation.
- Stable absence or unsupported topology does not enter exponential failure retry, while missing/unreadable required boundaries still do.
- Confirmed disk-I/O identity appearance, change, and removal preserve baseline invalidation and out-of-order generation safety.
- Repeated development-host profiling reports either a confirmed absence at the normal inventory cadence or a successful disk-I/O source, not an unexplained initial retry burst.
- Publication and shutdown SLOs remain satisfied under both absence and injected boundary failure.

## Validation

Run focused disk discovery, catalog, scheduler backoff, inventory reconciliation, and async-loop tests; repeat normal and CPU-0 `main` profiling; then run the full repository gates from `docs/DEVELOPMENT.md`.
