# Add AMDGPU graphs and temperature notifications

Type: task
Status: resolved
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

## Completion evidence

Implemented NVIDIA, AMDGPU, then Intel graph-source precedence, with AMD history keyed by `amd:<canonical PCI identity>`. The serial executor selects each retained AMD graph field at the history deadline; async execution retains source-keyed usage/codec points and excludes later samples from earlier deadlines. Codec history survives transient sibling failures, stops appending on confirmed codec absence, and resets with usage history when the selected device or vendor changes. Slow AMD work does not overwrite or invalidate fast graph points. GPU rendering shares one chart path and uses the configured AMD codec label and activity threshold.

Added the independent AMD temperature latch and evaluator using the existing disabled-by-default enable flag, 80°C threshold, localized AMD temperature label, and shared sustain/hysteresis settings. Only newly captured AMD temperatures reach evaluation; cached values and failed temperature attempts remain display-only. NVIDIA and AMD alert state, thresholds, messages, and delivery remain independent.

Ten new tests cover all eight vendor-presence combinations, history identity/vendor reset, independent deadline selection and partial failures, async history commits and slow-job isolation, future-sample exclusion, codec legend/threshold behavior, independent alert timers/hysteresis/delivery, and fresh-temperature-only evaluation. Updated `docs/ITEMS.md` with the implemented behavior.

Validation passed with Rust 1.97.1: locked dependency fetch and unchanged Cargo.lock, fmt, all-target/all-feature check and Clippy with warnings denied, all-target/all-feature tests, rustdoc, repository gate, diff whitespace check, shell syntax checks, disposable user-install checks, and native package-layout/upgrade/uninstall checks. The full suite passed 880 tests, comprising 849 unit and 31 integration tests. The Qt render matrix passed for dark, light, and overlay themes; its contact sheet and separate dark/light AMD graph fixtures were visually inspected. Artifacts remain under `.test-artifacts/plasma/qt/` and `.test-artifacts/plasma/amd04/`. Live Strix Halo comparison, profiling, and live applet evidence remain ticket 05.
