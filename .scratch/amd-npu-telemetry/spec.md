# AMD NPU telemetry

Status: ready-for-agent

## Problem Statement

The development machine runs local models on its AMD XDNA NPU (Strix Halo, `1022:17f0`, `amdxdna` driver) next to GPU work, but PlasmaTop shows nothing about it. The user cannot see whether the NPU is busy, how much power it draws, or how its load changes over time, while CPU and AMD GPU have tooltip rows and graphs.

## Solution

PlasmaTop detects the AMD NPU and shows its busy percentage, power, and clock as tooltip and panel items, plus an NPU usage chart on the graphs page, in the same style as the AMD GPU items. Readings come from the APU's power-management metrics table that the `amdgpu` driver publishes in sysfs, so polling never opens the NPU device and never wakes it from runtime suspend. A suspended NPU reads as idle.

## Research summary

- **No usable Rust library.** crates.io has execution runtimes and kernel replayers (`hrx-rs`, `taconite*`, `rlx-xdna`, `virtio-accel-xdna`), a binary-only monitor (`amdtop`), and `libamdgpu_top`. `libamdgpu_top` exposes XDNA device lookup, a few `amdxdna` info ioctls, and per-process fdinfo, but no sensor query; it links libdrm by default and targets amdgpu broadly. None reads NPU busy or power without opening the device. No dependency is added.
- **Sensor ioctl rejected.** The `amdxdna` sensor query (`DRM_IOCTL_AMDXDNA_GET_INFO` with `DRM_AMDXDNA_QUERY_SENSORS`), used by the earlier dashboard helper in the strix-halo-inference project, gives per-column busy and total power. It needs an open of `/dev/accel/accel0`, which wakes a suspended NPU for the 5-second autosuspend delay, and an `ioctl` that would need `unsafe` code the crate denies. Polling it would keep the NPU awake.
- **Chosen source: `gpu_metrics` v3.0.** The APU's `amdgpu` device publishes a binary `gpu_metrics` table. On this machine it reports `structure_size` 264, `format_revision` 3, `content_revision` 0, decoded with natural C alignment (verified against live values). The Strix Halo SMU driver fills `average_ipu_activity[8]` (per-column busy %), `average_ipu_power` (mW), `average_ipuclk_frequency` (MHz), and IPU read/write bandwidth. All IPU fields read 0 while the NPU is suspended. Reading the file does not touch the NPU.
- **Earlier measurements** (strix-halo-inference, October 14, 2026, via the sensor ioctl, which reads the same firmware): small-model inference read about 37–44% busy at 1.15–1.5 W. Busy dropped to zero within four seconds of the last request; power fell off over tens of seconds. The firmware time-filters its readings.

## User Stories

1. As a user running NPU models, I want to see NPU busy percentage in the tooltip, so that I know whether the NPU is working.
2. As a user, I want to see NPU power in watts with one decimal, so that small NPU draws like 1.2 W are visible.
3. As a user, I want to see the NPU clock, so that I can tell whether the NPU is clocked up.
4. As a user, I want an idle NPU to read 0% and 0.0 W, and a suspended NPU to also show its clock as `off`, so that idle and suspended are unambiguous. An idle NPU that another process keeps awake shows its real clock.
5. As a user, I want an NPU usage chart on the graphs page, so that I can watch NPU load over time like CPU and GPU.
6. As a user, I want to add, remove, and reorder the NPU chart from the Graphs settings page, so that I control the graphs page like other charts.
7. As a user, I want NPU items placeable on the panel, so that I can keep NPU busy always visible if I choose.
8. As a user, I want NPU busy colored by thresholds like GPU usage, so that heavy NPU load stands out.
9. As a laptop user, I want PlasmaTop's NPU polling never to wake the NPU, so that monitoring costs no NPU power.
10. As a user on a machine without an AMD NPU, I want NPU items and the NPU chart omitted silently, so that the shared config works everywhere.
11. As a user, I want NPU rows to appear in the default tooltip and the chart in the default graph order, so that I get NPU telemetry without editing config.
12. As a user, I want a transient read failure to keep the last valid NPU values, so that rows don't flicker.
13. As a user, I want the NPU chart history to reset if the NPU source changes, so that the chart never mixes devices.
14. As a user, I want NPU work to cost nothing when no NPU item or chart is demanded, so that unused features stay free.
15. As a user, I want `plasma-top list-items` to show the NPU items and where they can go, so that I can configure them.
16. As a user, I want `plasma-top probe` to report the detected NPU and its readings, so that I can check detection.
17. As a maintainer, I want the `gpu_metrics` parser bounds-checked and version-gated, so that a different layout produces absence instead of wrong numbers.
18. As a maintainer, I want NPU readings documented with their source and caveats, so that no one mistakes NPU power for an addition to PPT power.

