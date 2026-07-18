# Handoff: `BASE` / `base-copilot-20260719`

## Contract

- Objective: Implement Phase 0 step P0.2 only: add reproducible Python checkout/dev-test setup docs and clarify current required vs optional Python dependencies without changing runtime behavior.
- Integration base SHA: `cba9a33390de836348dd7bb6802a9869d3442ee5`
- Branch/worktree: `rust-migration-base-bootstrap` @ `/var/mnt/xdata/code/_self/plasma-stats`
- Owned paths:
  - `README.md`
  - `docs/DEVELOPMENT.md`
  - `requirements-dev.txt`
  - `plans/handoffs/base-copilot-20260719.md`
- Forbidden/shared paths:
  - `src/**`
  - `tests/**`
  - `config/**`
  - `style/**`
  - `plans/STATUS.md`
  - `plans/LANES.md`
  - `AGENTS.md`
  - `packaging/**`
  - `install.sh`
  - `uninstall.sh`
- Dependencies verified integrated: none; this BASE slice has no prior lane dependency.

## Result

- Status: `handoff`
- Commits: none (working tree changes only)
- Changed files:
  - `README.md` — clarifies required base runtime dependencies vs optional feature extras and points checkout developers to `docs/DEVELOPMENT.md`
  - `docs/DEVELOPMENT.md` — baseline checkout/dev-test setup for the migration program
  - `requirements-dev.txt` — minimal pip install set for checkout runtime + current test/lint tools
  - `plans/handoffs/base-copilot-20260719.md` — this handoff
- Behavior implemented/preserved:
  - documented `psutil` as required today because `src/sensors.py` imports it unconditionally
  - documented `pytest`, `ruff`, and `vulture` installation for checkout validation
  - documented PyGObject as the packaged-parity dependency for notifications plus UPower/UDisks integration, while noting that the core pure-logic tests do not require it
  - documented optional extras for NVIDIA, the connections page, the fastfetch page, and HID batteries
  - preserved production Python behavior, defaults, config semantics, and golden HTML snapshots (no code or test files changed)
- Explicitly not implemented:
  - no runtime dependency behavior changes or fallbacks
  - no install or packaging script changes
  - no plan status updates
  - no local dependency installation in this environment

## Parity evidence

- Current Python symbols/files covered:
  - `src/sensors.py` unconditional `import psutil`
  - `src/notifier.py` opportunistic `gi`/PyGObject notification path
  - `packaging/aur/PKGBUILD` base dependencies: `python`, `python-psutil`, `python-gobject`, `plasma-workspace`
  - `packaging/aur/PKGBUILD` optional dependencies: `python-nvidia-ml-py`, `nvidia-utils`, `iproute2`, `fastfetch`, `hidapi`
- Oracle fixtures/cases: none added (out of scope for P0.2)
- Exact differences remaining:
  - none for the documented baseline setup after creating an isolated `.venv` and installing `requirements-dev.txt`
- Inventory entries proposed resolved: none

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `python3 -m venv .venv && . .venv/bin/activate && python -m pip install --upgrade pip && python -m pip install -r requirements-dev.txt` | pass | Isolated baseline environment created successfully; installed `psutil`, `pytest`, `ruff`, and `vulture`. |
| `. .venv/bin/activate && python -m pytest tests/ -v` | pass | Full Python suite passed: `173 passed`. |
| `. .venv/bin/activate && ruff check .` | pass | Lint gate passed in the documented environment. |
| `. .venv/bin/activate && vulture src/ tests/ pirostats tests/vulture_whitelist.py --min-confidence 60` | pass | Dead-code gate passed in the documented environment. |
| `. .venv/bin/activate && ./pirostats render >/tmp/pirostats_render_smoke.txt` | pass | Checkout render smoke succeeded in the documented environment. |
| `. .venv/bin/activate && ./pirostats list-items >/tmp/pirostats_list_items_smoke.txt` | pass | Checkout token-list smoke succeeded in the documented environment. |
| `. .venv/bin/activate && ./pirostats probe --config config/config.toml >/tmp/pirostats_probe_smoke.txt` | pass | Checkout probe smoke succeeded in the documented environment. |
| `git diff --check` | pass | No whitespace or patch-format issues in the scoped doc changes. |
| `python3 -c "import sys, importlib.util as u; ..."` | pass | Pre-setup baseline on this host: Python `3.14.6`; `psutil=False`; `pytest=False`; `ruff=False`; `vulture=False`. |
| `git status --short` | pass | Final snapshot shows only the intended scoped files: `README.md`, `docs/DEVELOPMENT.md`, `requirements-dev.txt`, and this handoff. |

## Dependencies and safety

- New/changed dependencies and review:
  - Added `requirements-dev.txt` with `psutil`, `pytest`, `ruff`, and `vulture` only.
  - Left PyGObject out of the pip manifest on purpose because packaged-parity installation is typically distro-managed alongside GI libraries/typelibs.
- Native/build/runtime requirements:
  - Runtime today still requires Python `3.11+` and `psutil`.
  - The documented checkout validation now works in an isolated `.venv` created during this handoff.
  - Full packaged parity also expects PyGObject / `python-gobject`.
  - Optional feature extras remain external: `python-nvidia-ml-py`, `nvidia-utils`, `iproute2`, `fastfetch`, `hidapi`.
- Unsafe/FFI locations and invariants: none touched.

## Risks/blockers

- Known risks:
  - `requirements-dev.txt` is a baseline manifest, not a locked environment file; exact versions remain whatever pip resolves at install time.
  - The `.venv` validation proves the checkout workflow, but not full packaged parity with distro-managed PyGObject/GI typelibs.
- Blocker requiring integration decision:
  - none for this slice
- Suggested next lane/API change:
  - Continue with P0.3 by capturing and storing the validated Python baseline outputs/artifacts, then begin fixture/oracle work for P0.4/P0.5.

## Review notes

- Diff inspected for out-of-scope paths: yes
- Production runtime untouched by tests: yes
- No skipped/weakened checks: yes
- Rebase required before merge: no
