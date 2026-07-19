#!/usr/bin/env bash
# Run the Rust daemon and Plasma applet against disposable per-run XDG roots.
# Nothing is installed under /usr or written to the production PiroStats runtime.
set -euo pipefail

usage() {
	cat <<'EOF'
Usage: tools/qml_verify.sh [--smoke] [--no-build]

Launch an isolated Plasma applet backed by the Rust daemon.

  --smoke     Run a short non-interactive load check, then exit.
  --no-build  Reuse rust/target/release/pirostats.

Without --smoke, close the plasmawindowed window to finish. While open, check
hover tooltip, middle-click pinning, wheel paging, and geometry changes. All
temporary files and the user-local test applet copy are removed on exit.
EOF
}

smoke=false
build=true
for arg in "$@"; do
	case "$arg" in
		--smoke) smoke=true ;;
		--no-build) build=false ;;
		-h|--help) usage; exit 0 ;;
		*) echo "unknown argument: $arg" >&2; usage >&2; exit 2 ;;
	esac
done

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_dir/rust/target/release/pirostats"
original_runtime="${XDG_RUNTIME_DIR:-}"
test_root="$(mktemp -d /tmp/pirostats-qml-verify.XXXXXX)"
daemon_pid=""
qml_pid=""

cleanup() {
	if [[ -n "$qml_pid" ]]; then
		kill -TERM "$qml_pid" 2>/dev/null || true
		wait "$qml_pid" 2>/dev/null || true
	fi
	if [[ -n "$daemon_pid" ]]; then
		kill -TERM "$daemon_pid" 2>/dev/null || true
		wait "$daemon_pid" 2>/dev/null || true
	fi
	rm -rf "$test_root"
}
trap cleanup EXIT INT TERM

commands=(kpackagetool6 plasmawindowed python3)
if [[ "$build" == true ]]; then
	commands+=(cargo)
fi
for command in "${commands[@]}"; do
	if ! command -v "$command" >/dev/null 2>&1; then
		echo "required command not found: $command" >&2
		exit 1
	fi
done

if [[ "$build" == true ]]; then
	cargo build --manifest-path "$repo_dir/rust/Cargo.toml" --release --locked
elif [[ ! -x "$binary" ]]; then
	echo "Rust binary not found: $binary" >&2
	exit 1
fi

mkdir -p "$test_root"/{runtime,config,cache,data,home,package,bin}
chmod 700 "$test_root/runtime"
ln -s "$binary" "$test_root/bin/pirostats"

# A Wayland display name is relative to XDG_RUNTIME_DIR. Expose only that socket
# inside the disposable root; X11 sessions need no equivalent setup.
if [[ -n "${WAYLAND_DISPLAY:-}" && -n "$original_runtime" \
	&& -S "$original_runtime/$WAYLAND_DISPLAY" ]]; then
	ln -s "$original_runtime/$WAYLAND_DISPLAY" "$test_root/runtime/$WAYLAND_DISPLAY"
fi

export XDG_RUNTIME_DIR="$test_root/runtime"
export XDG_CONFIG_HOME="$test_root/config"
export XDG_CACHE_HOME="$test_root/cache"
export XDG_DATA_HOME="$test_root/data"
export HOME="$test_root/home"
export PIROSTATS_CODE_ROOT="$repo_dir"

cp -a "$repo_dir/plasmoid/package/." "$test_root/package/"

# Installed defaults intentionally use /usr/bin/pirostats. Change only the
# disposable package copy so wheel/click commands target this test binary.
python3 - "$test_root/package/contents/config/main.xml" "$test_root/bin/pirostats" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
path.write_text(
    path.read_text(encoding="utf-8").replace("/usr/bin/pirostats", sys.argv[2]),
    encoding="utf-8",
)
PY

kpackagetool6 --type Plasma/Applet --install "$test_root/package" >/dev/null

"$binary" daemon >"$test_root/daemon.log" 2>&1 &
daemon_pid=$!

wait_for_file() {
	local path="$1"
	for _ in $(seq 1 100); do
		[[ -s "$path" ]] && return 0
		kill -0 "$daemon_pid" 2>/dev/null || break
		sleep 0.05
	done
	echo "timed out waiting for $path" >&2
	cat "$test_root/daemon.log" >&2
	return 1
}

runtime_root="$XDG_RUNTIME_DIR/pirostats"
wait_for_file "$runtime_root/panel.html"
wait_for_file "$runtime_root/tooltip.html"

expected_entries=$'panel.html\nstate\ntooltip.html'
actual_entries=""
for _ in $(seq 1 40); do
	actual_entries="$(find "$runtime_root" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort)"
	[[ "$actual_entries" == "$expected_entries" ]] && break
	sleep 0.05
done
if [[ "$actual_entries" != "$expected_entries" ]]; then
	echo "unexpected runtime-root entries:" >&2
	printf '%s\n' "$actual_entries" >&2
	exit 1
fi

plasmawindowed com.github.lucazade.pirostats >"$test_root/qml.log" 2>&1 &
qml_pid=$!

if [[ "$smoke" == true ]]; then
	sleep 3
	if ! kill -0 "$qml_pid" 2>/dev/null; then
		echo "plasmawindowed exited during smoke test" >&2
		cat "$test_root/qml.log" >&2
		exit 1
	fi
	if grep -Eiq '(^| )(error|fatal):|failed to load|is not installed|ReferenceError|TypeError' "$test_root/qml.log"; then
		echo "QML load errors detected" >&2
		cat "$test_root/qml.log" >&2
		exit 1
	fi
	echo "QML smoke passed: isolated Rust daemon + plasmawindowed applet"
	exit 0
fi

cat <<EOF
Isolated QML verification running.
  test root: $test_root
  runtime:   $runtime_root
  daemon:    $daemon_pid
  applet:    $qml_pid

Check hover tooltip, middle-click pin/unpin, wheel paging, page updates, and
window/geometry changes. Close plasmawindowed to clean up. No system paths or
production runtime files are touched.
EOF

wait "$qml_pid"
qml_pid=""
