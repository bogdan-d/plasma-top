#!/usr/bin/env bash
# Disposable native install/upgrade/uninstall layout test.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
ROOT="$TMP/root"
HOME="$TMP/home"
trap 'rm -rf "$TMP"' EXIT
export CARGO_HOME="${CARGO_HOME:-$TMP/cargo-home}"
export CARGO_TARGET_DIR="$REPO_DIR/target"

mkdir -p "$HOME/.config/plasma-top" "$HOME/.cache/plasma-top"
printf 'user config\n' >"$HOME/.config/plasma-top/config.toml"
printf 'cache\n' >"$HOME/.cache/plasma-top/geom"

cargo build --manifest-path "$REPO_DIR/Cargo.toml" --release --locked --features nvml
BINARY="$CARGO_TARGET_DIR/release/plasma-top"

stage_native() {
    DESTDIR="$ROOT" HOME="$HOME" PLASMA_TOP_BINARY="$BINARY" "$REPO_DIR/install.sh" --system
}

assert_native_layout() {
    test -x "$ROOT/usr/bin/plasma-top"
    test ! -L "$ROOT/usr/bin/plasma-top"
    test -x "$ROOT/usr/lib/plasma-top/plasma-top"
    test -f "$ROOT/usr/lib/plasma-top/config/config.toml"
    test -f "$ROOT/usr/lib/plasma-top/style/style-dark.css"
    test -f "$ROOT/usr/lib/plasma-top/lang/en.toml"
    test -f "$ROOT/usr/lib/systemd/user/plasma-top.service"
    test -f "$ROOT/usr/share/plasma/plasmoids/com.github.bogdan-d.plasma-top/metadata.json"
    test -f "$ROOT/usr/share/licenses/plasma-top/LICENSE"
    test -f "$ROOT/usr/share/licenses/plasma-top/NOTICE"
    test ! -e "$ROOT/usr/lib/plasma-top/src"
    PLASMA_TOP_CODE_ROOT="$ROOT/usr/lib/plasma-top" \
        "$ROOT/usr/lib/plasma-top/plasma-top" list-items >/dev/null
}

stage_legacy_layout() {
    rm -rf "$ROOT/usr/lib/plasma-top"
    mkdir -p "$ROOT/usr/lib/plasma-top" "$ROOT/usr/bin"
    printf 'legacy package marker\n' >"$ROOT/usr/lib/plasma-top/stale-runtime"
    ln -sfn /usr/lib/plasma-top/plasma-top "$ROOT/usr/bin/plasma-top"
}

stage_legacy_layout
stage_native
assert_native_layout
printf 'stale upgrade file\n' >"$ROOT/usr/lib/plasma-top/stale"
stage_native
assert_native_layout
test ! -e "$ROOT/usr/lib/plasma-top/stale"

DESTDIR="$ROOT" HOME="$HOME" "$REPO_DIR/uninstall.sh" --system
test ! -e "$ROOT/usr/bin/plasma-top"
test ! -e "$ROOT/usr/lib/plasma-top"
test ! -e "$ROOT/usr/lib/systemd/user/plasma-top.service"
test ! -e "$ROOT/usr/share/plasma/plasmoids/com.github.bogdan-d.plasma-top"
test "$(cat "$HOME/.config/plasma-top/config.toml")" = "user config"
test "$(cat "$HOME/.cache/plasma-top/geom")" = "cache"

# Exercise the PKGBUILD package function against the same compiled candidate.
srcdir="$TMP/aur-src"
pkgdir="$TMP/aur-pkg"
mkdir -p "$srcdir" "$pkgdir"
ln -s "$REPO_DIR" "$srcdir/plasma-top"
# shellcheck disable=SC1091
source "$REPO_DIR/packaging/aur/PKGBUILD"
package
test -x "$pkgdir/usr/bin/plasma-top"
test -x "$pkgdir/usr/lib/plasma-top/plasma-top"
test -f "$pkgdir/usr/share/licenses/plasma-top-git/LICENSE"
test -f "$pkgdir/usr/share/licenses/plasma-top-git/NOTICE"
test ! -e "$pkgdir/usr/lib/plasma-top/src"

echo "P6 native package layout, legacy upgrade, repeat upgrade, and uninstall checks passed"
