#!/usr/bin/env bash
# System-wide install matching the distro-package layout. Set DESTDIR to stage
# the package without touching the host; user-service/applet actions are skipped.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPLET_ID="com.github.lucazade.pirostats"
ROOT="${DESTDIR:-}"
ROOT="${ROOT%/}"
LIBDIR="$ROOT/usr/lib/pirostats"

SUDO=""
if [ -z "$ROOT" ] && [ "$(id -u)" -ne 0 ]; then
	SUDO="sudo"
fi

if [ -n "${PIROSTATS_BINARY:-}" ]; then
	BINARY="$PIROSTATS_BINARY"
else
	command -v cargo >/dev/null || {
		echo "[error] cargo not found — Rust 1.85+ is required." >&2
		exit 1
	}
	CARGO_TARGET_DIR="$REPO_DIR/rust/target" \
		cargo build --manifest-path "$REPO_DIR/rust/Cargo.toml" --release --locked --features nvml
	BINARY="$REPO_DIR/rust/target/release/pirostats"
fi

[ -x "$BINARY" ] || {
	echo "[error] Rust binary not found or not executable: $BINARY" >&2
	exit 1
}

if [ -z "$ROOT" ] && ! command -v kpackagetool6 >/dev/null; then
	echo "[error] kpackagetool6 not found — Plasma 6 is required." >&2
	exit 1
fi

# Build succeeds before the installed tree is replaced, keeping failed upgrades
# on the previous working version.
$SUDO rm -rf "$LIBDIR"
$SUDO install -d "$LIBDIR"
$SUDO cp -r "$REPO_DIR/style" "$REPO_DIR/lang" "$REPO_DIR/config" "$LIBDIR/"
$SUDO install -m755 "$BINARY" "$LIBDIR/pirostats"
$SUDO install -Dm755 "$REPO_DIR/packaging/pirostats-launcher" "$ROOT/usr/bin/pirostats"
$SUDO install -Dm644 "$REPO_DIR/service/pirostats.service" \
	"$ROOT/usr/lib/systemd/user/pirostats.service"
$SUDO install -Dm644 "$REPO_DIR/plasmoid/package/contents/icons/pirostats.svg" \
	"$ROOT/usr/share/icons/hicolor/scalable/apps/pirostats.svg"
$SUDO install -Dm644 "$REPO_DIR/LICENSE" "$ROOT/usr/share/licenses/pirostats/LICENSE"
$SUDO install -Dm644 "$REPO_DIR/NOTICE" "$ROOT/usr/share/licenses/pirostats/NOTICE"

if [ -n "$ROOT" ]; then
	APPLET_DIR="$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
	$SUDO rm -rf "$APPLET_DIR"
	$SUDO install -d "$APPLET_DIR"
	$SUDO cp -r "$REPO_DIR/plasmoid/package/." "$APPLET_DIR/"
	echo "PiroStats staged under $ROOT"
	exit 0
fi

if kpackagetool6 --type Plasma/Applet --global --show "$APPLET_ID" >/dev/null 2>&1; then
	$SUDO kpackagetool6 --type Plasma/Applet --global --upgrade "$REPO_DIR/plasmoid/package"
	applet_upgraded=true
else
	$SUDO kpackagetool6 --type Plasma/Applet --global --install "$REPO_DIR/plasmoid/package"
	applet_upgraded=false
fi

command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
systemctl --user daemon-reload
systemctl --user enable pirostats
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
	echo "(right-click the panel -> Add Widgets -> search 'PiroStats')."
fi
