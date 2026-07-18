# Handoff: `BASE` / `base-oracle-20260719`

## Contract

- Objective: Implement the minimal Phase 0.4 / P0.5 starter only: a deterministic Python oracle harness for the render path, with one full fixture, one focused parity test, and no production-code changes.
- Integration base SHA: `cba9a33390de836348dd7bb6802a9869d3442ee5`
- Branch/worktree: `rust-migration-base-bootstrap` @ `/var/mnt/xdata/code/_self/plasma-stats`
- Owned paths:
  - `tests/oracle.py`
  - `tests/test_oracle.py`
  - `tests/fixtures/oracle_render_full.toml`
  - `plans/handoffs/base-oracle-20260719.md`
- Forbidden/shared paths:
  - `src/**`
  - existing `tests/**` files other than the two new oracle files
  - `config/**`
  - `style/**`
  - `plans/STATUS.md`
  - `plans/LANES.md`
  - `README.md`
  - `AGENTS.md`
  - `packaging/**`
  - `install.sh`
  - `uninstall.sh`
- Dependencies verified integrated: none; this slice is intentionally self-contained and read-only with respect to production code.

## Result

- Status: `handoff`
- Commits: none (working tree changes only)
- Changed files:
  - `tests/oracle.py` — deterministic fixture loader + render helper + tiny CLI
  - `tests/test_oracle.py` — focused golden parity test for panel vertical, panel horizontal, and tooltip
  - `tests/fixtures/oracle_render_full.toml` — readable full-hardware/full-readings render fixture
  - `plans/handoffs/base-oracle-20260719.md` — this handoff
- Behavior implemented/preserved:
  - added a deterministic oracle harness that reconstructs `HardwareInfo`, `Readings`, `DiskUsage`, `BatterySys`, and `BatteryPeriph` from one TOML fixture and renders `panel_v`, `panel_h`, or `tooltip` through the existing Python formatter
  - matched `tests/test_golden_render.py` determinism rules by freezing `config.detect_panel_geometry()` to `PanelGeometry(vertical=True)` and freezing `time.time()` to `1_000_000.0`
  - kept the scope limited to the shipped default full-render path for future Rust differential comparison work
  - preserved production Python code, config defaults, and existing golden HTML snapshots unchanged
- Explicitly not implemented:
  - any `src/**` production changes
  - any golden snapshot changes
  - any broader fixture schema or Rust-side differential harness

## Parity evidence

- Current Python symbols/files covered:
  - `tests/test_golden_render.py` fixture values and deterministic render conditions
  - `config.load_config`
  - `config.apply_canonical_width`
  - `formatter.PanelFormatter`
  - `sensors.HardwareInfo`, `Readings`, `DiskUsage`, `BatterySys`, `BatteryPeriph`
- Oracle fixtures/cases:
  - `tests/fixtures/oracle_render_full.toml`
  - `panel_v`
  - `panel_h`
  - `tooltip`
- Exact differences remaining:
  - none in the scoped render path; the oracle outputs are byte-identical to the current goldens for all three covered surfaces
- Inventory entries proposed resolved:
  - none recorded in this slice

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `. .venv/bin/activate && python -m pytest tests/test_oracle.py -v` | pass | New focused oracle parity test passed: `1 passed`. |
| `. .venv/bin/activate && python -m pytest tests/test_golden_render.py -v` | pass | Existing golden render safety net passed: `3 passed`. |
| `. .venv/bin/activate && python tests/oracle.py tests/fixtures/oracle_render_full.toml panel_v > /tmp/pirostats_oracle_panel_v.html && cmp -s /tmp/pirostats_oracle_panel_v.html tests/golden/panel_v.html` | pass | CLI smoke path produced byte-identical vertical-panel HTML. |
| `git diff --check` | pass | No whitespace or patch-format issues in this slice. |
| `git rev-parse HEAD && git branch --show-current && git status --short` | pass | Base SHA `cba9a33390de836348dd7bb6802a9869d3442ee5`; branch `rust-migration-base-bootstrap`; working tree already contains unrelated doc/setup changes outside this slice. |

## Dependencies and safety

- New/changed dependencies and review:
  - none; stdlib `tomllib`, `argparse`, and `unittest.mock` only
- Native/build/runtime requirements:
  - same Python test environment as the existing suite
- Unsafe/FFI locations and invariants:
  - none touched

## Risks/blockers

- Known risks:
  - the fixture intentionally covers only the shipped default full-render path, not future fixture permutations
  - the branch/worktree is not clean: pre-existing unrelated changes are present in `README.md`, `docs/DEVELOPMENT.md`, `requirements-dev.txt`, and `plans/handoffs/base-copilot-20260719.md`; reviewers should isolate this slice from those files
- Blocker requiring integration decision:
  - none currently
- Suggested next lane/API change:
  - use this oracle fixture/output path as the first Rust differential reference before expanding fixture coverage

## Review notes

- Diff inspected for out-of-scope paths: yes
- Production runtime untouched by tests: yes
- No skipped/weakened checks: yes
- Rebase required before merge: no