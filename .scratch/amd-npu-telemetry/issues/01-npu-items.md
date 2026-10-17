# 01: AMD NPU tooltip and panel items

**What to build:** PlasmaTop detects the AMD NPU and shows `npu_amd_usage`, `npu_amd_power`, and `npu_amd_freq` in the tooltip (and optionally the panel), read from the APU's `gpu_metrics` v3.0 table without ever opening or waking the NPU, as described in the [spec](../spec.md). Machines without a qualifying NPU show nothing. The NPU chart is ticket 02.

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

- [ ] Detection follows the spec: an `amdxdna`-bound accel device plus the `amdgpu` device whose `gpu_metrics` header is `structure_size` ≥ 264, `format_revision` 3, `content_revision` 0, chosen independently of the selected AMDGPU device; source identity combines both PCI identities; reconciliation keeps the `Confirmed` versus `Failed` contract.
- [ ] One `gpu_metrics` read per sample decodes explicit little-endian fields at the spec's fixed offsets (busy columns 46, power 116, clock 180), bounds-checked against bytes read and declared size, never cast into a struct; the file's 4096-byte size is ignored.
- [ ] Every row of the spec's outcome rules holds: failed reads retain values and capture times; confirmed removal clears; unsupported revision or size is not detected at discovery and drops a retained source; `0xFFFF` clears only its metric (any unavailable column clears busy); a column above 100 fails busy only while power and clock update.
- [ ] The three items exist as independent capabilities with the AMD GPU items' value-only forms and surfaces, `display.poll_interval` freshness, one shared owner and read, and no work unless demanded.
- [ ] Display: busy as percent with `[50, 70]` thresholds; power in watts with one decimal from integer milliwatts (at most `65.5 W`); clock in MHz with `off` at zero (at most 65534 MHz), via a small NPU formatter rather than the AMD GPU one.
- [ ] Labels in `lang/*.toml`, glyphs in `style/icons.toml`, colors in CSS mirrored between dark and light styles; widths bounded and covered by the canonical-width test, whose full-hardware fixture includes the NPU source.
- [ ] The default tooltip GPU section lists the three items after the AMD GPU items; the panel default is unchanged; `plasma-top list-items` shows them; `plasma-top probe` reports the NPU source and latest readings.
- [ ] Fixtures: the real suspended table (`.tmp/npu-capture/gpu_metrics.suspended.bin`) and active table (`.tmp/npu-capture/flm/gpu_metrics.active.bin`, busy 96%, 3352 mW, 1810 MHz) are copied into the test fixtures, plus the spec's synthetic variants (wrong format and content revision, declared size larger than bytes, short file, `0xFFFF` fields, column above 100, no accel device, discrete-plus-APU tree).
- [ ] Seam-1 tests cover detection, reconciliation, every outcome rule including a valid sample followed by a mixed unavailable/invalid/valid table, and demand gating through the existing scheduled-owner path; seam-2 render tests and goldens cover active, idle, absent, `off` clock, one-decimal power, thresholds, and `list-items`.
- [ ] Items doc gains the NPU section (source, firmware time-filtering, suspended reads as idle, time-filtered clock briefly non-zero after suspend, NPU power is part of PPT); glossary gains AMD NPU, NPU busy, NPU power, and NPU frequency, and records the scoped `gpu_metrics` exception; metrics inventory lists the source.
- [ ] Full Rust and repository gates pass; Qt render review of the new rows; touched files respect the line limits.
