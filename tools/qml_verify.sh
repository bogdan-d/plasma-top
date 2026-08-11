#!/usr/bin/env bash
# Run the Rust daemon and Plasma applet against disposable per-run XDG roots.
# Nothing is installed under /usr or written to the production PlasmaTop runtime.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: tools/qml_verify.sh [--smoke] [--no-build]

Launch an isolated Plasma applet backed by the Rust daemon.

  --smoke     Run a short non-interactive load check, then exit.
  --no-build  Reuse target/release/plasma-top.

Without --smoke, close the plasmawindowed window to finish an Application-form inspection.
`plasmawindowed` cannot emulate panel form factors; use tools/plasma_live_matrix.sh for horizontal/vertical interaction checks.
All temporary files and the user-local test applet copy are removed on exit.
EOF
}

smoke=false
build=true
for arg in "$@"; do
    case "$arg" in
    --smoke) smoke=true ;;
    --no-build) build=false ;;
    -h | --help)
        usage
        exit 0
        ;;
    *)
        echo "unknown argument: $arg" >&2
        usage >&2
        exit 2
        ;;
    esac
done

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_dir/target/release/plasma-top"
original_runtime="${XDG_RUNTIME_DIR:-}"
test_root="$(mktemp -d /tmp/plasma-top-qml-verify.XXXXXX)"
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
    cargo build --manifest-path "$repo_dir/Cargo.toml" --release --locked
elif [[ ! -x "$binary" ]]; then
    echo "Rust binary not found: $binary" >&2
    exit 1
fi

mkdir -p "$test_root"/{runtime,config,cache,data,home,package,bin}
chmod 700 "$test_root/runtime"
export PLASMA_TOP_QML_TRACE="$test_root/commands.tsv"
export PATH="$test_root/bin:$PATH"

cat >"$test_root/bin/plasma-top" <<EOF
#!/usr/bin/env bash
printf '%s\tplasma-top\t%s\n' "\$(date +%s.%N)" "\$*" >>"\$PLASMA_TOP_QML_TRACE"
exec "$binary" "\$@"
EOF
cat >"$test_root/bin/cat" <<'EOF'
#!/usr/bin/env bash
printf '%s\tcat\t%s\n' "$(date +%s.%N)" "$*" >>"$PLASMA_TOP_QML_TRACE"
exec /usr/bin/cat "$@"
EOF
chmod +x "$test_root/bin/plasma-top"
chmod +x "$test_root/bin/cat"

# A Wayland display name is relative to XDG_RUNTIME_DIR. Expose only that socket
# inside the disposable root; X11 sessions need no equivalent setup.
if [[ -n "${WAYLAND_DISPLAY:-}" && -n "$original_runtime" &&
    -S "$original_runtime/$WAYLAND_DISPLAY" ]]; then
    ln -s "$original_runtime/$WAYLAND_DISPLAY" "$test_root/runtime/$WAYLAND_DISPLAY"
fi

export XDG_RUNTIME_DIR="$test_root/runtime"
export XDG_CONFIG_HOME="$test_root/config"
export XDG_CACHE_HOME="$test_root/cache"
export XDG_DATA_HOME="$test_root/data"
export HOME="$test_root/home"
export PLASMA_TOP_CODE_ROOT="$repo_dir"

cp -a "$repo_dir/plasmoid/package/." "$test_root/package/"

# Installed defaults intentionally use /usr/bin/plasma-top. Change only the
# disposable package copy so wheel/click commands target this test binary.
python3 - "$test_root/package/contents/config/main.xml" "$test_root/bin/plasma-top" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
path.write_text(
    path.read_text(encoding="utf-8").replace("/usr/bin/plasma-top", sys.argv[2]),
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

runtime_root="$XDG_RUNTIME_DIR/plasma-top"
wait_for_file "$runtime_root/panel.html"

expire_stale_lease() {
    local lease_path="$1"
    [[ -e "$lease_path" ]] || return 0
    touch -d '2000-01-01 00:00:00 UTC' "$lease_path" 2>/dev/null || {
        [[ ! -e "$lease_path" ]] && return 0
        return 1
    }
    for _ in $(seq 1 120); do
        [[ ! -e "$lease_path" ]] && return 0
        kill -0 "$daemon_pid" 2>/dev/null || break
        sleep 0.05
    done
    echo "daemon did not expire stale presentation lease: $lease_path" >&2
    return 1
}

unexpected_entries=""
for _ in $(seq 1 40); do
    unexpected_entries="$(find "$runtime_root" -mindepth 1 -maxdepth 1 \! -name panel.html \! -name tooltip.html \! -name state -printf '%f\n' | sort)"
    [[ -z "$unexpected_entries" ]] && break
    sleep 0.05
done
if [[ -n "$unexpected_entries" ]]; then
    echo "unexpected runtime-root entries:" >&2
    printf '%s\n' "$unexpected_entries" >&2
    exit 1
fi

plasmawindowed com.github.bogdan-d.plasma-top >"$test_root/qml.log" 2>&1 &
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
    lease="$(find "$runtime_root/state/presented" -maxdepth 1 -type f -printf '%f\n' 2>/dev/null | awk '/^[1-9][0-9]*$/ { print; exit }')"
    if grep -Eq $'\tplasma-top\tpresent [1-9][0-9]*$' "$PLASMA_TOP_QML_TRACE"; then
        [[ -n "$lease" ]] || {
            echo "QML present command did not create a numeric lease" >&2
            exit 1
        }
        wait_for_file "$runtime_root/tooltip.html"
    elif grep -Eq $'\tcat\t.*/tooltip\.html$' "$PLASMA_TOP_QML_TRACE"; then
        echo "hidden QML read tooltip.html at startup" >&2
        cat "$PLASMA_TOP_QML_TRACE" >&2
        exit 1
    fi
    kill -TERM "$qml_pid"
    wait "$qml_pid" 2>/dev/null || true
    qml_pid=""
    if [[ -n "$lease" && -e "$runtime_root/state/presented/$lease" ]]; then
        expire_stale_lease "$runtime_root/state/presented/$lease"
    fi
    stale_test_lease="$runtime_root/state/presented/987654"
    "$test_root/bin/plasma-top" present 987654
    [[ -e "$stale_test_lease" ]] || {
        echo "failed to create disposable crash-expiry lease" >&2
        exit 1
    }
    expire_stale_lease "$stale_test_lease"
    if ! grep -Fq 'Component.onDestruction:' "$test_root/package/contents/ui/PresentationLease.qml" ||
        ! grep -Fq 'command("dismiss")' "$test_root/package/contents/ui/PresentationLease.qml"; then
        echo "best-effort clean-removal dismiss callback missing" >&2
        exit 1
    fi
    echo "QML smoke passed: hidden reads gated; crash leases expire; clean-removal callback present"
    exit 0
fi

cat <<EOF
Isolated QML verification running.
  test root: $test_root
  runtime:   $runtime_root
  daemon:    $daemon_pid
  applet:    $qml_pid

Application-form inspection only; this host does not exercise compact panel behavior.
Close plasmawindowed to clean up. Run tools/plasma_live_matrix.sh for horizontal/vertical geometry, hover, pinning, and wheel evidence.
No system paths or production runtime files are touched.
EOF

wait "$qml_pid"
qml_pid=""
