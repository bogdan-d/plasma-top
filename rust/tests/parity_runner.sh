#!/usr/bin/env bash
#
# Parity runner: diff the Python oracle output against the Rust formatter for
# a shared fixture.
#
# Status: STUB. The Rust formatter is Wave 4 work (FORMATTER lane) and is not
# yet wired. This script fixes the contract and the eventual call shape so the
# integration owner can replace the Rust side in-place when Wave 4 lands.
#
# Contract:
#   $1 = path to oracle fixture TOML (shared between Python and Rust)
#   $2 = component: panel_v | panel_h | tooltip
#
# Output:
#   - exit 0 if the two outputs match (or are both empty);
#   - exit 1 with a unified diff if they differ;
#   - exit 77 ("skip") if the Rust binary does not yet implement `render`
#     (the current state — Wave 4 FORMATTER lane will close this).
#
# The script intentionally does NOT fake-implement parity: the failure path
# stays explicit so integration can see when Wave 4 needs to be wired.

set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "usage: $0 <fixture.toml> <panel_v|panel_h|tooltip>" >&2
    exit 2
fi

fixture="$1"
component="$2"

case "${component}" in
panel_v | panel_h | tooltip) ;;
*)
    echo "unknown component: ${component}" >&2
    exit 2
    ;;
esac

# Resolve the repo root from this script's location (rust/tests/parity_runner.sh).
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "${script_dir}/../.." && pwd)"
rust_dir="${repo_root}/rust"

py_out="$(mktemp)"
rs_out="$(mktemp)"
trap 'rm -f "${py_out}" "${rs_out}"' EXIT

# Run the Python oracle (tests/oracle.py). The oracle is the BASE lane's
# frozen contract: changes to its CLI require a BASE handoff update.
if ! python3 "${repo_root}/tests/oracle.py" "${fixture}" "${component}" >"${py_out}"; then
    echo "parity: Python oracle failed to render ${fixture} ${component}" >&2
    exit 1
fi

# Expected Rust interface (Wave 4 FORMATTER lane):
#   pirostats render --fixture <path> --component <panel_v|panel_h|tooltip>
# Until Wave 4 lands, the binary exits non-zero with an `Error::ScaffoldOnly`
# for `render`. We report parity as "not yet implemented" and exit 77 so any
# caller can detect the deferred state without conflating it with a real diff.
rust_bin="${rust_dir}/target/debug/pirostats"
if [[ ! -x "${rust_bin}" ]]; then
    rust_bin="${rust_dir}/target-p2-fixtures/debug/pirostats"
fi

if "${rust_bin}" render \
    --fixture "${fixture}" \
    --component "${component}" >"${rs_out}" 2>/dev/null; then
    if diff -u "${py_out}" "${rs_out}"; then
        if [[ -s "${py_out}" || -s "${rs_out}" ]]; then
            echo "parity: identical"
        else
            echo "parity: both empty"
        fi
        exit 0
    fi
    echo "parity: outputs differ" >&2
    exit 1
fi

echo "parity: Rust formatter not yet implemented (Wave 4 FORMATTER lane)" >&2
exit 77
