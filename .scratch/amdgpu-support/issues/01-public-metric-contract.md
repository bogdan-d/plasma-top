# Add the public AMDGPU metric contract

Type: task
Status: ready-for-agent

## Objective

Add the complete vendor-specific AMD item/config/render vocabulary without changing existing NVIDIA or Intel behavior.

## Work

- Add `GpuAmdUsage`, `GpuAmdCodecUsage`, `GpuAmdMemUsage`, `GpuAmdFreq`, `GpuAmdTemp`, `GpuAmdPower`, and `GpuAmdFanSpeed` to `src/domain/metric.rs`, including exact token parsing/formatting, capabilities, allowed forms, and panel/tooltip admission.
- Add the selected AMDGPU inventory/source shape to `src/domain/readings.rs`, including canonical PCI identity and optional direct-source paths for every capability. This is the stable cross-stage contract used by synthetic rendering now and discovery/scheduling in later tickets.
- Extend `DisplaySnapshot` and bounded canonical-width fixtures in `src/domain/readings.rs` with typed AMD readings. Preserve used/total bytes for AMDGPU memory so rendering can show context rather than only a percentage.
- Add color and notification configuration fields in `src/config/schema.rs` and `config/config.toml` with the defaults fixed by the spec. Unknown-field warnings and existing config merge behavior must remain unchanged.
- Add hardware-gated formatter/registry arms in `src/render/registry.rs` and `src/render/formatter/`; use vendor-visible labels, MHz, watts, RPM/off, and used/total-plus-percent formatting.
- Add English labels and icons in `lang/en.toml` and `style/icons.toml`; reuse existing GPU glyphs unless a genuinely clearer existing glyph is available.
- Request all seven items in the default tooltip while leaving panel defaults unchanged.
- Document the items in `docs/ITEMS.md`, including the UMA VRAM-allocation caveat and VCN codec semantics.

## Focused checks

- Token round trips, unsupported-form rejection, surface admission, capability derivation, and `list-items` coverage.
- Config defaults and old config compatibility.
- Synthetic AMD panel/tooltip formatting, hardware gating, thresholds, units, zero-fan rendering, missing-value omission, and canonical tooltip width.
- Default tooltip configuration parses and requests all AMD items without rendering unsupported rows.

## Done when

The public contract is exhaustive and testable with synthetic readings, existing vendor behavior is unchanged, and focused domain/config/render tests pass.
