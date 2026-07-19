# Handoff: `BASE` / `base-p0.7-amd-host-20260719`

## Contract

- Objective: Refresh Phase 0.7 evidence on the available host, classify its GPU
  and power hardware explicitly, and leave unsupported live paths documented.
- Integration base SHA: `30f653d4e4e3b886ba324a36d3520c518da1b3dc`
- Branch/worktree: `rust-migration-base-bootstrap` @
  `/var/mnt/xdata/code/_self/plasma-stats`
- Owned paths: `scripts/capture-baseline.sh`, `plans/STATUS.md`, this handoff.
- Forbidden/shared paths: production Python/Rust, tests, config, style, QML.
- Dependencies verified integrated: existing Phase 0 capture harness and `.venv`.

## Result

- Status: `handoff`
- Commits: none.
- Changed files:
  - `scripts/capture-baseline.sh` records DRM/PCI display devices, power supplies,
    and generic hidraw node count in host metadata.
  - `plans/STATUS.md` records Wave 3 readiness and the current hardware gap.
  - This handoff records the ignored local evidence bundle's result.
- Behavior implemented/preserved:
  - refreshed `.test-artifacts/` on 2026-07-19 with zero required failures;
  - identified AMD Strix Halo integrated graphics (`1002:1586`, `amdgpu`);
  - confirmed `intel_gpu: (not found)`, `has_nvidia: False`, and missing
    `nvidia-smi`;
  - confirmed no system battery power-supply node and no supported mouse or
    keyboard battery reading;
  - preserved product/runtime behavior unchanged.
- Explicitly not implemented:
  - no claim of multi-host evidence: only this host was available;
  - no Intel, NVIDIA, UPower battery/peripheral, or Bolt device live coverage;
  - no AMD GPU metric support, which is outside the compatibility rewrite.

## Parity evidence

- Current Python files covered: `pirostats` probe/profiling/render paths and
  `tools/qt_shot.py` through `scripts/capture-baseline.sh`.
- Oracle fixtures/cases: existing full Python test suite and live-host capture.
- Exact differences remaining: supported GPU metrics target Intel/NVIDIA; this
  host's AMD iGPU exercises graceful absence only.
- Inventory entries proposed resolved: none.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `./scripts/capture-baseline.sh` | pass | `.test-artifacts/summary.txt`: `required_failures=0`; all HTML and Qt captures passed. |
| `./pirostats probe --config config/config.toml` via harness | pass | Probe records no Intel/NVIDIA GPU, no system battery, and two NVMe temperature sources. |
| DRM/PCI sysfs and `lspci -nnk` inventory | pass | AMD Strix Halo `1002:1586`, kernel driver `amdgpu`. |

## Dependencies and safety

- New/changed dependencies: none; `lspci` evidence is optional.
- Native/build/runtime requirements: sysfs on Linux; absent paths remain valid.
- Unsafe/FFI locations and invariants: none.

## Risks/blockers

- Known risks: generated evidence is ignored and machine-specific; review before
  external sharing.
- Blocker requiring integration decision: other hosts/devices are still needed
  for Intel, NVIDIA NVML/fallback, UPower, and Bolt/HID live evidence.
- Suggested next lane/API change: assign ready Wave 3 lanes while collecting the
  same capture bundle on those external targets.

## Review notes

- Diff inspected for out-of-scope paths: yes.
- Production runtime untouched by tests: yes.
- No skipped/weakened checks: yes.
- Rebase required before merge: no.
