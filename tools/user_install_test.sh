#!/usr/bin/env bash
# Disposable user-local install/upgrade/uninstall test. Never touches real paths.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
export HOME="$TMP/home with spaces"
export XDG_DATA_HOME="$TMP/data with spaces"
export XDG_CONFIG_HOME="$TMP/config"
export XDG_CACHE_HOME="$TMP/cache"
export XDG_RUNTIME_DIR="$TMP/runtime"
export PIROSTATS_BINARY="$REPO_DIR/rust/target/release/pirostats"
export FAKE_LOG="$TMP/commands.log"
export FAKE_APPLET="$TMP/installed-applet"
export FAKE_APPLET_STATE="$TMP/applet-installed"
FAKE_BIN="$TMP/fake-bin"
mkdir -p "$HOME" "$XDG_CONFIG_HOME/pirostats" "$XDG_CACHE_HOME/pirostats" \
	"$XDG_RUNTIME_DIR" "$FAKE_BIN"
printf 'user config\n' > "$XDG_CONFIG_HOME/pirostats/config.toml"
printf 'cache\n' > "$XDG_CACHE_HOME/pirostats/geom"

cat > "$FAKE_BIN/systemctl" <<'EOF'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >> "$FAKE_LOG"
if [[ -n "${FAIL_SERVICE:-}" && " $* " == *" is-active "* ]]; then exit 1; fi
exit 0
EOF
cat > "$FAKE_BIN/kpackagetool6" <<'EOF'
#!/usr/bin/env bash
printf 'kpackagetool6 %s\n' "$*" >> "$FAKE_LOG"
case " $* " in
  *" --show "*) [[ -e "$FAKE_APPLET_STATE" ]] ;;
  *" --install "*|*" --upgrade "*)
    source_path="${!#}"
    rm -rf "$FAKE_APPLET"
    cp -a "$source_path" "$FAKE_APPLET"
    : > "$FAKE_APPLET_STATE"
    ;;
  *" --remove "*) rm -f "$FAKE_APPLET_STATE" ;;
esac
EOF
for command in kbuildsycoca6 kstart killall; do
	cat > "$FAKE_BIN/$command" <<'EOF'
