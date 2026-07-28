# Development setup

Rust is the sole runtime implementation.

## Requirements

- Rust 1.85+ with Cargo, rustfmt, and Clippy
- Python 3 with PyQt6 only for the optional Qt screenshot tools

## Verification

```bash
cargo fetch --locked --manifest-path rust/Cargo.toml
git diff --exit-code -- rust/Cargo.lock
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps
tools/repository_gate.sh
bash -n install.sh uninstall.sh packaging/aur/PKGBUILD \
  packaging/aur/pirostats.install pirostats tools/*.sh scripts/*.sh
tools/user_install_test.sh
tools/p6_package_test.sh
```

Run `tools/p6_qt_matrix.sh --no-build` on a host with PyQt6 and Qt SVG support
for render/QML changes or release verification. Run `tools/qml_verify.sh
--smoke` in Plasma when applet/runtime integration changes. Full interactive
Plasma checks remain environment-specific.

`./pirostats` runs the Rust checkout with repository assets:

```bash
./pirostats render
./pirostats probe --config config/config.toml
./pirostats list-items
```

`install.sh` and `packaging/aur/PKGBUILD` both build with `--locked`; package
launchers set `PIROSTATS_CODE_ROOT` and execute the installed native binary.
`tools/p6_package_test.sh` verifies their shared manifest, legacy-layout
upgrade, repeat upgrade, uninstall, and user-file preservation contracts.

Python is not a runtime, build, lint, or test dependency. The two retained Python
files, `tools/qt_shot.py` and `tools/p6_png_diff.py`, are source-independent Qt
developer tools. Fixed parity snapshots and fixtures live under `rust/tests/`.
