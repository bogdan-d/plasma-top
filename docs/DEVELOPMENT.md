# Development setup

This is the baseline checkout setup for the Rust migration program's Phase 0
work. It documents the current Python runtime and dev/test dependencies without
changing production behavior.

## Requirements

- **Python 3.11+**
- **pip** for local package installation
- Optional but recommended: a **virtual environment** (`python3 -m venv`)

`./pirostats` runs directly from the repository checkout, but `psutil` is a real
runtime dependency today because `src/sensors.py` imports it unconditionally.

## Quick setup

From the repository root:

```bash
python3 -m venv .venv
source .venv/bin/activate
python3 -m pip install --upgrade pip
python3 -m pip install -r requirements-dev.txt
```

If you do not want a virtual environment, install the same requirements into the
Python interpreter you plan to use for local development.

## Recommended packaged-parity dependency

For full packaged behavior, install **PyGObject** (`gi`; commonly packaged as
`python-gobject`) through your distro package manager. It enables:

- desktop notifications
- UPower battery integration
- UDisks SMART integration

PyGObject is **not required** for the core pure-logic test suite, but it is the
closest match to the packaged runtime.

## Optional feature extras

These are not needed for the baseline test/lint loop, but they enable the same
feature set as the packaged install when the corresponding hardware or page is
present:

- `python-nvidia-ml-py` — preferred NVIDIA metrics path (NVML)
- `nvidia-utils` — provides `nvidia-smi`, used as the NVIDIA fallback path
- `iproute2` — the tooltip connections page (`ss`)
- `fastfetch` — the fastfetch tooltip page
- `hidapi` — Logitech Bolt/Unifying peripheral battery reads

## Verification

Run these from the repository root after installing the baseline requirements:

```bash
python3 -m pytest tests/ -v
ruff check .
vulture src/ tests/ pirostats tests/vulture_whitelist.py --min-confidence 60

./pirostats render
./pirostats probe --config config/config.toml
./pirostats list-items
```

The test/lint commands cover the baseline Python validation loop for the
migration program. The CLI commands are lightweight checkout smoke checks; some
hardware-specific rows or pages may still depend on the optional extras above.