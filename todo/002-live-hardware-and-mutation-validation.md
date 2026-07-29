# Live hardware and mutation validation

## Status

Deferred until suitable hardware and permission to mutate host state exist. Fixture coverage is accepted; no known implementation defect exists.

## Coverage still missing

- Intel GPU discovery, utilization, frequency, cache, and reset behavior.
- NVIDIA NVML path and `nvidia-smi` fallback.
- System battery and UPower peripheral batteries.
- Bolt and HID receiver/device paths.
- Suspend/resume recovery.
- Default-route/interface switching.
- Disk and device hotplug.

Existing fixture tests cover success, absence, malformed data, failure, timeouts, clamps, caching, and reset behavior. Prior live testing used an AMD host without Intel/NVIDIA GPU, batteries, supported peripherals, or HID/Bolt devices; host mutation was intentionally forbidden.

## Relevant files

- `src/sensors/gpu_intel.rs`
- `src/sensors/gpu_nvidia.rs`
- `src/sensors/power.rs`
- `src/sensors/hid.rs`
- `src/sensors/net.rs`
- `src/sensors/disk.rs`
- `src/sensors/tests.rs`
- `tools/p6_live_matrix.sh`

## Handoff

1. Record host hardware, driver, kernel, Plasma, session, and permissions.
2. Run one hardware family per evidence session; do not claim unavailable paths.
3. Compare readings with trustworthy host tools and defined sensor formulas.
4. Exercise cache expiry, disappearance, reconnect, and recovery where safe.
5. For mutations, verify daemon survival, bounded logs, refreshed discovery, correct interface/device selection, and clean shutdown.
6. Add focused fixtures only for defects found; do not duplicate existing cases.

## Done when

- Each checklist item has reproducible live evidence or its own documented defect.
- Any fixes pass focused tests plus all gates in `docs/DEVELOPMENT.md`.
- Unsupported AMD GPU metrics remain out of scope unless separately requested.