## Implementation Decisions

### Detection

- An **AMD NPU** is present when an accel device is bound to the `amdxdna` driver and an `amdgpu` DRM device exposes `gpu_metrics` with `format_revision` 3, `content_revision` 0, and `structure_size` of at least 264. If several `amdgpu` devices exist, the NPU source is the one whose `gpu_metrics` meets that rule, independently of the selected AMDGPU device (a discrete GPU never carries NPU fields).
- The source identity combines the accel device's PCI identity and the metrics device's PCI identity, so a change invalidates samples and history. Discovery follows the existing `Confirmed` versus `Failed` reconciliation: incomplete enumeration is failure, not confirmed absence.
- This is a deliberate, NPU-only exception to the AMDGPU rule against parsing binary `gpu_metrics`; AMDGPU items keep their text sources. Record the exception and its reason in the docs and glossary.

### Parsing

- One read of `gpu_metrics` per sample. The file reports a size of 4096 but returns `structure_size` bytes (264 here); bounds checks use the bytes actually read and the declared `structure_size`, never the file size.
- Decode explicit little-endian integers at fixed natural-alignment offsets of the v3.0 layout; never cast bytes into a struct. Fields used:

  | Field | Byte offset | Type |
  | --- | ---: | --- |
  | `structure_size` | 0 | `u16` |
  | `format_revision` / `content_revision` | 2 / 3 | `u8` |
  | `average_ipu_activity` | 46 | 8 × `u16`, percent |
  | `average_ipu_power` | 116 | `u16`, mW |
  | `average_ipuclk_frequency` | 180 | `u16`, MHz |

- `0xFFFF` in a 16-bit field means unavailable. Busy is the mean of the eight columns. Power is kept as integer milliwatts until formatting.

### Outcome rules

| Condition | Effect |
| --- | --- |
| I/O error, short read, or bytes fewer than the declared size on a retained source | Failed attempt: inventory, values, capture times, and history are retained |
| Complete enumeration confirms the accel or metrics device is gone | Confirmed absence: NPU capabilities, values, and history are cleared |
| At discovery, a complete readable table with an unsupported revision or `structure_size` below 264 | No NPU source is detected |
| A retained source later returns a complete readable table with an unsupported revision or size | Treated as source change: values and history are cleared and every metric records absence; only a later table passing the discovery version check shows values again, and rediscovery drops the source while the layout stays unsupported (the owner cannot observe rediscovery, so it re-qualifies with the same header check) |
| Valid table, a field is `0xFFFF` | That metric records valid absence; others update normally. Any unavailable busy column makes busy unavailable |
| Valid table, a busy column above 100 | Busy records a failed attempt and keeps its previous sample; power and clock still update |

### Items

| Item | Meaning | Field | Display |
| --- | --- | --- | --- |
| `npu_amd_usage` | Mean NPU column busy %, firmware time-filtered | `average_ipu_activity[8]` | Percent; thresholds like `gpu_amd_usage` |
| `npu_amd_power` | Time-filtered NPU power, part of PPT | `average_ipu_power` (mW) | Watts with one decimal, at most `65.5 W` |
| `npu_amd_freq` | Time-filtered NPU clock | `average_ipuclk_frequency` (MHz) | MHz; `off` at zero |