#!/usr/bin/env bash
printf '%s %s\n' "$(basename "$0")" "$*" >> "$FAKE_LOG"
exit 0
EOF
done
cat > "$FAKE_BIN/sudo" <<'EOF'
#!/usr/bin/env bash
echo "sudo must not run in user mode" >&2
exit 99
EOF
chmod +x "$FAKE_BIN"/*
export PATH="$FAKE_BIN:$PATH"

[[ -x "$PIROSTATS_BINARY" ]] || {
	cargo build --manifest-path "$REPO_DIR/rust/Cargo.toml" --release --locked --features nvml
}

# Argument failures happen before writes.
if "$REPO_DIR/install.sh" --bogus >/dev/null 2>&1; then exit 1; fi
if DESTDIR="$TMP/stage" "$REPO_DIR/install.sh" --user >/dev/null 2>&1; then exit 1; fi
if DESTDIR=/ "$REPO_DIR/install.sh" >/dev/null 2>&1; then exit 1; fi
if DESTDIR=/ "$REPO_DIR/uninstall.sh" >/dev/null 2>&1; then exit 1; fi
if XDG_DATA_HOME=relative "$REPO_DIR/install.sh" --user >/dev/null 2>&1; then exit 1; fi
if XDG_DATA_HOME=/usr/local/share "$REPO_DIR/install.sh" --user >/dev/null 2>&1; then exit 1; fi
if XDG_DATA_HOME=/tmp/../usr/local/share "$REPO_DIR/install.sh" --user >/dev/null 2>&1; then exit 1; fi
[[ ! -e "$XDG_DATA_HOME/pirostats" ]]

# Uninstall and install refuse same-named paths without ownership marker.
mkdir -p "$XDG_DATA_HOME/pirostats" "$HOME/.local/bin"
printf 'keep data\n' > "$XDG_DATA_HOME/pirostats/unrelated"
printf 'keep launcher\n' > "$HOME/.local/bin/pirostats"
"$REPO_DIR/uninstall.sh" --user >/dev/null
[[ "$(cat "$XDG_DATA_HOME/pirostats/unrelated")" == "keep data" ]]
[[ "$(cat "$HOME/.local/bin/pirostats")" == "keep launcher" ]]
if "$REPO_DIR/install.sh" --user >/dev/null 2>&1; then exit 1; fi
rm -rf -- "$XDG_DATA_HOME/pirostats"
rm -f -- "$HOME/.local/bin/pirostats"

mkdir -p "$TMP/symlink-target"
ln -s "$TMP/symlink-target" "$XDG_DATA_HOME/pirostats"
if "$REPO_DIR/install.sh" --user >/dev/null 2>&1; then exit 1; fi
rm -f -- "$XDG_DATA_HOME/pirostats"

"$REPO_DIR/install.sh" --user
USER_ROOT="$XDG_DATA_HOME/pirostats"
LAUNCHER="$HOME/.local/bin/pirostats"
UNIT="$XDG_DATA_HOME/systemd/user/pirostats.service"
[[ -x "$USER_ROOT/pirostats" && -x "$LAUNCHER" && -f "$UNIT" ]]
[[ -f "$XDG_DATA_HOME/icons/hicolor/scalable/apps/pirostats.svg" ]]
[[ -f "$XDG_DATA_HOME/licenses/pirostats/LICENSE" ]]
grep -Fq 'ExecStart=%h/.local/bin/pirostats daemon' "$UNIT"
grep -Fq "&quot;$LAUNCHER&quot; click" "$FAKE_APPLET/contents/config/main.xml"
grep -Fq "&quot;$LAUNCHER&quot; page prev" "$FAKE_APPLET/contents/config/main.xml"
grep -Fq "&quot;$LAUNCHER&quot; page next" "$FAKE_APPLET/contents/config/main.xml"
! grep -q -- '--global' "$FAKE_LOG"
! grep -q '^sudo ' "$FAKE_LOG"
"$LAUNCHER" list-items >/dev/null
[[ ! -L "$USER_ROOT/pirostats" ]]
! grep -Fq "$REPO_DIR" "$LAUNCHER"

printf 'keep\n' > "$USER_ROOT/keep"
if PIROSTATS_BINARY="$TMP/missing" "$REPO_DIR/install.sh" --user >/dev/null 2>&1; then
	exit 1
fi
[[ "$(cat "$USER_ROOT/keep")" == keep ]]
rm "$USER_ROOT/keep"

# Units differ only at ExecStart.
diff -u \
	<(sed 's|^ExecStart=.*|ExecStart=<launcher> daemon|' "$REPO_DIR/service/pirostats.service") \
	<(sed 's|^ExecStart=.*|ExecStart=<launcher> daemon|' "$REPO_DIR/service/pirostats-user.service")

printf 'stale\n' > "$USER_ROOT/stale"
"$REPO_DIR/install.sh" --user
[[ ! -e "$USER_ROOT/stale" ]]
grep -q -- '--upgrade' "$FAKE_LOG"

# Unsafe state roots fail before owned files are touched.
if XDG_CACHE_HOME=/ "$REPO_DIR/uninstall.sh" --user >/dev/null 2>&1; then exit 1; fi
if XDG_RUNTIME_DIR=/ "$REPO_DIR/uninstall.sh" --user >/dev/null 2>&1; then exit 1; fi
[[ -x "$USER_ROOT/pirostats" && -x "$LAUNCHER" ]]

if FAIL_SERVICE=1 "$REPO_DIR/install.sh" --user >"$TMP/activation-failure.log" 2>&1; then
	exit 1
fi
grep -Fq 'journalctl --user -u pirostats -n 100' "$TMP/activation-failure.log"
"$LAUNCHER" list-items >/dev/null

"$REPO_DIR/uninstall.sh" --user
"$REPO_DIR/uninstall.sh" --user
[[ ! -e "$USER_ROOT" && ! -e "$LAUNCHER" && ! -e "$UNIT" ]]
[[ "$(cat "$XDG_CONFIG_HOME/pirostats/config.toml")" == "user config" ]]
[[ ! -e "$XDG_CACHE_HOME/pirostats" ]]
! grep -q -- '--global' "$FAKE_LOG"
! grep -q '^sudo ' "$FAKE_LOG"

echo "User-local install, upgrade, and uninstall checks passed"
