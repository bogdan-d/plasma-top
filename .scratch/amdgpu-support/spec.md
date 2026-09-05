# AMDGPU telemetry

Status: ready-for-agent

## Objective

Add capability-detected telemetry for one selected Linux AMDGPU device, with full support for the interfaces exposed by the development Strix Halo machine and truthful degradation across modern AMD APUs and discrete GPUs.

## Hardware evidence

The development machine exposes one AMDGPU DRM device at PCI `0000:c3:00.0` (`1002:1586`, Strix Halo / Radeon 8060S) through `/sys/class/drm/card1/device`. It provides `gpu_busy_percent`, `vcn_busy_percent`, `mem_info_vram_{used,total}`, an AMDGPU hwmon directory with edge temperature, `sclk`, average PPT power, and no fan tachometer. The selected hwmon directory must be resolved dynamically; `card1` and `hwmon9` are boot-specific names.

## Domain contract

- An **AMDGPU device** is a DRM display device bound to the Linux `amdgpu` driver. Legacy `radeon` devices are out of scope.
- PlasmaTop selects one AMDGPU device by sorting qualifying devices by canonical PCI identity and choosing the first. The stable PCI identity participates in scheduler source identity so a selection change invalidates old samples.
- Every metric is an independent capability. A missing direct source omits only that metric; it does not suppress other AMDGPU metrics.
- Discovery uses sysfs directly and preserves the existing `Confirmed` versus `Failed` reconciliation contract. Incomplete enumeration is failure, not confirmed absence.
- Sampling uses documented direct text files only. Do not parse binary `gpu_metrics`, DPM control files, or `/proc/*/fdinfo`, and do not add an external command or dependency.
- Confirmed source or capability removal invalidates the corresponding retained samples. A transient read failure retains the last valid sample and does not emit a notification.

## Public items

| Item | Meaning | Direct source | Formatting | Freshness |
| --- | --- | --- | --- | --- |
| `gpu_amd_usage` | Device-wide GPU activity | `gpu_busy_percent` | Percent | `display.poll_interval` |
| `gpu_amd_codec_usage` | Combined VCN video-codec activity, including encode or decode | `vcn_busy_percent` | Percent | `display.poll_interval` |
| `gpu_amd_mem_usage` | Used share of the driver-reported VRAM allocation domain | `mem_info_vram_used`, `mem_info_vram_total` | Used / total plus percent | `display.poll_interval` |
| `gpu_amd_freq` | Current graphics-core `sclk` | hwmon `freq*_input` whose label is `sclk` | MHz | `display.poll_interval` |
| `gpu_amd_temp` | Edge temperature only | hwmon `temp*_input` whose label is `edge` | Celsius | 30 seconds |
| `gpu_amd_power` | Average package/PPT power | hwmon `power1_average` | Watts | 30 seconds |
| `gpu_amd_fan_speed` | Measured fan tachometer speed | hwmon `fan*_input` | RPM, or `off` at zero | 30 seconds |

`gpu_amd_mem_usage` must not combine GTT with VRAM. On unified-memory APUs, its used and total values describe the driver VRAM allocation domain rather than dedicated physical memory or all GPU-accessible system memory.

Visible labels identify the vendor, for example `AMD GPU usage`, because AMD items may be rendered beside NVIDIA or Intel items and an AMDGPU device may be integrated or discrete.

## Demand and scheduling

- Use one fast AMDGPU job for demanded usage, codec, VRAM, and graphics-clock fields.
- Use one slow AMDGPU job for demanded edge-temperature, average-power, and tachometer fields.
- Each job reads only demanded fields within its group and retains per-metric sample state so one malformed or failed field cannot discard successful siblings.
- Request every AMDGPU item in the shipped default tooltip. Keep compact panel defaults unchanged. Hardware gating suppresses unsupported rows.
- Add AMDGPU inventory reconciliation only while AMD items, AMD notification demand, or AMD graph history are configured.

## Graphs and notifications

- The existing single GPU graph source precedence becomes NVIDIA, then AMDGPU, then Intel.
- AMDGPU graph history contains device usage and codec usage. The history source identity is `amd:<PCI identity>` and resets when the selected source changes.
- Add independent `notifications.gpu_amd_temp` and `notify_thresholds.gpu_amd_temp` settings, state, and vendor-specific message. Preserve all NVIDIA notification behavior and configuration.
- AMD temperature notification defaults to 80°C and disabled, with the existing sustained-temperature and hysteresis behavior.

## Threshold defaults

- `gpu_amd_usage`, `gpu_amd_mem_usage`, and `gpu_amd_temp`: `[50, 70]`.
- `gpu_amd_codec_usage`: active above `1`.
- Frequency, power, and fan speed have no universal color bands.

## Explicit exclusions

- Legacy `radeon` driver support.
- Generic `gpu_*` aliases or migration of existing vendor-specific tokens.
- Per-device items or a multi-device graph UI.
- Aggregated GTT and VRAM memory.
- Hotspot, junction, or memory-temperature substitution.
- Binary `gpu_metrics`, `pp_dpm_*`, fdinfo, or command fallbacks.
- AMD usage, memory, power, fan, clock, or codec notifications.

## Acceptance

- The development Strix Halo displays every available selected-page/default-tooltip AMD metric without hard-coded DRM or hwmon indexes; fan remains cleanly absent.
- A fixture AMDGPU dGPU with a tachometer renders all seven items.
- Missing optional files omit only their metric, malformed reads retain prior valid samples, and confirmed capability/device removal clears stale values.
- Multiple AMDGPU fixtures select the lowest sorted canonical PCI identity and reset retained/history state when that identity changes.
- Mixed-vendor graph fixtures enforce NVIDIA → AMDGPU → Intel precedence, and NVIDIA and AMD temperature alerts operate independently.
- No AMD source is read unless demanded, no control file is written, and no subprocess or dependency is introduced.
- Focused tests, all repository Rust gates, repository gate, current profiling, and live Strix Halo comparison evidence pass.

## Implementation sequence

1. [Add the public AMDGPU metric contract](issues/01-public-metric-contract.md).
2. [Discover and sample AMDGPU sources](issues/02-discovery-and-sampling.md).
3. [Integrate AMDGPU scheduling and lifecycle](issues/03-scheduler-integration.md).
4. [Add graphs and temperature notifications](issues/04-graphs-and-notifications.md).
5. [Validate the Strix Halo and repository](issues/05-live-and-release-validation.md).
