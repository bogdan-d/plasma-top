# Handoff: `DAEMON-CLI` / `daemon-cli-codex-20260720`

## Contract

- Objective: compose production daemon lifecycle, diagnostics, and all CLI commands.
- Integration base SHA: `017a216` (collector commits `c1fb59e` / `017a216`).
- Branch/worktree: `rust-migration-base-bootstrap` at repository root.
- Owned paths: `rust/src/{adapters,daemon,diagnostics,cli,error,lib,main}.rs`, focused integration tests; integration-owned dependency/composition/status files.
- Forbidden paths preserved: QML, packaging, CSS, TOML schema, Python oracle, goldens.
- Dependencies verified integrated: all Phase 2–5 collector/backend lanes.

## Result

- Status: `handoff`, locally verified by integration owner.
- Commit: recorded by the finishing commit.
- Changed files: production boundary adapters; daemon/diagnostics/CLI/process entry; collector context lifetime split needed for persistent optional NVML/Bolt adapters; CLI/daemon integration tests; dependency ledger/lock; status.
- Behavior implemented/preserved: timeout-bound shell-free commands; blocking `busctl` system/session D-Bus translation; critical never-expiring notifications; dark/light KDE detection with file fallback; Qt-safe CSS/comment collapse and overlay; CSS/theme/config/machine/geometry reloads; last-good malformed reload; first paint with `skip_slow`; canonical width; poll work compensation; 100ms page wake and active process-page refresh; periodic peripheral rescan; NVML/Bolt production wiring; boot readiness logs; SIGINT/SIGTERM flag cleanup; page metadata/runtime cleanup; render/probe/profiling/list-items/page/click commands; argparse-compatible top/subcommand help and common error exits.
- Explicitly not implemented: QML/package cutover (Phase 6); live multi-hardware evidence (Phase 7).

## Parity evidence

- Python symbols covered: all `src/daemon.py` callables (`_css_path_for` through `main`) and root `pirostats` dispatch behavior.
- Deterministic evidence: CSS/RGB/D-Bus normalization units; 507 library tests; isolated two-poll daemon lifecycle with fake clock, absent command/D-Bus adapters, fixture proc/sys roots, first/normal publication observation, page-change wake, malformed hot reload retaining last good config, and cleanup; 3 process CLI tests plus existing runtime/config integration suites.
- Differential evidence: `list-items` byte diff is empty; top-level/render/daemon help byte diffs are empty; unknown-command and invalid-render-choice stderr/exit codes match argparse; live panel text differed only in simultaneous volatile CPU/temperature readings.
- Exact differences remaining: none in deterministic lane-owned output. Profiling timings and live sensor values are intentionally volatile under E3.
- Inventory entries proposed resolved: `src/daemon.py` file/callables, root `pirostats`, and Rust daemon/diagnostic/adapter/CLI entries.

## Validation

| Command | Result | Notes/artifact |
|---|---|---|
| `cargo fmt --manifest-path rust/Cargo.toml -- --check` | pass | clean |
| `cargo check --manifest-path rust/Cargo.toml --all-targets` | pass | default features |
| `cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings` | pass | includes NVML + test support |
| `cargo test --manifest-path rust/Cargo.toml --all-targets --all-features` | pass | 507 library + 26 integration = 533 tests |
| `cargo doc --manifest-path rust/Cargo.toml --no-deps` | pass | warning-free |
| `.venv/bin/python -m pytest tests/ -q` | pass | 175 passed, 1 optional skip |
| `.venv/bin/ruff check .` | pass | clean |
| `.venv/bin/vulture src/ tests/ pirostats tests/vulture_whitelist.py --min-confidence 60` | pass | clean |
| Python/Rust `list-items` + help/error diffs | pass | exact deterministic bytes/codes |
| Python/Rust live horizontal panel render | pass E3 | same structure/order; volatile CPU/temp values only |

## Dependencies and safety

- Added `wait-timeout 0.2`, `signal-hook 0.3`, and `serde_json 1`; full license/native/transitive/replacement review is in `rust/DEPENDENCIES.md`.
- Runtime commands: `busctl` for D-Bus and `notify-send` for notifications; both degrade through typed adapter errors. No shell expansion.
- Unsafe/FFI: no project unsafe code. Signal/syscall/native boundaries remain in reviewed safe crates.

## Risks/blockers

- Known risks: live D-Bus/NVML/Bolt/systemd/Plasma validation remains Phase 6/7; current host lacks supported Intel/NVIDIA/battery/Bolt hardware.
- Blocker requiring integration decision: none.
- Suggested next lane: QML-VERIFY and PACKAGING may begin from this commit.

## Review notes

- Diff inspected for out-of-scope paths: yes.
- Production runtime untouched by deterministic tests: yes; all daemon/CLI runtime tests use temp roots.
- No skipped/weakened checks: yes.
- Rebase required before merge: no.
