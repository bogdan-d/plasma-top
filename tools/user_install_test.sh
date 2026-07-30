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
export PLASMA_TOP_BINARY="$REPO_DIR/target/release/plasma-top"
export FAKE_LOG="$TMP/commands.log"
export FAKE_APPLET="$TMP/installed-applet"
export FAKE_APPLET_STATE="$TMP/applet-installed"
FAKE_BIN="$TMP/fake-bin"
mkdir -p "$HOME" "$XDG_CONFIG_HOME/plasma-top" "$XDG_CACHE_HOME/plasma-top" \
    "$XDG_RUNTIME_DIR" "$FAKE_BIN"
printf 'user config\n' >"$XDG_CONFIG_HOME/plasma-top/config.toml"
printf 'cache\n' >"$XDG_CACHE_HOME/plasma-top/geom"

cat >"$FAKE_BIN/systemctl" <<'EOF'
#!/usr/bin/env bash
printf 'systemctl %s\n' "$*" >> "$FAKE_LOG"
if [[ -n "${FAIL_SERVICE:-}" && " $* " == *" is-active "* ]]; then exit 1; fi
exit 0
EOF
cat >"$FAKE_BIN/kpackagetool6" <<'EOF'
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
    cat >"$FAKE_BIN/$command" <<'EOF'
