#!/usr/bin/env bash
# Disposable native install/upgrade/rollback/uninstall layout test.
set -euo pipefail

REPO_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TMP="$(mktemp -d)"
ROOT="$TMP/root"
HOME="$TMP/home"
trap 'rm -rf "$TMP"' EXIT
export CARGO_HOME="${CARGO_HOME:-$TMP/cargo-home}"
export CARGO_TARGET_DIR="$REPO_DIR/rust/target"

mkdir -p "$HOME/.config/pirostats" "$HOME/.cache/pirostats"
printf 'user config\n' > "$HOME/.config/pirostats/config.toml"
printf 'cache\n' > "$HOME/.cache/pirostats/geom"

cargo build --manifest-path "$REPO_DIR/rust/Cargo.toml" --release --locked --features nvml
BINARY="$CARGO_TARGET_DIR/release/pirostats"

stage_native() {
	DESTDIR="$ROOT" HOME="$HOME" PIROSTATS_BINARY="$BINARY" "$REPO_DIR/install.sh"
}

assert_native_layout() {
	test -x "$ROOT/usr/bin/pirostats"
	test ! -L "$ROOT/usr/bin/pirostats"
	test -x "$ROOT/usr/lib/pirostats/pirostats"
	test -f "$ROOT/usr/lib/pirostats/config/config.toml"
	test -f "$ROOT/usr/lib/pirostats/style/style-dark.css"
	test -f "$ROOT/usr/lib/pirostats/lang/en.toml"
	test -f "$ROOT/usr/lib/systemd/user/pirostats.service"
	test -f "$ROOT/usr/share/plasma/plasmoids/com.github.lucazade.pirostats/metadata.json"
	test -f "$ROOT/usr/share/licenses/pirostats/LICENSE"
	test -f "$ROOT/usr/share/licenses/pirostats/NOTICE"
	test ! -e "$ROOT/usr/lib/pirostats/src"
	PIROSTATS_CODE_ROOT="$ROOT/usr/lib/pirostats" \
		"$ROOT/usr/lib/pirostats/pirostats" list-items >/dev/null
}

stage_python_rollback() {
	rm -rf "$ROOT/usr/lib/pirostats"
	mkdir -p "$ROOT/usr/lib/pirostats" "$ROOT/usr/bin"
	cp -r "$REPO_DIR/src" "$REPO_DIR/style" "$REPO_DIR/lang" "$REPO_DIR/config" \
		"$ROOT/usr/lib/pirostats/"
	install -m755 "$REPO_DIR/pirostats" "$ROOT/usr/lib/pirostats/pirostats"
	ln -sfn /usr/lib/pirostats/pirostats "$ROOT/usr/bin/pirostats"
}

stage_python_rollback
stage_native
assert_native_layout
printf 'stale upgrade file\n' > "$ROOT/usr/lib/pirostats/stale"
stage_native
assert_native_layout
test ! -e "$ROOT/usr/lib/pirostats/stale"

stage_python_rollback
test -d "$ROOT/usr/lib/pirostats/src"
test -L "$ROOT/usr/bin/pirostats"
test "$(cat "$HOME/.config/pirostats/config.toml")" = "user config"
test "$(cat "$HOME/.cache/pirostats/geom")" = "cache"

stage_native
DESTDIR="$ROOT" HOME="$HOME" "$REPO_DIR/uninstall.sh"
test ! -e "$ROOT/usr/bin/pirostats"
test ! -e "$ROOT/usr/lib/pirostats"
test ! -e "$ROOT/usr/lib/systemd/user/pirostats.service"
test ! -e "$ROOT/usr/share/plasma/plasmoids/com.github.lucazade.pirostats"
test "$(cat "$HOME/.config/pirostats/config.toml")" = "user config"
test "$(cat "$HOME/.cache/pirostats/geom")" = "cache"

# Exercise the PKGBUILD package function against the same compiled candidate.
srcdir="$TMP/aur-src"
pkgdir="$TMP/aur-pkg"
mkdir -p "$srcdir" "$pkgdir"
ln -s "$REPO_DIR" "$srcdir/pirostats"
# shellcheck source=../packaging/aur/PKGBUILD
source "$REPO_DIR/packaging/aur/PKGBUILD"
package
test -x "$pkgdir/usr/bin/pirostats"
test -x "$pkgdir/usr/lib/pirostats/pirostats"
test -f "$pkgdir/usr/share/licenses/pirostats-git/LICENSE"
test -f "$pkgdir/usr/share/licenses/pirostats-git/NOTICE"
test ! -e "$pkgdir/usr/lib/pirostats/src"

echo "P6 package layout, upgrade, rollback, and uninstall checks passed"
