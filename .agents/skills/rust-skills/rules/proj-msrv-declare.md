# proj-msrv-declare

> Published libraries should declare and test an intentional MSRV; applications should pin their current toolchain

## Why It Matters

Published libraries need an explicit compatibility contract for downstream users. Setting `package.rust-version` causes Cargo to emit a clear, actionable error when the installed toolchain is too old, instead of a cryptic type or feature error deep inside your code. Resolver 3 is MSRV-aware: it can avoid selecting dependency versions whose own `rust-version` exceeds yours. Edition 2024 packages use resolver 3 by default; virtual workspaces should declare `resolver = "3"` explicitly.

## Bad

```toml
[package]
name = "my-crate"
version = "0.1.0"
edition = "2021"
# no rust-version — users get cryptic errors on old toolchains,
# and nothing prevents a dep bump from silently raising the floor
```

## Published Libraries: Declare and Test a Compatibility Floor

```toml
[package]
name = "my-crate"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"  # intentional compatibility floor; edition 2024 requires at least 1.85

[workspace]
resolver = "3"  # default for edition 2024; enables MSRV-aware dep resolution
```

CI job pinning the MSRV toolchain (GitHub Actions example):

```yaml
# .github/workflows/msrv.yml
- name: Install MSRV toolchain
  uses: dtolnay/rust-toolchain@master
  with:
    toolchain: "1.85"

- name: Check MSRV
  run: cargo check --all-features
```

## Applications: Pin the Current Toolchain

Applications normally control their deployment environment, so they should use current idiomatic syntax rather than carry old-compatibility constraints. Pin the toolchain in source and CI:

```toml
# Cargo.toml
[package]
name = "my-app"
version = "0.1.0"
edition = "2024"
rust-version = "1.97.1"
```

```toml
# rust-toolchain.toml
[toolchain]
channel = "1.97.1"
```

```yaml
# .github/workflows/ci.yml
- uses: dtolnay/rust-toolchain@1.97.1
- run: cargo check
```

Do not make `cargo-msrv` a default application policy; use it when maintaining a published library's compatibility floor.

## Choosing and Maintaining a Library MSRV

- Pick the oldest stable toolchain your users are realistically running (check distro packages, embedded targets, corporate freeze windows).
- Do not go lower than needed — a lower MSRV widens the set of eligible dependency versions and can force you onto older, buggier releases.
- When you bump MSRV, treat it as a semver-minor change (for libraries) and document it in your changelog.
- Run `cargo msrv` (the `cargo-msrv` tool) to find the actual floor automatically.

## See Also

- [proj-workspace-deps](proj-workspace-deps.md) - use workspace dependency inheritance
- [lint-cargo-metadata](lint-cargo-metadata.md) - warn on missing Cargo.toml metadata
- [doc-cargo-metadata](doc-cargo-metadata.md) - fill Cargo.toml metadata fields
