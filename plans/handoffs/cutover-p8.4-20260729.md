# CUTOVER P8.4 handoff — Codex — 2026-07-29

## Result

P8.4 is complete. Commit `ba9b507` removes the former Python production runtime,
its obsolete live launch path, oracle-only dependency manifest, Ruff config, and
Python CI job. Rust remains the sole production implementation. Installers,
services, checkout/package launchers, applet actions, and AUR manifests remain
Rust-only and pass their applicable gates.

P8.5 did not start. Oracle fixtures/tests, `tools/python_oracle.py`, preview
tools, migration evidence, `rust/tests/parity_runner.sh`, stronger inventory
closure, and PM001 retain their deferred dispositions.

## Baseline and rollback

- Started from a clean tree at requested commit `be4fd96` on
  `rust-migration-base-bootstrap`.
- Active tools: Cargo 1.97.1 and Rust 1.97.1; repository stable override active.
- Annotated tag object `bf5829a` (`pre-rust-cutover`) remains unchanged and
  dereferences to commit `31ec788`.
- The tag, not a dead checkout Python launcher, is the supported source rollback
  point. Previously verified package/user-file rollback evidence remains in the
  P6/P8.1 handoffs.

## Pre-deletion inventory

The tracked Python inventory contained 40 files:

- **19 former production runtime files:** every file under `src/`; all removed.
- **15 tests/migration-evidence files:** `tests/*.py`; retained for P8.5
  disposition, but no longer an executable Python-runtime gate.
- **6 developer tools:** `tools/{demo_shot,inventory_ast_reporter,
  manual_tooltip_preview,p6_png_diff,python_oracle,qt_shot}.py`; retained.
  The oracle and two previews still reference the removed source and are
  deliberately deferred for P8.5 archive/disposition. The inventory reporter
  remains migration evidence; the Qt tools are source-independent.

Other source-coupled migration evidence was also identified before deletion:
`scripts/capture-baseline.sh`, `tests/oracle.py`,
`rust/tests/parity_runner.sh`, and the frozen call-edge table. None is installed
or production-reachable. P8.5 owns their archive/adaptation decisions.

`requirements-dev.txt` contained `psutil`, pytest, Ruff, and Vulture and stated
that none was a production dependency. `psutil` was used only by the removed
runtime/oracle; the other entries implemented retired Python gates. Production
package dependencies were already native-only (`plasma-workspace`, optional
command providers, and Rust build dependencies), so no AUR runtime dependency
needed removal.

`plans/INVENTORY.md` now records each removed file's final P8.4 disposition and
the exact mapped Rust evidence. Every removed production callable was already
checked; its line/call-edge data remains as a frozen pre-removal ledger.

## Changes

- Deleted all 19 `src/*.py` files, `requirements-dev.txt`, `ruff.toml`, and
  `tools/python_live_matrix.sh`.
- Removed the Python oracle CI job; retained locked Rust
  fmt/check/clippy/test/doc and user-install CI.
- Made `tools/p6_live_matrix.sh` Rust-only.
- Kept the Qt matrix functional by retaining its Rust golden pre-gate and
  retiring only the deleted-source Python pytest pre-gate.
- Replaced package-test staging of the Python tree with a source-independent
  legacy-layout upgrade fixture. Native install, repeat upgrade, uninstall,
  user-file preservation, and AUR manifest checks remain covered.
- Updated current agent/development/design docs, QML source-path comments, and
  CSS comments to describe Rust ownership.

No Rust production code, test, fixture, Cargo dependency, lockfile, applet
behavior, service command, installer command, or package manifest was removed or
weakened.

## Retired Python gates

The full Python pytest, Ruff, Vulture, and Python AST inventory-sync gates were
retired by P8.4 because their subject—the `src/` runtime—was removed. They were
last green at P8.2 (175 passed, one optional skip; Ruff and Vulture passed).
Running them after deletion would test missing historical source, not the shipped
product. Their tests, fixtures, goldens, reporter, and frozen inventory evidence
remain for P8.5; no silent skip or replacement claim is made.

Python remains an implementation language for optional source-independent Qt
developer tools. It is not a PiroStats runtime or package dependency.

## Verification

All applicable gates passed after source removal:

```text
bash -n install.sh uninstall.sh tools/*.sh scripts/*.sh rust/tests/*.sh
cargo fetch --locked --manifest-path rust/Cargo.toml
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps
tools/user_install_test.sh
tools/p6_package_test.sh
tools/p6_qt_matrix.sh --no-build
git diff --exit-code -- rust/Cargo.lock
git diff --check
```

- Rust: 507 library + 26 integration tests passed (533 total).
- User-local clean install/upgrade/repeat uninstall/config preservation passed.
- Native legacy-layout upgrade, repeat upgrade, uninstall, user-file, and AUR
  manifest checks passed.
- Qt RichText matrix passed all 24 panel/page/theme cells with Rust golden and
  table-free pre-gates.
- `src/` is absent; the Cargo lockfile is unchanged; the rollback tag resolves
  to the expected pre-cutover commit.

The accepted P8.2 live Plasma evidence remains valid because P8.4 changed no Rust
or executable QML behavior. D005 and D006 remain unchanged.

## Next boundary

P8.5 remains next: decide the parity runner, strengthen inventory closure,
promote/archive retained oracle tests/fixtures/tools and migration plans, and
perform the final documentation cleanup. PM001 remains deferred as requested.
