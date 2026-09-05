# Validate the Strix Halo and repository

Type: task
Status: resolved
Blocked by: 04

## Objective

Prove the fixture implementation against the development Strix Halo, rendering path, performance contract, and complete repository gates.

## Work

- Record kernel, AMDGPU module, PCI identity, exposed source paths, permissions, Plasma/session version, config, and release commit used for evidence.
- Compare daemon readings with direct source values and one bounded corroborating `amdgpu_top` snapshot. Treat direct documented sysfs values as authoritative; do not make `amdgpu_top` a runtime or test dependency.
- Verify usage and codec response under a safe workload, VRAM used/total semantics, graphics clock units, edge temperature, average watts, and clean fan absence on Strix Halo.
- Exercise safe capability disappearance/recovery through fixtures; do not mutate writable DPM or power-control files on the host. Exercise daemon restart and config-driven demand changes live.
- Run `./plasma-top profiling --config config/config.toml` and a representative timed tooltip/graphs scenario. Compare discovery, job counts, subprocess counts, wake behavior, and publication timing with an unchanged baseline; AMD support must introduce no subprocess.
- Validate panel, tooltip, graphs, colors, width, and missing rows through `tools/qt_shot.py` or `tools/qt_render_matrix.sh` and the live applet.
- Update `docs/METRICS_INVENTORY.md`, `docs/PERFORMANCE.md`, and `.scratch/live-hardware-and-mutation-validation/spec.md` with implemented cadence/source facts and reproducible live evidence. Keep `CONTEXT.md` free of implementation details.

## Required gates

```bash
cargo fmt -- --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets --all-features
cargo doc --no-deps
tools/repository_gate.sh
```

Run the exact current commands from `docs/DEVELOPMENT.md` if they differ when implementation begins.

## Done when

All gates pass, Qt and live Strix Halo evidence is recorded, readings match their direct sources and units, unsupported metrics remain absent, and no claim is made for untested legacy `radeon` hardware.

## Comments

2026-09-05: Completed host/sysfs and `amdgpu_top` comparison, bounded Vulkan and VA-API workloads, release baseline comparison, disposable daemon restart/config demand checks, all 880 tests and required repository/package gates, Qt visual review, and application-form QML smoke. Fixed the probe output omission for AMD source paths and all seven readings. Details, reproduction commands, timings, and limits are in [the validation report](../validation.md).

2026-09-05 follow-up: Bogdan ran `./install.sh` and supplied a screenshot of the installed widget. Visual review confirms all six supported AMD rows, correct units and VRAM threshold color, clean fan absence, and tooltip geometry without clipping. Installation approval is no longer pending. The ticket remains `ready-for-human` while installed-widget hover/pinning, wheel paging to AMD graphs, and light/dark theme switching await confirmation. See the installed-widget evidence in [the report](../validation.md).

2026-09-05 acceptance: In response to the installed-widget interaction checks, Bogdan confirmed that everything seems to work fine.

## Answer

Validated AMDGPU telemetry on Strix Halo against direct sysfs readings and a bounded `amdgpu_top` snapshot, with graphics/codec workload response and clean fan absence. The release baseline comparison showed no added subprocesses or missed display deadlines. All 880 tests, required gates, Qt visual checks, and disposable QML/lifecycle checks passed. Bogdan installed the candidate, supplied live-tooltip evidence, and confirmed normal operation. Fixed the diagnostic omission of AMD source paths and readings; documented results and measurement limits in [the validation report](../validation.md), `docs/METRICS_INVENTORY.md`, and `docs/PERFORMANCE.md`.
