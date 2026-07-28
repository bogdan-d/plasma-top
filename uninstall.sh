#!/usr/bin/env bash
# Reverse install.sh. User config remains untouched. Set DESTDIR to remove only
# a staged package tree without invoking user services or Plasma tools.
set -uo pipefail

APPLET_ID="com.github.lucazade.pirostats"
ROOT="${DESTDIR:-}"
ROOT="${ROOT%/}"

SUDO=""
if [ -z "$ROOT" ] && [ "$(id -u)" -ne 0 ]; then
	SUDO="sudo"
fi

if [ -z "$ROOT" ]; then
	systemctl --user disable --now pirostats 2>/dev/null || true
fi

$SUDO rm -f "$ROOT/usr/lib/systemd/user/pirostats.service"
$SUDO rm -f "$ROOT/usr/bin/pirostats"
$SUDO rm -rf "$ROOT/usr/lib/pirostats"
$SUDO rm -f "$ROOT/usr/share/icons/hicolor/scalable/apps/pirostats.svg"
$SUDO rm -rf "$ROOT/usr/share/licenses/pirostats"

if [ -n "$ROOT" ]; then
	$SUDO rm -rf "$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
	echo "PiroStats removed from $ROOT"
	exit 0
fi

if command -v kpackagetool6 >/dev/null; then
	$SUDO kpackagetool6 --type Plasma/Applet --global --remove "$APPLET_ID" 2>/dev/null || true
fi
systemctl --user daemon-reload 2>/dev/null || true

RUNTIME_DIR="${XDG_RUNTIME_DIR:+$XDG_RUNTIME_DIR/pirostats}"
rm -rf "${RUNTIME_DIR:-/tmp/pirostats-$(id -u)}" 2>/dev/null || true
rm -f /tmp/pirostats_* 2>/dev/null || true
rm -rf ~/.cache/pirostats 2>/dev/null || true

echo "PiroStats uninstalled. (Remove the widget from your panel if it is still there.)"
echo "Your config in ~/.config/pirostats was kept; delete it by hand if wanted."
