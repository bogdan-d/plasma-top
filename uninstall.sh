#!/usr/bin/env bash
# Remove shipped files while preserving user configuration.
set -uo pipefail

usage() {
	printf 'Usage: ./uninstall.sh [--user]\n'
}

canonical_path() {
	realpath -m -- "$1"
}

MODE=system
case "${1:-}" in
	"") ;;
	--user) MODE=user ;;
	-h|--help) usage; exit 0 ;;
	*) echo "[error] unknown argument: $1" >&2; usage >&2; exit 2 ;;
esac
[[ $# -le 1 ]] || { echo "[error] expected at most one argument" >&2; exit 2; }
if [[ "$MODE" == user && -n "${DESTDIR:-}" ]]; then
	echo "[error] DESTDIR cannot be combined with --user" >&2
	exit 2
fi

APPLET_ID="com.github.lucazade.pirostats"

prepare_runtime_cache() {
	local runtime_base runtime_dir cache_base cache_dir
	if [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
		[[ "$XDG_RUNTIME_DIR" == /* ]] || {
			echo "[error] refusing unsafe XDG_RUNTIME_DIR: ${XDG_RUNTIME_DIR:-<empty>}" >&2
			return 1
		}
		runtime_base="$(canonical_path "$XDG_RUNTIME_DIR")"
		[[ "$runtime_base" != / ]] || {
			echo "[error] XDG_RUNTIME_DIR resolves to /" >&2
			return 1
		}
		runtime_dir="$runtime_base/pirostats"
	else
		runtime_dir="/tmp/pirostats-$(id -u)"
	fi
	if [[ -n "${XDG_CACHE_HOME:-}" ]]; then
		cache_base="$XDG_CACHE_HOME"
	else
		[[ "${HOME:-}" == /* && "$HOME" != / ]] || {
			echo "[error] refusing unsafe HOME for cache cleanup: ${HOME:-<empty>}" >&2
			return 1
		}
		cache_base="$HOME/.cache"
	fi
	[[ "$cache_base" == /* ]] || {
		echo "[error] refusing unsafe cache root: $cache_base" >&2
		return 1
	}
	cache_base="$(canonical_path "$cache_base")"
	[[ "$cache_base" != / ]] || {
		echo "[error] cache root resolves to /" >&2
		return 1
	}
	cache_dir="$cache_base/pirostats"
	RUNTIME_REMOVE="$runtime_dir"
	CACHE_REMOVE="$cache_dir"
}

remove_runtime_cache() {
	rm -rf -- "$RUNTIME_REMOVE" "$CACHE_REMOVE" 2>/dev/null || true
}

if [[ "$MODE" == user ]]; then
	[[ "${HOME:-}" == /* && "$HOME" != / ]] || {
		echo "[error] --user requires an absolute, non-root HOME" >&2
		exit 2
	}
	HOME="$(canonical_path "$HOME")"
	[[ "$HOME" != / ]] || { echo "[error] HOME resolves to /" >&2; exit 2; }
	DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
	[[ "$DATA_HOME" == /* ]] || {
		echo "[error] XDG_DATA_HOME must be an absolute directory below a data root" >&2
		exit 2
	}
	DATA_HOME="$(canonical_path "$DATA_HOME")"
	[[ "$DATA_HOME" != / && "$DATA_HOME" != "$HOME" \
		&& "$DATA_HOME" != /usr && "$DATA_HOME" != /usr/* ]] || {
		echo "[error] refusing unsafe XDG_DATA_HOME: $DATA_HOME" >&2
		exit 2
	}
	LIBDIR="$DATA_HOME/pirostats"
	if [[ -L "$LIBDIR" ]]; then
		echo "[error] refusing symlinked install root: $LIBDIR" >&2
		exit 1
	fi
	if [[ ! -f "$LIBDIR/.pirostats-install" ]]; then
		echo "No owned user-local PiroStats install found under $DATA_HOME."
		exit 0
	fi
	prepare_runtime_cache || exit 2

	systemctl --user disable --now pirostats 2>/dev/null || true
	if command -v kpackagetool6 >/dev/null; then
		kpackagetool6 --type Plasma/Applet --remove "$APPLET_ID" 2>/dev/null || true
	fi
	rm -f -- "$HOME/.local/bin/pirostats"
	rm -rf -- "$LIBDIR"
	rm -f -- "$DATA_HOME/systemd/user/pirostats.service"
	rm -f -- "$DATA_HOME/icons/hicolor/scalable/apps/pirostats.svg"
	rm -rf -- "$DATA_HOME/licenses/pirostats"
	systemctl --user daemon-reload 2>/dev/null || true
	remove_runtime_cache
	echo "PiroStats user-local install removed."
	echo "Your config in ${XDG_CONFIG_HOME:-$HOME/.config}/pirostats was kept."
	exit 0
fi

ROOT="${DESTDIR:-}"
if [[ -n "$ROOT" ]]; then
	[[ "$ROOT" == /* && "$ROOT" != / ]] || {
		echo "[error] DESTDIR must be an absolute directory other than /" >&2
		exit 2
	}
	ROOT="$(canonical_path "$ROOT")"
	[[ "$ROOT" != / ]] || { echo "[error] DESTDIR resolves to /" >&2; exit 2; }
fi
SUDO=""
if [[ -z "$ROOT" && "$(id -u)" -ne 0 ]]; then SUDO=sudo; fi
if [[ -z "$ROOT" ]]; then
	prepare_runtime_cache || exit 2
	systemctl --user disable --now pirostats 2>/dev/null || true
fi

$SUDO rm -f -- "$ROOT/usr/lib/systemd/user/pirostats.service"
$SUDO rm -f -- "$ROOT/usr/bin/pirostats"
$SUDO rm -rf -- "$ROOT/usr/lib/pirostats"
$SUDO rm -f -- "$ROOT/usr/share/icons/hicolor/scalable/apps/pirostats.svg"
$SUDO rm -rf -- "$ROOT/usr/share/licenses/pirostats"

if [[ -n "$ROOT" ]]; then
	$SUDO rm -rf -- "$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
	echo "PiroStats removed from $ROOT"
	exit 0
fi

if command -v kpackagetool6 >/dev/null; then
	$SUDO kpackagetool6 --type Plasma/Applet --global --remove "$APPLET_ID" 2>/dev/null || true
fi
systemctl --user daemon-reload 2>/dev/null || true
remove_runtime_cache
echo "PiroStats uninstalled. (Remove the widget from your panel if it is still there.)"
echo "Your config in ~/.config/pirostats was kept; delete it by hand if wanted."
