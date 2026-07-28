# CUTOVER P8.5 handoff — Codex — 2026-07-29

## Result

P8.5 and the Rust migration final gate are complete. Rust remains the sole
runtime implementation. Migration-only Python tests, oracle bridges, previews,
inventory tooling, baseline capture, and the exit-77 parity stub are removed.
Useful fixed parity evidence now lives under `rust/tests/`. No production Rust,
QML behavior, service, installer, package, CLI, or runtime contract changed.

PM001 was not investigated or modified.

## Baseline and rollback

- Started from clean `5cc8e12` on `rust-migration-base-bootstrap`.
- Active repository override: stable Rust 1.97.1, Cargo 1.97.1.
- Annotated tag object `bf5829a` (`pre-rust-cutover`) is unchanged and
  dereferences to commit `31ec788`.
- P8.5 infrastructure/evidence commit: `2154e7c`.

## Final pre-removal inventory

Before deletion, retained migration material was re-inventoried:

- 15 Python files under `tests/`, including oracle/inventory/lint/dead-code
  harnesses and 11 behavior suites;
- 6 Python developer tools: source-coupled oracle, AST reporter, and two
  previews; source-independent Qt renderer and PNG comparator;
- 3 HTML goldens plus one duplicated oracle TOML fixture;
- `rust/tests/parity_runner.sh` and `scripts/capture-baseline.sh`;
- migration plans/handoffs and 34 unchecked `INVENTORY.md` rows.

The source-independent `tools/qt_shot.py` and `tools/p6_png_diff.py` remain.
Migration plans and the frozen Python callable/call-edge ledger remain as
historical evidence. Generated caches were not retained.

## Evidence promotion and runner decision

- Moved panel H, panel V, and tooltip byte snapshots to `rust/tests/golden/`.
  `tooltip_and_panel_goldens_match_python_snapshots` reads all three directly.
- Kept the byte-identical full oracle TOML under
  `rust/tests/fixtures/oracle/`; `FixtureLoader` covers valid, missing,
  malformed, and missing-table cases.
- Existing Rust `full_hw`/`full_readings` formatter corpus renders the exact
  promoted snapshots. Sensor, adapter, collector, daemon, CLI, package, Qt, and
  live suites retain broader behavioral evidence formerly exercised by Python.
- Retired `rust/tests/parity_runner.sh`. Implementing it required adding a
  fixture-only production CLI seam solely for a dead Python differential tool.
  Existing exact corpora and integration/live evidence provide closure without
  new production surface. No exit-77 stub remains.

No unique behavioral evidence was discarded.

## Current inventory enforcement

The Python AST reporter tested deleted source, so it was retired with its test.
`tools/repository_gate.sh` now checks current concerns only:

- exact allowlist for source-independent Python tools;
- absence of retired runtime/migration paths;
- required Rust, parity, launcher, service, installer, and package files;
- no Python runtime references on production launch/package/QML/CI surfaces;
- exact system/user service and package launcher contracts;
- AUR Rust build dependencies;
- canonical-width and table-free render assertions.

Compiler reachability, Clippy `-D warnings`, Rust tests, package manifests, and
Qt integration provide callable/dead-path closure. `plans/INVENTORY.md` has zero
unchecked rows and retains exact row-level dispositions.

## Removed and retained material

Removed:

- `tests/` Python suites, duplicated fixture, and old golden location;
- `tools/{python_oracle,inventory_ast_reporter,demo_shot,
  manual_tooltip_preview}.py`;
- `rust/tests/parity_runner.sh`;
- `scripts/capture-baseline.sh`;
- migration-only target-directory ignore and stale source-path documentation.

Retained:

- all Rust production/tests/fixtures and promoted fixed snapshots;
- source-independent Qt tools and Rust/Qt/package lifecycle scripts;
- plans, handoffs, accepted deviations, rollback documentation, screenshots,
  and Phase 7/P8 live evidence;
- unchanged `pre-rust-cutover` tag.

## Documentation and CI

Refreshed `README.md`, `docs/{DESIGN,DEVELOPMENT,PERFORMANCE}.md`, `AGENTS.md`,
CSS ownership comments, Cargo dependency comments, and CI. README/development
docs now describe native package contracts and Rust-only gates. Plans are
clearly historical. CI runs locked Rust gates, repository/shell closure,
user-install lifecycle, and native package manifest lifecycle.

## Final verification

All commands passed after P8.5 changes:

```text
cargo fetch --locked --manifest-path rust/Cargo.toml
git diff --exit-code -- rust/Cargo.lock
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps
tools/repository_gate.sh
bash -n install.sh uninstall.sh packaging/aur/PKGBUILD packaging/aur/pirostats.install pirostats tools/*.sh scripts/*.sh
tools/user_install_test.sh
tools/p6_package_test.sh
tools/p6_qt_matrix.sh --no-build
git diff --check
```

- Rust: 507 library + 26 integration tests passed (533 total).
- User-local clean install, upgrade, repeat uninstall, and config preservation
  passed.
- Native clean/legacy upgrade, repeat upgrade, uninstall, user-file, and AUR
  manifest checks passed.
- Qt RichText matrix passed all 24 panel/page/theme cells.
- Repository gate, shell syntax, inventory closure, dead-path audit, lockfile,
  and rollback-tag checks passed.
- D005 and D006 remain accepted and unchanged.

The repository is ready for normal Rust-only development. PM001 is the next
separate post-migration issue if chosen.
