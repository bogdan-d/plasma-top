# Adopt Rust 1.97.1 and restore current syntax

Type: task
Status: ready-for-agent
Blocked by: 04

## Objective

Drop backward-compatibility work for Rust 1.87, set Rust 1.97.1 as the project minimum, and restore idiomatic current Rust syntax before further async scheduler work.

## Scope

- Raise the minimum toolchain from Rust 1.87 to exactly Rust 1.97.1 across Cargo metadata, `rust-toolchain.toml`, development and user documentation, packaging, CI, installer messages, repository gates, the async scheduler spec, and dependency/architecture evidence.
- Undo every compatibility-only rewrite from `5fbe6ce` that replaced stable current-Rust let chains with nested `if`/`if let` code for Rust 1.87. Restore let chains wherever the original condition remains clearer and behaviorally identical.
- Cover both issue-04 adapter code and pre-existing scheduler, notification, page-command, sensor, and test sites that were mechanically rewritten only to satisfy Rust 1.87.
- Remove obsolete Rust 1.87 validation instructions and assumptions. Do not preserve or test compatibility with older compilers.
- Keep all scheduler, sensor, command, D-Bus, notification, shutdown, and rendering behavior unchanged. Make no unrelated refactor or feature change.

## Acceptance

- `Cargo.toml` declares `rust-version = "1.97.1"`, repository toolchain and CI use 1.97.1, AUR requires `cargo>=1.97.1`, and no active project requirement still claims Rust 1.87 support.
- Every nested conditional introduced solely for the Rust 1.87 downgrade is restored to an equivalent let chain; nested conditionals with independent domain intent remain untouched.
- The diff contains no production behavior change beyond syntax and toolchain policy.
- Current Rust formatting, Clippy, tests, docs, packaging, and repository gates pass.

## Validation

Run a clean Rust 1.97.1 all-target/all-feature check, inspect the syntax-only source diff against `5fbe6ce`, and run the full gates from `docs/DEVELOPMENT.md`.
