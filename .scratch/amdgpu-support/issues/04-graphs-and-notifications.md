# Add AMDGPU graphs and temperature notifications

Type: task
Status: ready-for-agent
Blocked by: 03

## Objective

Complete AMDGPU graph-history and independent edge-temperature alert behavior without changing existing vendor semantics.

## Work

- Extend `src/sensors/gpu_history.rs`, catalog history selection, scheduled history sampling, and `src/render/pages.rs` with AMD usage and codec histories.
- Preserve one GPU graph source and implement exact precedence NVIDIA, then AMDGPU, then Intel. Use `amd:<canonical PCI identity>` as the history source and reset history when that identity or preferred vendor changes.
- Label the secondary AMD history as codec activity rather than decoder activity.
- Add independent `gpu_amd_temp` enable/threshold configuration, notification reading, latch state, localized message, and evaluation in `src/notify.rs` and its child tests.
- Reuse existing temperature sustain and hysteresis settings. Default the AMD alert to 80°C and disabled.
- Ensure mixed NVIDIA/AMD systems can independently arm, trip, clear, and emit both temperature alerts; never alias AMD state to the legacy NVIDIA `gpu_temp` reading.

## Focused checks

- Graph selection covers no GPU, each single vendor, NVIDIA+AMD, AMD+Intel, and all three vendors.
- History resets on source identity/vendor changes and does not combine samples from different devices.
- AMD codec history uses the chosen terminology and threshold.
- AMD and NVIDIA temperature alerts have separate config, retained notification inputs, sustain timers, hysteresis, messages, and failure handling.
- Cached or failed AMD temperature attempts do not notify; a newly captured sample does.

## Done when

The graph page follows the confirmed precedence and AMD edge-temperature alerts work independently with all focused graph and notification tests passing.
