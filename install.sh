#!/usr/bin/env bash
# Install system-wide by default, or entirely below user-owned paths with --user.
set -euo pipefail

usage() {
	cat <<'EOF'
Usage: ./install.sh [--user] [--dry-run|--dry]

  --user  Install without sudo under ~/.local and XDG_DATA_HOME.
  --dry-run, --dry  Print resolved paths and planned commands without running them.
EOF
}

print_command() {
	printf '  '
	printf '%q ' "$@"
	printf '\n'
}

print_dry_run() {
	printf 'Dry run only; no system changes will be made.\n'
	printf 'Mode: %s\nSource: %s\nBinary: %s\n' "$MODE" "$REPO_DIR" "$BINARY"
	if [[ "$MODE" == user ]]; then
		printf 'Data root: %s\nInstall tree: %s\nLauncher: %s\n' "$DATA_HOME" "$LIBDIR" "$LAUNCHER"
		printf 'Systemd unit: %s\nIcon: %s\nLicenses: %s\n' "$UNIT" "$ICON" "$LICENSEDIR"
	else
		printf 'Install tree: %s\nLauncher: %s\n' "$LIBDIR" "$ROOT/usr/bin/pirostats"
		printf 'Systemd unit: %s\nApplet: %s\n' \
			"$ROOT/usr/lib/systemd/user/pirostats.service" \
			"$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
	fi

	printf '\n1. Build or select binary\n'
	if [[ -n "${PIROSTATS_BINARY:-}" ]]; then
		printf '  Use prebuilt binary %q.\n' "$BINARY"
	else
		printf '  CARGO_TARGET_DIR=%q cargo build --manifest-path %q --release --locked --features nvml\n' \
			"$REPO_DIR/rust/target" "$REPO_DIR/rust/Cargo.toml"
	fi
	print_command test -x "$BINARY"

	if [[ "$MODE" == user ]]; then
		printf '\n2. Stage and validate user-local files\n'
		print_command mkdir -p "$DATA_HOME" "$BINDIR" "$UNITDIR" "$(dirname "$ICON")" "$LICENSEDIR"
		print_command mktemp -d "$DATA_HOME/.pirostats.tmp.XXXXXX"
		print_command mktemp -d "$TEMP_HOME/pirostats-applet.XXXXXX"
		print_command cp -r "$REPO_DIR/style" "$REPO_DIR/lang" "$REPO_DIR/config" '$STAGE/'
		print_command install -m755 "$BINARY" '$STAGE/pirostats'
		printf '  Write ownership marker %q.\n' "$LIBDIR/.pirostats-install"
		print_command '$STAGE/pirostats' list-items
		printf '  Create launcher with PIROSTATS_CODE_ROOT=%q.\n' "$LIBDIR"
		print_command install -m644 "$REPO_DIR/service/pirostats-user.service" "$UNIT"
		print_command install -m644 "$REPO_DIR/plasmoid/package/contents/icons/pirostats.svg" "$ICON"
		print_command install -m644 "$REPO_DIR/LICENSE" "$LICENSEDIR/LICENSE"
		print_command install -m644 "$REPO_DIR/NOTICE" "$LICENSEDIR/NOTICE"

		printf '\n3. Replace prior owned install atomically\n'
		printf '  Refuse an existing install tree without %q.\n' "$LIBDIR/.pirostats-install"
		printf '  Move prior owned tree to a temporary backup; restore it if replacement fails.\n'
		print_command mv '$STAGE' "$LIBDIR"
		printf '  Move launcher, unit, icon, license, and notice temporary files to paths above; remove backup.\n'

		printf '\n4. Install or upgrade Plasma applet\n'
		printf '  Copy applet to temporary staging and replace /usr/bin/pirostats actions with %q.\n' "$LAUNCHER"
		print_command kpackagetool6 --type Plasma/Applet --show "$APPLET_ID"
		printf '  If found:\n'
		print_command kpackagetool6 --type Plasma/Applet --upgrade '$APPLET_STAGE'
		printf '  If absent:\n'
		print_command kpackagetool6 --type Plasma/Applet --install '$APPLET_STAGE'
		print_command kbuildsycoca6

		printf '\n5. Activate user service\n'
		print_command systemctl --user daemon-reload
		print_command systemctl --user enable pirostats
		print_command systemctl --user restart pirostats
		print_command systemctl --user is-active --quiet pirostats
		printf '  On applet upgrade, restart plasmashell when kstart is available.\n'
	else
		printf '\n2. Replace system files\n'
		print_command ${SUDO:+$SUDO} rm -rf "$LIBDIR"
		print_command ${SUDO:+$SUDO} install -d "$LIBDIR"
		print_command ${SUDO:+$SUDO} cp -r "$REPO_DIR/style" "$REPO_DIR/lang" "$REPO_DIR/config" "$LIBDIR/"
		print_command ${SUDO:+$SUDO} install -m755 "$BINARY" "$LIBDIR/pirostats"
		print_command ${SUDO:+$SUDO} install -Dm755 "$REPO_DIR/packaging/pirostats-launcher" "$ROOT/usr/bin/pirostats"
		print_command ${SUDO:+$SUDO} install -Dm644 "$REPO_DIR/service/pirostats.service" "$ROOT/usr/lib/systemd/user/pirostats.service"
		print_command ${SUDO:+$SUDO} install -Dm644 "$REPO_DIR/plasmoid/package/contents/icons/pirostats.svg" "$ROOT/usr/share/icons/hicolor/scalable/apps/pirostats.svg"
		print_command ${SUDO:+$SUDO} install -Dm644 "$REPO_DIR/LICENSE" "$ROOT/usr/share/licenses/pirostats/LICENSE"
		print_command ${SUDO:+$SUDO} install -Dm644 "$REPO_DIR/NOTICE" "$ROOT/usr/share/licenses/pirostats/NOTICE"
		if [[ -n "$ROOT" ]]; then
			printf '\n3. Stage applet in DESTDIR; service is not activated\n'
			print_command rm -rf "$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
			print_command install -d "$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
			print_command cp -r "$REPO_DIR/plasmoid/package/." "$ROOT/usr/share/plasma/plasmoids/$APPLET_ID/"
		else
			printf '\n3. Install or upgrade global applet, then activate user service\n'
			print_command kpackagetool6 --type Plasma/Applet --global --show "$APPLET_ID"
			printf '  If found:\n'
			print_command ${SUDO:+$SUDO} kpackagetool6 --type Plasma/Applet --global --upgrade "$REPO_DIR/plasmoid/package"
			printf '  If absent:\n'
			print_command ${SUDO:+$SUDO} kpackagetool6 --type Plasma/Applet --global --install "$REPO_DIR/plasmoid/package"
			print_command kbuildsycoca6
			print_command systemctl --user daemon-reload
			print_command systemctl --user enable pirostats
			print_command systemctl --user restart pirostats
			printf '  On applet upgrade, restart plasmashell when kstart is available.\n'
		fi
	fi
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
DRY_RUN=false
user_set=false
dry_run_set=false
for argument in "$@"; do
	case "$argument" in
		--user)
			[[ "$user_set" == false ]] || { echo "[error] duplicate argument: $argument" >&2; exit 2; }
			MODE=user
			user_set=true
			;;
		--dry-run|--dry)
			[[ "$dry_run_set" == false ]] || { echo "[error] duplicate dry-run argument: $argument" >&2; exit 2; }
			DRY_RUN=true
			dry_run_set=true
			;;
		-h|--help) usage; exit 0 ;;
		*) echo "[error] unknown argument: $argument" >&2; usage >&2; exit 2 ;;
	esac
done
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
	if [[ "$DRY_RUN" == false ]]; then
		for command in kpackagetool6 systemctl; do
			command -v "$command" >/dev/null || {
				echo "[error] $command not found" >&2
				exit 1
			}
		done
	fi
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
	if [[ "$DRY_RUN" == false && -z "$ROOT" ]] && ! command -v kpackagetool6 >/dev/null; then
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
	BINARY="$REPO_DIR/rust/target/release/pirostats"
	if [[ "$DRY_RUN" == false ]]; then
		command -v cargo >/dev/null || {
			echo "[error] cargo not found — Rust 1.85+ is required." >&2
			exit 1
		}
		CARGO_TARGET_DIR="$REPO_DIR/rust/target" \
			cargo build --manifest-path "$REPO_DIR/rust/Cargo.toml" --release --locked --features nvml
	fi
fi
if [[ "$DRY_RUN" == true ]]; then
	print_dry_run
	exit 0
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
