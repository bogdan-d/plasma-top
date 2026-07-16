#!/usr/bin/env bash
# System-wide install of PiroStats, mirroring the distro-package layout:
#   /usr/lib/pirostats/            code + shipped defaults (read-only)
#   /usr/bin/pirostats             the CLI (symlink; resolves the tree beside it)
#   /usr/lib/systemd/user/         the --user service unit
#   /usr/share/plasma/plasmoids/   the applet (global)
#   /usr/share/icons/...           the applet icon
# Placing files under /usr needs root, so the copy steps run via sudo; enabling
# the --user service is per-user and runs as you. Your customization lives in
# ~/.config/pirostats and is never touched here. Re-run to upgrade.
#
# This is the manual, non-AUR channel; the AUR PKGBUILD produces the same layout.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPLET_ID="com.github.lucazade.pirostats"
LIBDIR="/usr/lib/pirostats"

SUDO=""
[ "$(id -u)" -ne 0 ] && SUDO="sudo"

if ! command -v kpackagetool6 >/dev/null; then
	echo "[error] kpackagetool6 not found — Plasma 6 is required." >&2
	exit 1
fi

# ── code tree + CLI (root) ────────────────────────────────────────────────────
$SUDO rm -rf "$LIBDIR"
$SUDO install -d "$LIBDIR"
$SUDO cp -r "$REPO_DIR/src" "$REPO_DIR/style" "$REPO_DIR/lang" "$REPO_DIR/config" "$LIBDIR/"
$SUDO install -m755 "$REPO_DIR/pirostats" "$LIBDIR/pirostats"
$SUDO find "$LIBDIR" -name __pycache__ -type d -prune -exec rm -rf {} +
$SUDO ln -sf "$LIBDIR/pirostats" /usr/bin/pirostats

# ── systemd --user unit + icon (root) ─────────────────────────────────────────
$SUDO install -Dm644 "$REPO_DIR/service/pirostats.service" /usr/lib/systemd/user/pirostats.service
$SUDO install -Dm644 "$REPO_DIR/plasmoid/package/contents/icons/pirostats.svg" \
	/usr/share/icons/hicolor/scalable/apps/pirostats.svg

# ── applet, global (root) ─────────────────────────────────────────────────────
if kpackagetool6 --type Plasma/Applet --global --show "$APPLET_ID" >/dev/null 2>&1; then
	$SUDO kpackagetool6 --type Plasma/Applet --global --upgrade "$REPO_DIR/plasmoid/package"
	applet_upgraded=true
else
	$SUDO kpackagetool6 --type Plasma/Applet --global --install "$REPO_DIR/plasmoid/package"
	applet_upgraded=false
fi

# ── enable the service + refresh the widget (per-user, no sudo) ────────────────
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
systemctl --user daemon-reload
systemctl --user enable pirostats
# restart (not enable --now): if a previous version is already running, --now
# wouldn't restart it and the old code would stay in RAM.
systemctl --user restart pirostats

if [ "$applet_upgraded" = true ] && command -v kstart >/dev/null; then
	killall plasmashell 2>/dev/null || true
	kstart plasmashell >/dev/null 2>&1 &
fi

echo
echo "PiroStats installed system-wide. Service status:"
systemctl --user status pirostats --no-pager || true
if [ "$applet_upgraded" = false ]; then
	echo
	echo "First install: add the 'PiroStats' widget to a panel"
	echo "(right-click the panel → Add Widgets → search 'PiroStats')."
fi
