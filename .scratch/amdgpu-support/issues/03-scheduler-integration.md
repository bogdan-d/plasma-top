# Integrate AMDGPU scheduling and lifecycle

Type: task
Status: ready-for-agent
Blocked by: 02

## Objective

Connect AMDGPU inventory, demand, fast/slow jobs, collection, invalidation, and reconciliation to the production daemon while preserving scheduler contracts and source safety.

## Work

- Extend inventory-family reconciliation in `src/sensors/discovery.rs` with the selected AMDGPU source and capability identity established by tickets 01–02.
- Add granular AMD capabilities, an AMD inventory family, owner, source identity, and fast/slow job kinds through `src/sensors/catalog.rs`, `src/scheduler/model.rs`, `src/sensors/coordinator.rs`, attempts, and scheduler execution.
- Before adding behavior to warning-size files, extract cohesive GPU catalog construction from `src/sensors/catalog.rs` into `src/sensors/catalog/gpu.rs` and GPU scheduled execution/merge behavior from `src/sensors/scheduled.rs` into `src/sensors/scheduled/gpu.rs`. Keep public module interfaces small and prove the extraction behavior-neutral before adding AMD branches.
- Put AMD catalog tests in `src/sensors/catalog/tests/amd.rs`; do not grow the existing near-limit `src/sensors/catalog/tests.rs`.
- Build a fast job at `display.poll_interval` for demanded usage, codec, memory, and clock fields, and a slow periodic job with a 30-second freshness budget for demanded temperature, power, and fan fields. Each job reads only demanded fields in its group.
- Use source identity `amd:<canonical PCI identity>`, reject obsolete generations, prevent owner overlap, and reset AMD retained state when config capability or confirmed inventory disappears or the selected identity changes.
- Include AMD inventory reconciliation at the existing 60-second hardware cadence only when AMD item, graph, or notification demand exists. Startup failure becomes initially absent; later failure retains prior inventory.
- Merge retained AMD samples into `DisplaySnapshot`, but expose notification candidates only from newly captured temperature samples.

## Focused checks

- AMD items demand only AMD inventory and the correct fast or slow job; unsupported capabilities create no sampling job.
- Tooltip presentation, hidden panel demand, default graph history, and notification-only demand pay only for their required fields.
- Fast/slow cadence, failure backoff, no-overlap behavior, stale-generation rejection, and profiling dispositions match existing scheduler contracts.
- Confirmed remove/re-add and selected-device change invalidate stale values; discovery failure and field read failure retain valid values.
- One failed field does not discard successful sibling captures from the same grouped job.
- Existing NVIDIA and Intel catalog/scheduler tests remain unchanged and pass.

## Done when

Production startup, reload, reconciliation, collection, and publication can populate all supported AMD readings without increasing any handwritten file beyond 1,000 lines or leaving touched warning-size files unsplit.
