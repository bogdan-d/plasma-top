# Validate the Strix Halo and repository

Type: task
Status: ready-for-agent
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
