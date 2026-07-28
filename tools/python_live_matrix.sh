#!/usr/bin/env bash
# Run the Plasma live matrix against the Python daemon.
set -euo pipefail

tools_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$tools_dir/p6_live_matrix.sh" --python "$@"