- All three accept the same value-only forms and surfaces as the AMD GPU items. Each is its own capability, gated on the detected source.
- All three share one owner and one file read, with `display.poll_interval` freshness. Each sample reads only when at least one NPU item or the NPU chart is demanded. Because one read yields every field, graph-only demand does not try to exclude power or clock.
- Display widths are bounded by the field types (busy 0–100, power up to 65.5 W, clock up to 65534 MHz since 65535 is the unavailable sentinel) and covered by the canonical-width test.
- The AMD GPU scalar formatter emits integer watts and recognizes `off` only for zero RPM, so NPU power and clock need a small NPU formatter for one-decimal watts and the zero-clock `off`.
- Labels go in `lang/*.toml`, glyphs in `style/icons.toml`, colors in CSS, mirrored between dark and light styles.

### Graphs

- New chart id `npu`: NPU busy % history on a 0–100 scale, labelled like the other percentage charts. It appears only when the NPU source is detected.
- The NPU chart is configured only when `pages.order` contains `graphs` and `pages.graph_order` contains `npu`. This differs on purpose from the existing GPU and network history jobs, which run whenever the graphs page is configured; NPU history must not. While configured, NPU discovery, usage sampling, and history run even when the graphs page is not presented. Removing either condition stops history and clears it; panel or presented-tooltip NPU demand continues independently.
- History follows `display.history_interval` and `graph_history_length`, resets on source change, and stays separate from the selected-GPU history so the NVIDIA, AMDGPU, then Intel chart selection is unchanged.
- The chart joins the known chart list, the default `graph_order` after `gpu`, the Graphs settings page (label "NPU usage"), and the settings-page help text.

### Defaults and docs

- The default tooltip's GPU section gains `npu_amd_usage`, `npu_amd_power`, and `npu_amd_freq` after the AMD GPU items; `npu_amd_usage` gets `[50, 70]` thresholds. The panel default is unchanged.
- `plasma-top probe` reports the NPU source and its latest readings.
- The items doc gains an NPU section with the source, firmware time-filtering, suspended-reads-as-idle, and that NPU power is part of PPT power, not added to it. The glossary gains AMD NPU, NPU busy, NPU power, and NPU frequency terms. The metrics inventory lists the new source.

## Testing Decisions

- Good tests assert observable behavior: detected capabilities, sampled values or absence, rendered rows, and chart presence. They do not assert byte offsets or private helpers directly.
- **Seam 1: sensor owner against fixture sysfs trees.** Capture real binary `gpu_metrics` from this machine into fixtures: one with the NPU suspended (all IPU fields zero) and one taken while the NPU runs work. Add synthetic variants derived from them: wrong format revision, wrong content revision, declared size larger than the bytes returned, short file, `0xFFFF` fields, a column above 100, no `amdxdna` accel device, and a discrete-plus-APU tree. Cover every row of the outcome rules, including a valid sample followed by a table mixing unavailable, invalid, and valid fields (checking which metrics clear, which retain, and which update). Cover item demand gating, including removing all NPU item demand, through the existing scheduled-owner path. Prior art: the AMD GPU sensor tests with fixture `sys` roots.
- **Chart history (ticket 02).** Cover chart presence and absence, chart-only demand, removing the chart, removing all NPU demand while the chart is configured, and history reset on source change.
- **Seam 2: render and goldens.** Tooltip rows for all three items (active, idle, absent), `off` clock, one-decimal power, thresholds, the NPU chart on the graphs page, `list-items` output, and canonical-width coverage before the first reading and at valid extremes. The canonical-width test skips items that render empty, so its full-hardware fixture must include the NPU source or the coverage passes without testing anything. Prior art: existing render tests, golden outputs, and the graph-chart tests.
- **Gates.** The full Rust and repository gates, Qt render review of the new rows and chart, and a Graphs settings smoke check, per the development doc.
- **Live check.** Close any app that holds the NPU open first (for example GNOME Resources, which opens `/dev/accel/accel0` on every refresh and keeps the NPU `active`); otherwise `runtime_status` never returns to `suspended`. Run NPU load on this machine (see NPU load for validation), confirm busy, power, and clock rise with load and track the earlier measurements, readings return to idle after work stops, and the NPU's `runtime_status` returns to `suspended` while PlasmaTop keeps polling. Capture the active `gpu_metrics` fixture during this run.

