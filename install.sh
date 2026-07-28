#!/usr/bin/env bash
# Install system-wide by default, or entirely below user-owned paths with --user.
set -euo pipefail

usage() {
	cat <<'EOF'
Usage: ./install.sh [--user]

  --user  Install without sudo under ~/.local and XDG_DATA_HOME.
EOF
}

canonical_path() {
	realpath -m -- "$1"
}

remove_temp_tree() {
	local path="$1" parent="$2" prefix="$3" name
	[[ -n "$path" ]] || return 0
	name="$(basename -- "$path")"
	if [[ "$(canonical_path "$(dirname -- "$path")")" != "$(canonical_path "$parent")" \
		|| "$name" != "$prefix"* ]]; then
		echo "[error] refusing unsafe temporary cleanup: $path" >&2
		return 1
	fi
	rm -rf -- "$path"
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

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPLET_ID="com.github.lucazade.pirostats"

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
	BINDIR="$HOME/.local/bin"
	LAUNCHER="$BINDIR/pirostats"
	UNITDIR="$DATA_HOME/systemd/user"
	UNIT="$UNITDIR/pirostats.service"
	ICON="$DATA_HOME/icons/hicolor/scalable/apps/pirostats.svg"
	LICENSEDIR="$DATA_HOME/licenses/pirostats"
	TEMP_HOME="$(canonical_path "${TMPDIR:-/tmp}")"
	[[ "$TEMP_HOME" != / ]] || { echo "[error] temporary directory resolves to /" >&2; exit 2; }
	ROOT=""
	SUDO=""
	for command in kpackagetool6 systemctl; do
		command -v "$command" >/dev/null || {
			echo "[error] $command not found" >&2
			exit 1
		}
	done
else
	ROOT="${DESTDIR:-}"
	if [[ -n "$ROOT" ]]; then
		[[ "$ROOT" == /* && "$ROOT" != / ]] || {
			echo "[error] DESTDIR must be an absolute directory other than /" >&2
			exit 2
		}
		ROOT="$(canonical_path "$ROOT")"
		[[ "$ROOT" != / ]] || { echo "[error] DESTDIR resolves to /" >&2; exit 2; }
	fi
	LIBDIR="$ROOT/usr/lib/pirostats"
	SUDO=""
	if [[ -z "$ROOT" && "$(id -u)" -ne 0 ]]; then SUDO=sudo; fi
	if [[ -z "$ROOT" ]] && ! command -v kpackagetool6 >/dev/null; then
		echo "[error] kpackagetool6 not found — Plasma 6 is required." >&2
		exit 1
	fi
fi

if [[ -n "${PIROSTATS_BINARY:-}" ]]; then
	[[ "$PIROSTATS_BINARY" == /* ]] || {
		echo "[error] PIROSTATS_BINARY must be an absolute path" >&2
		exit 2
	}
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
[[ -x "$BINARY" ]] || {
	echo "[error] Rust binary not found or not executable: $BINARY" >&2
	exit 1
}

	if [[ "$MODE" == user ]]; then
	echo "Installing PiroStats for current user under $DATA_HOME"
	mkdir -p "$(dirname "$LIBDIR")"
	STAGE=""
	APPLET_STAGE=""
	BACKUP="$DATA_HOME/.pirostats.backup.$$"
	cleanup() {
		remove_temp_tree "$STAGE" "$DATA_HOME" .pirostats.tmp. || true
		remove_temp_tree "$APPLET_STAGE" "$TEMP_HOME" pirostats-applet. || true
		if [[ -e "$BACKUP" ]]; then
			if [[ ! -e "$LIBDIR" ]]; then
				mv "$BACKUP" "$LIBDIR" || true
			else
				remove_temp_tree "$BACKUP" "$DATA_HOME" .pirostats.backup. || true
			fi
		fi
		rm -f -- "$LAUNCHER.tmp" "$UNIT.tmp" "$ICON.tmp" \
			"$LICENSEDIR/LICENSE.tmp" "$LICENSEDIR/NOTICE.tmp"
	}
	trap cleanup EXIT
	STAGE="$(mktemp -d "$DATA_HOME/.pirostats.tmp.XXXXXX")"
	APPLET_STAGE="$(mktemp -d "$TEMP_HOME/pirostats-applet.XXXXXX")"
	[[ ! -e "$BACKUP" && ! -L "$BACKUP" ]] || {
		echo "[error] refusing pre-existing backup path: $BACKUP" >&2
		exit 1
	}
	owned_install=false
	if [[ -e "$LIBDIR" || -L "$LIBDIR" ]]; then
		if [[ -L "$LIBDIR" || ! -f "$LIBDIR/.pirostats-install" ]]; then
			echo "[error] refusing to replace unowned path: $LIBDIR" >&2
			exit 1
		fi
		owned_install=true
	else
		for path in "$LAUNCHER" "$UNIT" "$ICON" \
			"$LICENSEDIR/LICENSE" "$LICENSEDIR/NOTICE"; do
			if [[ -e "$path" || -L "$path" ]]; then
				echo "[error] refusing to replace unowned path: $path" >&2
				exit 1
			fi
		done
	fi
	cp -r "$REPO_DIR/style" "$REPO_DIR/lang" "$REPO_DIR/config" "$STAGE/"
	install -m755 "$BINARY" "$STAGE/pirostats"
	printf 'user-local-v1\n' > "$STAGE/.pirostats-install"
	"$STAGE/pirostats" list-items >/dev/null

	mkdir -p "$BINDIR" "$UNITDIR" "$(dirname "$ICON")" "$LICENSEDIR"
	printf '#!/usr/bin/env bash\nexport PIROSTATS_CODE_ROOT=%q\nexec %q "$@"\n' \
		"$LIBDIR" "$LIBDIR/pirostats" > "$LAUNCHER.tmp"
	chmod 755 "$LAUNCHER.tmp"
	install -m644 "$REPO_DIR/service/pirostats-user.service" "$UNIT.tmp"
	install -m644 "$REPO_DIR/plasmoid/package/contents/icons/pirostats.svg" "$ICON.tmp"
	install -m644 "$REPO_DIR/LICENSE" "$LICENSEDIR/LICENSE.tmp"
	install -m644 "$REPO_DIR/NOTICE" "$LICENSEDIR/NOTICE.tmp"

	if [[ "$owned_install" == true ]]; then
		mv "$LIBDIR" "$BACKUP"
	fi
	if ! mv "$STAGE" "$LIBDIR"; then
		[[ -e "$BACKUP" ]] && mv "$BACKUP" "$LIBDIR"
		echo "[error] failed to replace $LIBDIR" >&2
		exit 1
	fi
	mv "$LAUNCHER.tmp" "$LAUNCHER"
	mv "$UNIT.tmp" "$UNIT"
	mv "$ICON.tmp" "$ICON"
	mv "$LICENSEDIR/LICENSE.tmp" "$LICENSEDIR/LICENSE"
	mv "$LICENSEDIR/NOTICE.tmp" "$LICENSEDIR/NOTICE"
	remove_temp_tree "$BACKUP" "$DATA_HOME" .pirostats.backup.

	cp -a "$REPO_DIR/plasmoid/package/." "$APPLET_STAGE/"
	xml="$APPLET_STAGE/contents/config/main.xml"
	xml_path=${LAUNCHER//&/\&amp;}; xml_path=${xml_path//</\&lt;}; xml_path=${xml_path//>/\&gt;}
	xml_text=$(<"$xml")
	xml_replacement='\&quot;'"$xml_path"'\&quot;'
	xml_text=${xml_text//\/usr\/bin\/pirostats/$xml_replacement}
	printf '%s' "$xml_text" > "$xml"

	if kpackagetool6 --type Plasma/Applet --show "$APPLET_ID" >/dev/null 2>&1; then
		if ! kpackagetool6 --type Plasma/Applet --upgrade "$APPLET_STAGE"; then
			echo "[error] files installed, but applet upgrade failed; retry ./install.sh --user" >&2
			exit 1
		fi
		applet_upgraded=true
	else
		if ! kpackagetool6 --type Plasma/Applet --install "$APPLET_STAGE"; then
			echo "[error] files installed, but applet install failed; retry ./install.sh --user" >&2
			exit 1
		fi
		applet_upgraded=false
	fi
	command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
	systemctl --user daemon-reload
	systemctl --user enable pirostats
	if ! systemctl --user restart pirostats || ! systemctl --user is-active --quiet pirostats; then
		echo "[error] files installed, but service activation failed" >&2
		echo "Recover: systemctl --user restart pirostats" >&2
		echo "Inspect: journalctl --user -u pirostats -n 100" >&2
		exit 1
	fi
	if [[ "$applet_upgraded" == true ]] && command -v kstart >/dev/null; then
		killall plasmashell 2>/dev/null || true
		kstart plasmashell >/dev/null 2>&1 &
	fi
	echo "PiroStats installed for current user. Service is active."
	if [[ "$applet_upgraded" == false ]]; then
		echo "Add the 'PiroStats' widget to a panel."
	fi
	exit 0
fi

# Native package-compatible installation.
$SUDO rm -rf -- "$LIBDIR"
$SUDO install -d "$LIBDIR"
$SUDO cp -r "$REPO_DIR/style" "$REPO_DIR/lang" "$REPO_DIR/config" "$LIBDIR/"
$SUDO install -m755 "$BINARY" "$LIBDIR/pirostats"
$SUDO install -Dm755 "$REPO_DIR/packaging/pirostats-launcher" "$ROOT/usr/bin/pirostats"
$SUDO install -Dm644 "$REPO_DIR/service/pirostats.service" "$ROOT/usr/lib/systemd/user/pirostats.service"
$SUDO install -Dm644 "$REPO_DIR/plasmoid/package/contents/icons/pirostats.svg" "$ROOT/usr/share/icons/hicolor/scalable/apps/pirostats.svg"
$SUDO install -Dm644 "$REPO_DIR/LICENSE" "$ROOT/usr/share/licenses/pirostats/LICENSE"
$SUDO install -Dm644 "$REPO_DIR/NOTICE" "$ROOT/usr/share/licenses/pirostats/NOTICE"

if [[ -n "$ROOT" ]]; then
	APPLET_DIR="$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
	$SUDO rm -rf -- "$APPLET_DIR"
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
if [[ "$applet_upgraded" == true ]] && command -v kstart >/dev/null; then
	killall plasmashell 2>/dev/null || true
	kstart plasmashell >/dev/null 2>&1 &
fi

echo "PiroStats installed system-wide. Service status:"
systemctl --user status pirostats --no-pager || true
if [[ "$applet_upgraded" == false ]]; then
	echo "First install: add the 'PiroStats' widget to a panel."
fi
