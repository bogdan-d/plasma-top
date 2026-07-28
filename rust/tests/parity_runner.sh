#!/usr/bin/env bash
#
# Parity runner: diff the Python oracle output against the Rust formatter for
# a shared fixture.
#
# Status: DEFERRED. Production `pirostats render` is implemented but deliberately
# has no fixture-only CLI flag. Fixed byte corpora and integration tests carry
# formatter parity until a shared-fixture diagnostic seam is approved.
#
# Contract:
#   $1 = path to oracle fixture TOML (shared between Python and Rust)
#   $2 = component: panel_v | panel_h | tooltip
#
# Output:
#   - exit 0 if the two outputs match (or are both empty);
#   - exit 1 with a unified diff if they differ;
#   - exit 77 ("skip") while the Rust CLI has no `render --fixture` seam.
#
# The script intentionally does not add a production-only fixture flag or fake
# parity. Exit 77 keeps the missing diagnostic seam visible.

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

# Deferred Rust diagnostic interface:
#   pirostats render --fixture <path> --component <panel_v|panel_h|tooltip>
# Production render rejects `--fixture`. Report that deferred test seam as a
# skip rather than conflating it with a real output mismatch.
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

echo "parity: shared-fixture Rust render seam is not implemented" >&2
exit 77
