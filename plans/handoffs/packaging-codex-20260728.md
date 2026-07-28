# PACKAGING handoff — Codex — 2026-07-28

## Scope

- Lane: `PACKAGING`, Phase 6 P6.4 plus disposable P6.5 evidence.
- Base: `6ffba34`.
- No system directory, package-manager, systemd, or global Plasma write occurred.
- Validation wrote only inside the repository and `/tmp`, per host constraint.

## Changed paths

- `install.sh`: locked release Rust build with `nvml`, native FHS tree,
  pre-replacement build failure safety, and `DESTDIR` staging mode.
- `uninstall.sh`: native manifest removal and `DESTDIR` staging mode.
- `packaging/pirostats-launcher`: sets `PIROSTATS_CODE_ROOT` for packaged assets
  and replaces itself with the sole Rust binary.
- `packaging/aur/PKGBUILD`: concrete `x86_64`, locked native build, no Python
  runtime dependencies, corrected optional tools, native manifest.
- `packaging/aur/pirostats.install`: native optional-tool guidance.
- `tools/p6_package_test.sh`: disposable install/upgrade/Python-rollback/
  uninstall and AUR `package()` manifest gate.
- `plans/{STATUS,INVENTORY}.md`: lane evidence and remaining live blocker.

## Verification

```text
bash -n install.sh uninstall.sh tools/p6_package_test.sh \
  packaging/aur/PKGBUILD packaging/aur/pirostats.install \
  packaging/pirostats-launcher
tools/p6_package_test.sh
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps
.venv/bin/python -m pytest tests/ -q
.venv/bin/ruff check .
.venv/bin/vulture src/ tests/ pirostats tests/vulture_whitelist.py --min-confidence 60
```

Results:

- Native release build passed with committed lockfile and `nvml` feature.
- Disposable manual install, repeat upgrade, Python rollback, native reinstall,
  and uninstall passed under `/tmp`; config and cache fixtures survived.
- AUR `package()` produced the expected native tree under `/tmp`; no Python
  source/runtime dependency was packaged; LICENSE and NOTICE were present.
- Rust aggregate passed: 507 library + 26 integration tests.
- Python oracle passed: 175 passed + 1 optional skip; ruff/vulture green.

## Remaining evidence

- P6.5 real package-manager install/upgrade/downgrade/uninstall and user-systemd
  lifecycle require a disposable Arch Plasma VM/session. Current immutable,
  non-Arch host explicitly forbids system-folder writes, so this was not run.
- `x86_64` is the only declared AUR architecture because it is the only tested
  native package target. Add another concrete architecture only with build/live
  evidence.
