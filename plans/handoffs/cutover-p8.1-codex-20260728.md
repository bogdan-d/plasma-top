# CUTOVER P8.1 handoff — Codex — 2026-07-28

## Result

- User deferred unavailable Intel/NVIDIA/battery/HID and mutation-driven live
  validation until suitable hardware is available. Existing fixture proof is
  accepted for this cutover; the gaps remain documented and reopenable.
- Annotated rollback tag `pre-rust-cutover` points to `31ec788`.
- Root `./pirostats` now runs the locked Rust crate with repository assets.
- The former Python entry point is retained only as
  `tools/python_oracle.py`; production installers never package it.
- Manual and AUR packaging already installed only the Rust binary, and the
  systemd service already executed that packaged launcher.
- README, development setup, CI labels, baseline capture, lint/dead-code paths,
  and inventory accounting now distinguish production Rust from Python oracle.
- `./install.sh --user` and `./uninstall.sh --user` implement the planned
  immutable-host path with no sudo or `/usr` writes, a detached asset tree,
  user unit, installation-neutral applet actions, and repeatable upgrades.

## Verification

```bash
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps
.venv/bin/python -m pytest tests/ -q
.venv/bin/ruff check .
.venv/bin/vulture src/ tests/ tools/python_oracle.py tests/vulture_whitelist.py --min-confidence 60
./pirostats list-items
.venv/bin/python tools/python_oracle.py list-items
tools/p6_package_test.sh
tools/user_install_test.sh
```

Rust gates pass with 507 library and 26 integration tests. Python oracle passes
with 175 tests and one optional skip. Ruff, Vulture, and Rust/Python
`list-items` byte comparison pass.
Disposable native install/upgrade/Python rollback/uninstall/AUR packaging passes.
Disposable user-local install/upgrade/repeat-uninstall passes with spaced paths,
failure preservation, no sudo/global applet operations, and unit parity. Removal
hardening rejects root/traversal `DESTDIR`, root state directories, symlinked or
unowned install roots, unrelated same-named files, and unsafe temp cleanup;
diagnostic `/tmp` wildcards are not removed.

## Next gate

P8.2 stabilization window. Keep `src/`, Python tests, and
`tools/python_oracle.py` until explicit acceptance after that window.
