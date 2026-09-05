# Add the public AMDGPU metric contract

Type: task
Status: resolved

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

## Completion evidence

Implemented on 2026-09-05. All seven AMDGPU tokens now have independent capabilities, value-only panel/tooltip admission, typed source/readings contracts, hardware-gated formatting, labels/icons, shipped tooltip defaults, and item documentation. Memory preserves exact used/total allocation bytes and renders MiB plus percentage. AMD color threshold pairs require exactly two values. Notification configuration is present; notification evaluation remains ticket 04. Discovery and scheduler branches remain inactive until tickets 02–03.

Validation passed with Rust 1.97.1: `cargo fmt -- --check`, `cargo check --all-targets --all-features`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all-targets --all-features` with 819 unit and 31 integration tests, `cargo doc --no-deps`, and `tools/repository_gate.sh`. The process-signal suite required execution outside the filesystem sandbox; its SIGTERM test timed out inside it. The shell's default Rust 1.98 triggered an unrelated pre-existing Clippy lint in `render/traces.rs`, so validation used the installed project toolchain without changing that code or weakening lints. `Cargo.lock` is unchanged.

`tools/qt_render_matrix.sh --no-build` passed and its contact sheet was inspected. Additional synthetic AMDGPU panel/tooltip renders were inspected with `tools/qt_shot.py`, including dark/light colors, memory context, units, fan-off, and absent-fan cases. Local artifacts are under `.test-artifacts/plasma/amd/`; the light fixtures supply the ambient light-theme text color because the screenshot helper defaults to white. These are synthetic render checks, not live AMDGPU telemetry evidence.

Next: ticket 02, discovery and sampling. The catalog is now 804 lines after exhaustive inactive AMD arms; retain ticket 03's planned GPU extraction before adding scheduler behavior.
