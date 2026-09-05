# Live hardware and mutation validation

## Status

AMDGPU Strix Halo sysfs acquisition, bounded workload response, and disposable daemon lifecycle were validated on 2026-09-05. See the [AMDGPU evidence report](../amdgpu-support/validation.md). Bogdan installed the candidate and supplied a live tooltip screenshot showing all six supported AMD rows without clipping and with fan correctly absent. In response to the hover/pinning, graph-paging, and theme-switching follow-up, Bogdan confirmed normal operation of the installed widget. Other hardware families and host mutations remain deferred; their fixture coverage is accepted.

## Coverage still missing

- Intel GPU discovery, utilization, frequency, cache, and reset behavior.
- NVIDIA NVML path and `nvidia-smi` fallback.
- System battery and UPower peripheral batteries.
- Bolt and HID receiver/device paths.
- Suspend/resume recovery.
- Default-route/interface switching.
- Disk and device hotplug.

Existing fixture tests cover success, absence, malformed data, failure, timeouts, clamps, caching, and reset behavior. Prior live testing used an AMD host without Intel/NVIDIA GPU, batteries, supported peripherals, or HID/Bolt devices. The later AMDGPU pass exercised usage, codec, VRAM, clock, edge temperature, and average power through real sysfs reads. Capability loss/recovery stayed in fixtures; config demand changes and restart used a disposable production daemon. No host power-control or physical hotplug mutation was performed.

## Relevant files

- `src/sensors/gpu_amd.rs`
- `src/sensors/gpu_intel.rs`
- `src/sensors/gpu_nvidia.rs`
- `src/sensors/power.rs`
- `src/sensors/hid.rs`
- `src/sensors/network.rs`
- `src/sensors/disk.rs`
- `src/sensors/tests.rs`
- `tools/plasma_live_matrix.sh`

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
- AMDGPU scope and validated paths follow the [AMDGPU spec](../amdgpu-support/spec.md); unavailable tachometer hardware, AMD discrete GPUs, and legacy `radeon` remain unvalidated.
