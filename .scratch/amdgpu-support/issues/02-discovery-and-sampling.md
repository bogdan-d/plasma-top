# Discover and sample AMDGPU sources

Type: task
Status: ready-for-agent
Blocked by: 01

## Objective

Implement a cadence-free AMDGPU sensor owner that discovers one stable source and samples each documented direct capability independently.

## Work

- Add `src/sensors/gpu_amd.rs` around the selected AMDGPU inventory/source contract from ticket 01. Keep discovery/path parsing and retained sampling state behind this module's small interface.
- Enumerate numeric `/sys/class/drm/cardN` entries, resolve each `device` symlink, require PCI vendor `0x1002`, display class `0x03...`, and bound driver `amdgpu`, then select the lowest sorted canonical PCI identity.
- Resolve `device/hwmon/hwmon*` dynamically. Match case-normalized labels exactly enough to select edge temperature and `sclk`; use documented `power1_average` and `fan1_input` files without assuming the hwmon directory index.
- Return confirmed absence only after complete enumeration. Treat unreadable/malformed identity data, broken canonicalization, or incomplete qualifying-device inspection as discovery failure so reconciliation can retain the old source.
- Wire the detector into initial local hardware discovery with the existing outcome/merge semantics. Periodic demand-family scheduling remains ticket 03.
- Parse percentages with range validation, VRAM byte counters with nonzero total and checked arithmetic, hwmon millidegrees Celsius, hertz, microwatts, and RPM. Never open writable control files for writing.
- Maintain independent retained samples and attempt times per AMD metric. A group attempt updates successful fields, marks missing discovered capabilities absent, and retains prior values for transient read/parse failures.
- Put all test implementations under `src/sensors/gpu_amd/tests.rs` or cohesive child test modules.

## Focused checks

- Non-AMD, non-display, and non-`amdgpu` cards are excluded.
- Card and hwmon numbering are irrelevant; canonical PCI sorting selects deterministically across multiple AMDGPU cards.
- Incomplete enumeration is failure, while a complete empty enumeration is confirmed absence.
- Every source parser covers valid, boundary, malformed, unreadable, zero-total, overflow, and missing-file behavior.
- Strix Halo-like UMA fixtures preserve VRAM used/total without adding GTT; VCN remains codec usage.
- Missing optional capabilities do not suppress the device or sibling readings, and source replacement clears retained samples.

## Done when

Initial production discovery can populate the selected AMDGPU inventory, the module can be exercised entirely through fixtures, it exposes no cadence or command dependency, and its focused tests pass.
