---
status: accepted
---

# Assemble displays from independently scheduled samples

PlasmaTop will replace its serial collection pass with demand-driven sampling groups that own their mutable counter, history, and retry state. Each completed metric sample carries its own capture time, while a scheduled display publication assembles the latest completed samples without waiting for a collection barrier; late or failed reads carry forward the latest valid sample, confirmed hardware removal invalidates it, and obsolete generations never commit.

This model was chosen over concurrent barrier collection and completion-driven publication because slow commands and hardware I/O must not delay panel deadlines, while publishing every completion would multiply daemon writes, Plasma `cat` processes, Qt RichText work, and wakeups. Stateful groups never overlap themselves, missed ticks are skipped rather than replayed, notifications evaluate completed samples after first paint, and rendering continues to consume a flattened `DisplaySnapshot`.

## Consequences

- Metric freshness is defined per sample rather than by one collection timestamp.
- Panel, notification, configured graph-history, presented-tooltip, and selected-page demand determine which jobs stay scheduled.
- Tooltip-only work stops while hidden except graph histories explicitly configured to remain warm.
- Profiling must report queue time, run time, sample age, and publication lateness rather than summing one serial pass.