#!/usr/bin/env bash
printf '%s %s\n' "$(basename "$0")" "$*" >> "$FAKE_LOG"
exit 0
EOF
done
cat >"$FAKE_BIN/sudo" <<'EOF'
#!/usr/bin/env bash
echo "sudo must not run in user mode" >&2
exit 99
EOF
chmod +x "$FAKE_BIN"/*
export PATH="$FAKE_BIN:$PATH"

[[ -x "$PLASMA_TOP_BINARY" ]] || {
    cargo build --manifest-path "$REPO_DIR/Cargo.toml" --release --locked --features nvml
}

# Argument failures happen before writes.
if "$REPO_DIR/install.sh" --bogus >/dev/null 2>&1; then exit 1; fi
if "$REPO_DIR/install.sh" --user >/dev/null 2>&1; then exit 1; fi
if "$REPO_DIR/uninstall.sh" --user >/dev/null 2>&1; then exit 1; fi
if DESTDIR="$TMP/stage" "$REPO_DIR/install.sh" >/dev/null 2>&1; then exit 1; fi
if DESTDIR=/ "$REPO_DIR/install.sh" --system >/dev/null 2>&1; then exit 1; fi
if DESTDIR=/ "$REPO_DIR/uninstall.sh" --system >/dev/null 2>&1; then exit 1; fi
if XDG_DATA_HOME=relative "$REPO_DIR/install.sh" >/dev/null 2>&1; then exit 1; fi
if XDG_DATA_HOME=/usr/local/share "$REPO_DIR/install.sh" >/dev/null 2>&1; then exit 1; fi
if XDG_DATA_HOME=/tmp/../usr/local/share "$REPO_DIR/install.sh" >/dev/null 2>&1; then exit 1; fi
[[ ! -e "$XDG_DATA_HOME/plasma-top" ]]

# Both dry-run spellings resolve paths without invoking install dependencies or writing files.
: >"$FAKE_LOG"
"$REPO_DIR/install.sh" --dry-run >"$TMP/dry-run.log"
"$REPO_DIR/install.sh" --dry >"$TMP/dry.log"
[[ ! -s "$FAKE_LOG" && ! -e "$XDG_DATA_HOME/plasma-top" ]]
grep -Fq "Install tree: $XDG_DATA_HOME/plasma-top" "$TMP/dry-run.log"
grep -Fq "Launcher: $HOME/.local/bin/plasma-top" "$TMP/dry-run.log"
grep -Fq 'systemctl --user restart plasma-top' "$TMP/dry-run.log"
grep -Fq 'Dry run only; no system changes will be made.' "$TMP/dry.log"

# Uninstall and install refuse same-named paths without ownership marker.
mkdir -p "$XDG_DATA_HOME/plasma-top" "$HOME/.local/bin"
printf 'keep data\n' >"$XDG_DATA_HOME/plasma-top/unrelated"
printf 'keep launcher\n' >"$HOME/.local/bin/plasma-top"
"$REPO_DIR/uninstall.sh" >/dev/null
[[ "$(cat "$XDG_DATA_HOME/plasma-top/unrelated")" == "keep data" ]]
[[ "$(cat "$HOME/.local/bin/plasma-top")" == "keep launcher" ]]
if "$REPO_DIR/install.sh" >/dev/null 2>&1; then exit 1; fi
rm -rf -- "$XDG_DATA_HOME/plasma-top"
rm -f -- "$HOME/.local/bin/plasma-top"

mkdir -p "$TMP/symlink-target"
ln -s "$TMP/symlink-target" "$XDG_DATA_HOME/plasma-top"
if "$REPO_DIR/install.sh" >/dev/null 2>&1; then exit 1; fi
rm -f -- "$XDG_DATA_HOME/plasma-top"

"$REPO_DIR/install.sh"
USER_ROOT="$XDG_DATA_HOME/plasma-top"
LAUNCHER="$HOME/.local/bin/plasma-top"
UNIT="$XDG_DATA_HOME/systemd/user/plasma-top.service"
[[ -x "$USER_ROOT/plasma-top" && -x "$LAUNCHER" && -f "$UNIT" ]]
[[ -f "$XDG_DATA_HOME/icons/hicolor/scalable/apps/plasma-top.svg" ]]
[[ -f "$XDG_DATA_HOME/licenses/plasma-top/LICENSE" ]]
grep -Fq 'ExecStart=%h/.local/bin/plasma-top daemon' "$UNIT"
grep -Fq "&quot;$LAUNCHER&quot; click" "$FAKE_APPLET/contents/config/main.xml"
grep -Fq "&quot;$LAUNCHER&quot; page prev" "$FAKE_APPLET/contents/config/main.xml"
grep -Fq "&quot;$LAUNCHER&quot; page next" "$FAKE_APPLET/contents/config/main.xml"
if grep -q -- '--global' "$FAKE_LOG"; then exit 1; fi
if grep -q '^sudo ' "$FAKE_LOG"; then exit 1; fi
"$LAUNCHER" list-items >/dev/null
[[ ! -L "$USER_ROOT/plasma-top" ]]
if grep -Fq "$REPO_DIR" "$LAUNCHER"; then exit 1; fi

# Uninstall dry runs report every removal target without stopping or deleting anything.
: >"$FAKE_LOG"
"$REPO_DIR/uninstall.sh" --dry-run >"$TMP/uninstall-dry-run.log"
"$REPO_DIR/uninstall.sh" --dry >"$TMP/uninstall-dry.log"
[[ ! -s "$FAKE_LOG" && -x "$USER_ROOT/plasma-top" && -x "$LAUNCHER" ]]
grep -Fq "Install tree: $USER_ROOT" "$TMP/uninstall-dry-run.log"
grep -Fq "Runtime data: $XDG_RUNTIME_DIR/plasma-top" "$TMP/uninstall-dry-run.log"
grep -Fq 'systemctl --user disable --now plasma-top' "$TMP/uninstall-dry-run.log"
grep -Fq 'Dry run only; no system changes will be made.' "$TMP/uninstall-dry.log"

printf 'keep\n' >"$USER_ROOT/keep"
if PLASMA_TOP_BINARY="$TMP/missing" "$REPO_DIR/install.sh" >/dev/null 2>&1; then
    exit 1
fi
[[ "$(cat "$USER_ROOT/keep")" == keep ]]
rm "$USER_ROOT/keep"

# Units differ only at ExecStart.
diff -u \
    <(sed 's|^ExecStart=.*|ExecStart=<launcher> daemon|' "$REPO_DIR/service/plasma-top.service") \
    <(sed 's|^ExecStart=.*|ExecStart=<launcher> daemon|' "$REPO_DIR/service/plasma-top-user.service")

printf 'stale\n' >"$USER_ROOT/stale"
"$REPO_DIR/install.sh"
[[ ! -e "$USER_ROOT/stale" ]]
grep -q -- '--upgrade' "$FAKE_LOG"

# Unsafe state roots fail before owned files are touched.
if XDG_CACHE_HOME=/ "$REPO_DIR/uninstall.sh" >/dev/null 2>&1; then exit 1; fi
if XDG_RUNTIME_DIR=/ "$REPO_DIR/uninstall.sh" >/dev/null 2>&1; then exit 1; fi
[[ -x "$USER_ROOT/plasma-top" && -x "$LAUNCHER" ]]

if FAIL_SERVICE=1 "$REPO_DIR/install.sh" >"$TMP/activation-failure.log" 2>&1; then
    exit 1
fi
grep -Fq 'journalctl --user -u plasma-top -n 100' "$TMP/activation-failure.log"
"$LAUNCHER" list-items >/dev/null

"$REPO_DIR/uninstall.sh"
"$REPO_DIR/uninstall.sh"
[[ ! -e "$USER_ROOT" && ! -e "$LAUNCHER" && ! -e "$UNIT" ]]
[[ "$(cat "$XDG_CONFIG_HOME/plasma-top/config.toml")" == "user config" ]]
[[ ! -e "$XDG_CACHE_HOME/plasma-top" ]]
if grep -q -- '--global' "$FAKE_LOG"; then exit 1; fi
if grep -q '^sudo ' "$FAKE_LOG"; then exit 1; fi

echo "User-local install, upgrade, and uninstall checks passed"
