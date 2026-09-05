# Integrate AMDGPU scheduling and lifecycle

Type: task
Status: resolved
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

## Completion evidence

Implemented demand-gated AMD inventory reconciliation, one fast and one 30-second slow job sharing the AMD owner, and serial/async display publication for all seven readings. Job tickets carry the effective field demand, so hidden work excludes tooltip-only fields; tooltip activation refreshes newly demanded fields even when the group was already running. Job definitions include selected source paths and demand, with `amd:<PCI identity>` job identity, generation rejection, and group-specific invalidation on replacement or removal. Failed reads retain valid samples, successful siblings still publish, and only freshly captured temperature produces a notification candidate. Profiling uses retained capture times for the ticket's demanded fields.

Extracted GPU catalog and scheduled execution before adding AMD behavior; the unchanged sensor suite passed 323 tests after extraction. Split the other touched warning-size files by state, dispatch/publication, attempts, and test responsibility. Every changed Rust file is below 800 lines.

Eight new tests cover per-item capability/demand/cadence, hidden and tooltip field subsets, owner serialization, activation refresh, graph and notification demand, backoff and obsolete generations, partial failures, notification freshness, source/config removal and re-addition, and production async discovery through publication of all seven fixture readings. Existing NVIDIA and Intel tests pass. Graph history publication and temperature-alert evaluation remain ticket 04; live Strix Halo comparison and profiling evidence remain ticket 05.

Validation passed with Rust 1.97.1: locked dependency fetch, unchanged Cargo.lock, fmt, all-target/all-feature check and Clippy with warnings denied, all-target/all-feature tests, rustdoc, repository gate, diff whitespace check, shell syntax checks, disposable user-install checks, and native package-layout/upgrade/uninstall checks. The full suite passed 870 tests, comprising 839 unit and 31 integration tests. Full tests ran outside the sandbox for SIGTERM shutdown; the package gate ran outside the sandbox for dependency downloads. No render, CSS, or QML files changed.
