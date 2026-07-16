#!/usr/bin/env bash
# Reverses install.sh: stops the --user service, removes the system files under
# /usr (via sudo), the applet, and the icon, then clears the runtime files. Your
# ~/.config/pirostats is left alone. Remove the widget from your panel too
# (right-click → Remove).
set -uo pipefail

APPLET_ID="com.github.lucazade.pirostats"

SUDO=""
[ "$(id -u)" -ne 0 ] && SUDO="sudo"

systemctl --user disable --now pirostats 2>/dev/null || true

$SUDO rm -f /usr/lib/systemd/user/pirostats.service
$SUDO rm -f /usr/bin/pirostats
$SUDO rm -rf /usr/lib/pirostats
$SUDO rm -f /usr/share/icons/hicolor/scalable/apps/pirostats.svg
if command -v kpackagetool6 >/dev/null; then
	$SUDO kpackagetool6 --type Plasma/Applet --global --remove "$APPLET_ID" 2>/dev/null || true
fi

systemctl --user daemon-reload 2>/dev/null || true

# Runtime tree (panel/tooltip HTML, page counter, geometry). Mirrors runtime.py's
# _runtime_dir: $XDG_RUNTIME_DIR/pirostats, else /tmp/pirostats-$UID.
RUNTIME_DIR="${XDG_RUNTIME_DIR:+$XDG_RUNTIME_DIR/pirostats}"
rm -rf "${RUNTIME_DIR:-/tmp/pirostats-$(id -u)}" 2>/dev/null || true
# The one-shot render/profile files, which stay in /tmp on purpose — see runtime.py.
rm -f /tmp/pirostats_* 2>/dev/null || true
rm -rf ~/.cache/pirostats 2>/dev/null || true

echo "PiroStats uninstalled. (Remove the widget from your panel if it's still there.)"
echo "Your config in ~/.config/pirostats was kept; delete it by hand if you want it gone."
