# Development setup

Rust is the sole runtime implementation.

## Requirements

- Rust 1.85+ with Cargo, rustfmt, and Clippy
- Python 3 with PyQt6 only for the optional Qt screenshot tools

## Verification

```bash
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps
tools/user_install_test.sh
tools/p6_package_test.sh
```

`./pirostats` runs the Rust checkout with repository assets:

```bash
./pirostats render
./pirostats probe --config config/config.toml
./pirostats list-items
```

The former Python pytest, Ruff, and Vulture gates were retired in P8.4 when the
Python runtime source was removed. Their fixtures and migration evidence remain
until the P8.5 archive pass.
