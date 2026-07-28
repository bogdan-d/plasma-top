# Development setup

Rust is the production implementation. Python remains temporarily as the
behavioral oracle during stabilization.

## Requirements

- Rust 1.85+ with Cargo, rustfmt, and Clippy
- Python 3.11+ for oracle tests
- `pytest`, `ruff`, `vulture`, and `psutil` from `requirements-dev.txt`

## Setup

```bash
python3 -m venv .venv
source .venv/bin/activate
python3 -m pip install -r requirements-dev.txt
```

## Verification

```bash
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets --all-features
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps

python3 -m pytest tests/ -v
ruff check .
vulture src/ tests/ tools/python_oracle.py tests/vulture_whitelist.py --min-confidence 60
```

`./pirostats` runs the Rust checkout with repository assets:

```bash
./pirostats render
./pirostats probe --config config/config.toml
./pirostats list-items
```

Use `python3 tools/python_oracle.py ...` only for explicit compatibility
comparisons. Optional live oracle integrations such as PyGObject and pynvml are
not production dependencies.
