# PlasmaTop

PlasmaTop gathers system metrics and presents them through a Plasma panel and tooltip. This glossary distinguishes metric sampling from display publication so freshness and coherence have precise meanings.

## Language

**Metric sample**:
A reading for one metric at a specific capture time. Samples for different metrics need not share a capture time.
_Avoid_: Poll result, collection-cycle value

**Sample age**:
The elapsed monotonic time since a metric sample was captured.
_Avoid_: Cache age

**Freshness budget**:
The maximum intended sample age before a new sampling attempt becomes due. Exceeding the budget does not by itself remove the latest sample from a display snapshot.
_Avoid_: Cache TTL, poll interval

**Demand set**:
The metrics currently required by the panel, enabled notifications, configured graph history, or a presented tooltip and its selected page. Metrics outside the demand set do not need fresh samples.
_Avoid_: Enabled metrics, configured metrics

**Hardware inventory**:
The latest known set of hardware that can provide PlasmaTop metrics. The inventory may change as devices and services appear or disappear.
_Avoid_: Hardware snapshot

**AMDGPU device**:
An AMD graphics device supported through the Linux AMDGPU driver. Available metrics vary by device capability.
_Avoid_: AMD device, AMD iGPU

**Selected AMDGPU device**:
The single AMDGPU device whose samples back vendor-specific AMD items when one or more AMDGPU devices are present.
_Avoid_: Primary AMD GPU, First AMD GPU

**AMDGPU memory usage**:
The used share of an AMDGPU device's driver-reported VRAM allocation domain. On unified-memory devices, it does not represent dedicated physical memory or all memory accessible to the GPU.
_Avoid_: AMD GPU RAM usage, AMD system-memory usage

**AMDGPU temperature**:
The selected AMDGPU device's edge temperature. It does not silently substitute hotspot, junction, or memory temperature.
_Avoid_: AMDGPU hotspot, AMDGPU junction temperature

**AMDGPU fan speed**:
The selected AMDGPU device's measured fan tachometer speed in revolutions per minute.
_Avoid_: AMDGPU fan duty, AMDGPU PWM

**AMDGPU codec usage**:
The busy percentage of the selected AMDGPU device's combined video codec block. It may include decoding and encoding activity.
_Avoid_: AMDGPU decoder usage, AMDGPU encoder usage

**AMDGPU power**:
The selected AMDGPU device's driver-reported average package power in watts.
_Avoid_: AMDGPU instantaneous power, AMDGPU energy

**AMDGPU frequency**:
The selected AMDGPU device's current graphics-core clock frequency.
_Avoid_: AMDGPU memory clock, AMDGPU fabric clock, AMDGPU SoC clock

**Display snapshot**:
The latest completed metric samples selected together for one display publication. A display snapshot may carry an older sample when no newer sample completed before its publication deadline.
_Avoid_: Collection snapshot, coherent sample

**Selected page**:
The tooltip page chosen for presentation, whether or not the tooltip is currently visible.
_Avoid_: Active page

**Presented tooltip**:
A tooltip surface currently visible because it is hovered, pinned, or shown as a planar/full representation. Page-owned metric samples are demanded only while the tooltip is presented.
_Avoid_: Active tooltip, open tooltip
