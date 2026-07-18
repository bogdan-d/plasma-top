# Handoff: `BASE` / `base-p0.7-pages-20260719`

## Contract

- Objective: Extend the local Phase 0 baseline/live-evidence harness so the current host also records deep-dive tooltip page evidence without touching product runtime code.
- Integration base SHA: `670cc9ea9658f642edf7906ac876d572e5d10171`
- Branch/worktree: `rust-migration-base-bootstrap` @ `/var/mnt/xdata/code/_self/plasma-stats`
- Owned paths:
  - `scripts/capture-baseline.sh`
  - `plans/handoffs/base-p0.7-pages-20260719.md`
  - `plans/STATUS.md`
- Forbidden/shared paths:
  - `src/**`
  - `tests/**`
  - `tools/**`
  - `config/**`
  - `style/**`
  - `plans/LANES.md`
  - `plans/INVENTORY.md`
  - `README.md`
  - `AGENTS.md`
  - `packaging/**`
  - `install.sh`
  - `uninstall.sh`
  - `.gitignore` files
- Dependencies verified integrated:
  - existing `./pirostats render --page <name> --format html` CLI flow writing `/tmp/pirostats_render_tooltip.html`
  - existing `tools/qt_shot.py` evidence path via `.venv` `PyQt6`
  - existing `.test-artifacts/` local evidence bundle semantics

## Result

- Status: `handoff`
- Commits: none (working tree changes only)
- Changed files:
  - `scripts/capture-baseline.sh` — adds non-fatal deep-dive page capture for HTML logs/artifacts and optional Qt PNG evidence
  - `plans/handoffs/base-p0.7-pages-20260719.md` — this slice handoff
  - `plans/STATUS.md` — records new current-host evidence scope and remaining Phase 0 gap
- Behavior implemented/preserved:
  - current host now captures command logs plus copied HTML artifacts for tooltip pages `processes`, `cpu_cores`, `connections`, `fastfetch`, and `graphs`
  - when `PyQt6` is available, current host also captures PNG renders plus logs for those page HTML artifacts under `.test-artifacts/live-host-evidence/qt-shots/`
  - page-specific capture failures are recorded as non-fatal `SOFT_FAIL(...)`/`SKIP` summary entries instead of aborting the whole harness
  - pre-existing required baseline failure semantics remain intact for pytest, lint/dead-code sweeps, CLI smokes, metadata, main panel/tooltip HTML, and main Qt shots
- Explicitly not implemented:
  - no `src/**`, `tests/**`, or runtime behavior changes
  - no new multi-host or external-hardware evidence collection beyond current machine
  - no attempt to force page success when host dependencies/hardware are absent

## Parity evidence

- Current Python symbols/files covered:
  - `scripts/capture-baseline.sh`
  - current-host `./pirostats render --page ... --format html` page render path
  - current-host Qt evidence path through `tools/qt_shot.py`
- Oracle fixtures/cases: none (local evidence harness only)
- Exact differences remaining:
  - evidence still comes from one machine only
  - GPU/battery/HID/NVIDIA-specific coverage remains unavailable on this host and still requires different hardware/dependencies
  - page capture is intentionally observational; it records failures honestly instead of normalizing them away
- Inventory entries proposed resolved: none

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `bash -n scripts/capture-baseline.sh` | pass | Shell syntax clean after deep-dive capture additions. |
| `./scripts/capture-baseline.sh` | pass | `.test-artifacts/summary.txt` reports `required_failures=0`; page logs/HTML/PNG landed for `processes`, `cpu_cores`, `connections`, `fastfetch`, `graphs`. |
| `git diff --check` | pass | No whitespace or patch-format issues. |

## Dependencies and safety

- New/changed dependencies and review:
  - none in repo; reused existing local `.venv` with `PyQt6` already available
- Native/build/runtime requirements:
  - page HTML capture depends on existing `./pirostats render --page ... --format html` CLI behavior
  - page PNG evidence is opportunistic and only runs when `PyQt6` is importable via selected Python
  - page content quality still depends on host tools/hardware such as `fastfetch`, `ss`, network state, and GPU stack availability
- Unsafe/FFI locations and invariants:
  - none touched

## Risks/blockers

- Known risks:
  - `.test-artifacts/` remains host-specific and may contain environment details that should be reviewed before sharing
  - future hosts may soft-fail some page captures if page dependencies disappear or page rendering regresses
- Blocker requiring integration decision:
  - none for this slice; remaining blocker is still broader external-hardware/multi-host evidence collection
- Suggested next lane/API change:
  - if Phase 0 must close tighter, collect the same evidence bundle on hardware with battery/HID/NVIDIA variants; otherwise proceed to Phase 1 scaffold work

## Review notes

- Diff inspected for out-of-scope paths: yes
- Production runtime untouched by tests: yes
- No skipped/weakened checks: yes
- Rebase required before merge: no