### NPU load for validation

Manual validation only; automated tests use the captured fixtures and never need an NPU.

1. **`xrt-smi validate`** is not usable here: the CachyOS `xrt` 2.21.75 and `xrt-plugin-amdxdna` packages ship no validate test archives, so every test fails with "No archive provided, skipping test". It only opens the device, which wakes the NPU without running work.
2. **FastFlowLM with `qwen3:0.6b`** is the load generator (0.66 GB, Apache-2.0 weights) for realistic sustained inference. FastFlowLM 1.0.9 (ROCm org, MIT runtime, free-to-redistribute binary NPU kernels) supports Strix Halo and Arch Linux. This machine already meets its prerequisites: in-tree `amdxdna` on kernel 7.2 (7.0+ suffices), NPU firmware 1.1.2.65 (1.1.0.0+ required), `xrt` and `xrt-plugin-amdxdna` installed. It lives only in this repository: the portable build is unpacked in the gitignored `.tmp/fastflowlm/` (tarball kept beside it), and every invocation sets `FLM_MODEL_PATH` to `.tmp/fastflowlm/models` and `FLM_DISABLE_UPDATE_CHECK=1`, so nothing is written under the user's home. Set up and checked on 2026-10-17: `flm validate` found the NPU (8 columns, aie2p, firmware 1.1.2.65, amdxdna 0.10, 32768 MB memlock), and `flm pull qwen3:0.6b` downloaded and verified 664 MB. Drive it with `tools/npu_load.sh [DURATION_SECONDS]` (default 60), which runs `flm serve qwen3:0.6b`, sends chat requests to its local server (port 52625) until the duration ends, and logs to `.tmp/fastflowlm/serve.log`. The v1.0.9 portable tarball (SHA-256 `6ecd27d59f1f17650faaedc11787c4b755912a1725508ef15a772a6cc98a582e`, 186 MB unpacked) is self-contained: a `flm` wrapper script, `flm-real`, bundled XRT 2.25 libraries, and NPU kernels. Checked without running it: it needs at most glibc 2.39 (this machine has 2.44), and its only library missing here, `libprotobuf.so.32`, is needed only by XRT's emulation libraries, which real hardware does not load. The AUR packages are stale (0.9.x).

FastFlowLM upstream recommends an unlimited memlock limit; this user's limit is 32 GiB, which should cover a 0.66 GB model, but confirm on first run. Strix Halo can hang or corrupt NPU results when GPU and NPU work overlap while the GPU fabric clock (FCLK) changes speed. Run every NPU workload as `tools/npu_fabric_clock.sh with -- <command>`. It holds FCLK through the installed copy, runs the command as the current user, and releases on exit, failure, or interruption. `tools/npu_fabric_clock.sh status` needs no root.

- The installed copy is a root-owned copy of `tools/npu_fabric_clock.sh` at `/usr/local/sbin/plasma-top-npu-fabric-clock`. Its sudoers rule allows exactly `hold` and `release`, with `NOSETENV`, and never points at the user-writable repository file. Re-install the copy after editing the script.
- The tool acts only on the single Strix Halo iGPU (`1002:1586`, bound to `amdgpu`) and refuses ambiguity. As root it always writes the real `/sys` and refuses an alternate sysfs root.
- `hold` sets `power_dpm_force_performance_level` to `manual` and `pp_dpm_fclk` to the top level, checking each write, then waits up to 10 s for the time-filtered `average_fclk_frequency` in `gpu_metrics` v3.0 (byte 182) to reach 98% of the top level. The `pp_dpm_fclk` `*` marker is not used: kernel 7.2 marks only an exact match with the sampled FCLK, which floats between levels, so no level was ever marked on this machine. Measured on 2026-10-17: under `manual` the sampled FCLK climbed from about 1690 to 1986 MHz within 3 s and fell back after release. Any failed write or verification restores `auto` and reports a failed restore separately.
- `release` writes `auto` and reads it back before reporting success.
- Holding costs about 2.3 W at idle, so it must not stay held. The tool is PlasmaTop's own; no third-party script is copied.

