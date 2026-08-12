#!/usr/bin/env bash
set -euo pipefail

cat >&2 <<'EOF'
tools/plasma_live_matrix.sh is temporarily disabled because its plasmoidviewer integration is unreliable.
Use tools/qml_verify.sh for application-form checks. Panel and desktop representation checks require an explicitly approved real-session pass for now.
Repair is tracked in .scratch/plasmoidviewer-validation/issues/01-fix-plasmoidviewer-validation.md.
EOF
exit 1