## Out of Scope

- Intel or other NPUs; the `npu_amd_` prefix leaves room for them.
- Per-column busy items, NPU bandwidth items, and the MP-IPU clock.
- Per-process NPU usage from `/proc/*/fdinfo` on the processes page.
- NPU notifications.
- Reading the `amdxdna` sensor ioctl or opening the accel device for any reason.
- Changing AMD GPU item sources.
- Installing FastFlowLM or any NPU runtime as part of PlasmaTop, its installer, or CI.

## Further Notes

- The firmware time-filters busy and power, so readings lag work by a few seconds and power decays over tens of seconds; the docs should say so rather than the code compensating.
- On other APU generations the v3.0 layout may differ or IPU fields may be unfilled. The version gate and `0xFFFF` handling limit wrong readings, but a matching version alone cannot prove a different generation fills the IPU fields; support stays scoped to this machine until another has its own fixtures and live check.
- Observed on 2026-10-17 while `xrt-smi` opened the NPU without running work: `runtime_status` went `active` for about 6 s (the 5 s autosuspend delay), `average_ipuclk_frequency` rose to about 1807 MHz, busy stayed 0, and power read 0–2 mW. After suspend, the clock decayed smoothly to 0 over about 9 s, so the time-filtered clock can read non-zero on a suspended NPU. Non-zero busy and realistic power came from the FastFlowLM run below.
- Observed on 2026-10-17 with FastFlowLM `qwen3:0.6b` serving 15 chat requests over 64 s (about 90 tokens/s, FCLK held): busy settled at 93–95% (median 93%), power at 3.1–3.4 W (median 3.32 W, peak 3.44 W), and the clock at 1810 MHz. All eight columns always read the same value, so the mean equals any column. Ramp-up took about 4 s from the first request. After the last request, busy fell to 0 within about 5 s, power to under 10 mW within about 6 s, and the NPU suspended about 6 s after the load ended. The sampler read `gpu_metrics` every 0.5 s throughout and the NPU still suspended, so these reads do not keep the NPU awake. Power decays in seconds here, faster than the earlier ioctl-based observation of tens of seconds. The table with the highest busy is kept as the active fixture source in the gitignored `.tmp/npu-capture/flm/`, beside the suspended one in `.tmp/npu-capture/`.
- Observed on 2026-10-17 with GNOME Resources 51.0 open: it reads busy, power, clock, and TOPs through `amdxdna` ioctls, opening the accel device for each query, and the NPU stayed `active` continuously with no work. Meanwhile `gpu_metrics` read busy 0, power 0 mW, and clock 800 MHz, so `off` means suspended (clock decayed to 0), not merely idle. Its per-process NPU usage from `/proc/<pid>/fdinfo` (`drm-engine-npu-amdxdna`, `drm-total-memory`) never opens the device and is the model for any future per-process NPU work.
- Reading `gpu_metrics` does not resume a suspended iGPU: kernel 7.2 only accesses metrics when the device is already active and returns an error otherwise, which the outcome rules treat as a failed attempt. Reading does not open or resume the NPU. On this machine the iGPU drives the display and its runtime PM is pinned on (`power/control` is `on`, `runtime_status` is `active`), so `gpu_metrics` stays readable and failed reads are transient; retained values never expire, so a machine whose iGPU can runtime-suspend while showing non-zero NPU values needs its own live check before support extends to it.
- Spec reviewed by Codex (`gpt-6.1-sol`, xhigh) on 2026-10-17; findings verified against the code and incorporated.
