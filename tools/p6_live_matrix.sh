#!/usr/bin/env bash
# Verify the unchanged Plasma applet against the Rust daemon in disposable roots.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: tools/p6_live_matrix.sh [--no-build] [--interactive|--planar]

Uses KDE's plasmoidviewer to exercise real horizontal and vertical compact
representations, then its planar representation. Automatic checks prove load,
QML-owned geometry publication, orientation, watcher refresh, lazy tooltip
reads, and runtime-root discipline. --interactive keeps a horizontal instance
open for hover/pin/wheel/resize validation. --planar opens the desktop form for
background/outline/font/config-page validation.

Evidence is written to .test-artifacts/p6/live/. No system or production runtime
path is modified. On immutable hosts, plasmoidviewer may be a Distrobox export.
EOF
}

build=true
interactive=false
planar=false
while (($#)); do
    case "$1" in
        --no-build) build=false ;;
        --interactive) interactive=true ;;
        --planar) planar=true ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_dir/rust/target/release/pirostats"
artifact_root="$repo_dir/.test-artifacts/p6/live"
original_runtime="${XDG_RUNTIME_DIR:-}"
original_home="$HOME"
original_config="${XDG_CONFIG_HOME:-$HOME/.config}"
original_cache="${XDG_CACHE_HOME:-$HOME/.cache}"
original_data="${XDG_DATA_HOME:-$HOME/.local/share}"
viewer_container="pirostats-plasma-sdk"
viewer=""
daemon_pid=""
viewer_pid=""
ydotool_pid=""
ydotool_socket=""

kill_container_viewers() {
    [[ "$viewer" == distrobox ]] || return 0
    XDG_RUNTIME_DIR="$original_runtime" \
        XDG_CONFIG_HOME="$original_config" \
        XDG_CACHE_HOME="$original_cache" \
        XDG_DATA_HOME="$original_data" \
        HOME="$original_home" \
        distrobox enter "$viewer_container" -- pkill -TERM -x plasmoidviewer \
        >/dev/null 2>&1 || true
}

cleanup() {
    kill_container_viewers
    if [[ -n "$viewer_pid" ]]; then
        kill -TERM "$viewer_pid" 2>/dev/null || true
        wait "$viewer_pid" 2>/dev/null || true
    fi
    if [[ -n "$daemon_pid" ]]; then
        kill -TERM "$daemon_pid" 2>/dev/null || true
        wait "$daemon_pid" 2>/dev/null || true
    fi
    if [[ -n "$ydotool_pid" ]]; then
        kill -TERM "$ydotool_pid" 2>/dev/null || true
        wait "$ydotool_pid" 2>/dev/null || true
    fi
    [[ -n "${test_root:-}" ]] && rm -rf "$test_root"
}
trap cleanup EXIT INT TERM

if command -v distrobox >/dev/null 2>&1 \
    && distrobox list 2>/dev/null | grep -q "| $viewer_container "; then
    viewer="distrobox"
elif command -v plasmoidviewer >/dev/null 2>&1; then
    viewer="host"
else
    echo "plasmoidviewer unavailable (install plasma-sdk or create $viewer_container)" >&2
    exit 1
fi
for command in python3 awk; do
    command -v "$command" >/dev/null 2>&1 || {
        echo "required command not found: $command" >&2
        exit 1
    }
done
if [[ "$build" == true ]]; then
    cargo build --manifest-path "$repo_dir/rust/Cargo.toml" --release --locked
fi
[[ -x "$binary" ]] || { echo "Rust binary not found: $binary" >&2; exit 1; }

rm -rf "$artifact_root"
mkdir -p "$artifact_root"
test_root="$(mktemp -d "$artifact_root/run.XXXXXX")"
mkdir -p "$test_root"/{runtime,config/pirostats,cache,data,home,package,bin,logs}
chmod 700 "$test_root/runtime"

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
export PIROSTATS_QML_TRACE="$test_root/commands.tsv"
export PATH="$test_root/bin:$PATH"

# Wayland does not honor plasmoidviewer's requested x/y coordinates. Move the
# pointer away before automatic cases so hover cannot open the tooltip gate.
if command -v ydotoold >/dev/null 2>&1 && command -v ydotool >/dev/null 2>&1; then
    ydotool_socket="$test_root/ydotool.sock"
    ydotoold -p "$ydotool_socket" -P 0600 >"$test_root/logs/ydotoold.log" 2>&1 &
    ydotool_pid=$!
    sleep 0.3
    kill -0 "$ydotool_pid" 2>/dev/null || {
        echo "ydotoold failed to start" >&2
        exit 1
    }
else
    echo "ydotool/ydotoold required for controlled lazy-tooltip verification" >&2
    exit 1
fi

cp "$repo_dir/config/config.toml" "$XDG_CONFIG_HOME/pirostats/config.toml"
cp "$repo_dir/config/machines.toml" "$XDG_CONFIG_HOME/pirostats/machines.toml"
cp -a "$repo_dir/plasmoid/package/." "$test_root/package/"

# Trace QML's shell-backed reads/actions without changing applet code.
cat >"$test_root/bin/cat" <<'EOF'
#!/usr/bin/env bash
printf '%s\tcat\t%s\n' "$(date +%s.%N)" "$*" >>"$PIROSTATS_QML_TRACE"
exec /usr/bin/cat "$@"
EOF
cat >"$test_root/bin/pirostats" <<EOF
#!/usr/bin/env bash
printf '%s\\tpirostats\\t%s\\n' "\$(date +%s.%N)" "\$*" >>"\$PIROSTATS_QML_TRACE"
exec "$binary" "\$@"
EOF
chmod +x "$test_root/bin/cat" "$test_root/bin/pirostats"

python3 - "$test_root/package/contents/config/main.xml" "$test_root/bin/pirostats" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "/usr/bin/pirostats"
if old not in text:
    raise SystemExit(f"expected action path missing from {path}")
path.write_text(text.replace(old, sys.argv[2]), encoding="utf-8")
PY

if [[ "$viewer" == host ]]; then
    kpackagetool6 --type Plasma/Applet --install "$test_root/package" >/dev/null
else
    XDG_RUNTIME_DIR="$original_runtime" \
        XDG_CONFIG_HOME="$original_config" \
        XDG_CACHE_HOME="$original_cache" \
        XDG_DATA_HOME="$original_data" \
        HOME="$original_home" \
        distrobox enter "$viewer_container" -- env \
        XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
        XDG_CONFIG_HOME="$XDG_CONFIG_HOME" \
        XDG_CACHE_HOME="$XDG_CACHE_HOME" \
        XDG_DATA_HOME="$XDG_DATA_HOME" \
        HOME="$HOME" \
        kpackagetool6 --type Plasma/Applet --install "$test_root/package" >/dev/null
fi

"$binary" daemon >"$test_root/logs/daemon.log" 2>&1 &
daemon_pid=$!

wait_for_file() {
    local path="$1"
    for _ in $(seq 1 160); do
        [[ -s "$path" ]] && return 0
        kill -0 "$daemon_pid" 2>/dev/null || break
        sleep 0.05
    done
    echo "timed out waiting for $path" >&2
    return 1
}

runtime_root="$XDG_RUNTIME_DIR/pirostats"
state_root="$runtime_root/state"
wait_for_file "$runtime_root/panel.html"
wait_for_file "$runtime_root/tooltip.html"

start_viewer() {
    local formfactor="$1" location="$2" size="$3" log="$4"
    # Keep automatic windows away from usual pointer/top-panel positions so an
    # accidental hover cannot invalidate the lazy-tooltip assertion.
    local args=(-a com.github.lucazade.pirostats -f "$formfactor" -l "$location" -s "$size" -x 1000 -y 1000)
    if [[ "$viewer" == host ]]; then
        plasmoidviewer "${args[@]}" >"$log" 2>&1 &
    else
        # Distrobox itself needs host runtime/HOME; only the contained viewer
        # receives disposable roots. Repository/artifact paths are host-mounted.
        XDG_RUNTIME_DIR="$original_runtime" \
            XDG_CONFIG_HOME="$original_config" \
            XDG_CACHE_HOME="$original_cache" \
            XDG_DATA_HOME="$original_data" \
            HOME="$original_home" \
            distrobox enter "$viewer_container" -- env \
            XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
            XDG_CONFIG_HOME="$XDG_CONFIG_HOME" \
            XDG_CACHE_HOME="$XDG_CACHE_HOME" \
            XDG_DATA_HOME="$XDG_DATA_HOME" \
            HOME="$HOME" \
            PIROSTATS_CODE_ROOT="$PIROSTATS_CODE_ROOT" \
            PIROSTATS_QML_TRACE="$PIROSTATS_QML_TRACE" \
            PATH="$test_root/bin:/usr/local/bin:/usr/bin:/bin" \
            plasmoidviewer "${args[@]}" >"$log" 2>&1 &
    fi
    viewer_pid=$!
}

stop_viewer() {
    kill_container_viewers
    kill -TERM "$viewer_pid" 2>/dev/null || true
    wait "$viewer_pid" 2>/dev/null || true
    viewer_pid=""
}

count_reads() {
    local name="$1"
    awk -F '\t' -v suffix="/$name" '$2 == "cat" && $3 ~ suffix "$" {n++} END {print n+0}' "$PIROSTATS_QML_TRACE"
}

verify_compact_case() {
    local formfactor="$1" location="$2" size="$3" expected_vertical="$4"
    local log="$test_root/logs/$formfactor.log"
    rm -f "$state_root/geom"
    : >"$PIROSTATS_QML_TRACE"
    YDOTOOL_SOCKET="$ydotool_socket" ydotool mousemove --absolute -x 4000 -y 1700
    start_viewer "$formfactor" "$location" "$size" "$log"
    sleep 0.5
    YDOTOOL_SOCKET="$ydotool_socket" ydotool mousemove --absolute -x 0 -y 1700

    for _ in $(seq 1 120); do
        [[ -s "$state_root/geom" ]] && break
        kill -0 "$viewer_pid" 2>/dev/null || break
        sleep 0.05
    done
    [[ -s "$state_root/geom" ]] || {
        echo "$formfactor QML did not publish state/geom" >&2
        cp "$log" "$artifact_root/$formfactor-failed.log"
        cp "$PIROSTATS_QML_TRACE" "$artifact_root/commands-$formfactor-failed.tsv"
        find "$runtime_root" -maxdepth 2 -printf '%y %p\n' >"$artifact_root/runtime-$formfactor-failed.txt"
        cat "$log" >&2
        return 1
    }
    kill -0 "$viewer_pid" 2>/dev/null || {
        echo "$formfactor plasmoidviewer exited during launch" >&2
        cat "$log" >&2
        return 1
    }
    if grep -Eiq '(^| )(error|fatal):|failed to load|is not installed|ReferenceError|TypeError|binding loop' "$log"; then
        echo "$formfactor QML errors detected" >&2
        cat "$log" >&2
        return 1
    fi
    read -r usable advance vertical tooltip_advance <"$state_root/geom"
    [[ "$usable" != 0 && "$advance" != 0 && "$tooltip_advance" != 0 \
        && "$vertical" == "$expected_vertical" ]] || {
        echo "$formfactor invalid geometry: $(cat "$state_root/geom")" >&2
        return 1
    }

    # Let Component.onCompleted and the first watcher burst settle before
    # measuring steady-state lazy reads.
    sleep 3
    panel_before="$(count_reads panel.html)"
    tooltip_before="$(count_reads tooltip.html)"
    sleep 3
    panel_after="$(count_reads panel.html)"
    tooltip_after="$(count_reads tooltip.html)"
    ((panel_after > panel_before)) || {
        echo "$formfactor watcher did not refresh panel.html" >&2
        return 1
    }
    [[ "$tooltip_after" == "$tooltip_before" ]] || {
        echo "$formfactor tooltip read while neither hovered nor pinned" >&2
        cp "$PIROSTATS_QML_TRACE" "$artifact_root/commands-$formfactor-failed.tsv"
        return 1
    }

    cp "$PIROSTATS_QML_TRACE" "$artifact_root/commands-$formfactor.tsv"
    cp "$state_root/geom" "$artifact_root/geom-$formfactor"
    cp "$log" "$artifact_root/$formfactor.log"
    printf 'PASS %s geometry=%s panel_reads=%s->%s tooltip_reads=%s->%s\n' \
        "$formfactor" "$(tr -d '\n' <"$state_root/geom")" \
        "$panel_before" "$panel_after" "$tooltip_before" "$tooltip_after" \
        >>"$artifact_root/automatic.txt"
    stop_viewer
}

: >"$artifact_root/automatic.txt"
verify_compact_case horizontal topedge 1200x80 0
verify_compact_case vertical leftedge 80x1200 1

expected_entries=$'panel.html\nstate\ntooltip.html'
actual_entries="$(find "$runtime_root" -mindepth 1 -maxdepth 1 -printf '%f\n' | sort)"
[[ "$actual_entries" == "$expected_entries" ]] || {
    echo "unexpected runtime-root entries:" >&2
    printf '%s\n' "$actual_entries" >&2
    exit 1
}
echo "PASS runtime_root entries=panel.html,state,tooltip.html" >>"$artifact_root/automatic.txt"
cp "$test_root/logs/daemon.log" "$artifact_root/daemon.log"

if [[ "$interactive" == false && "$planar" == false ]]; then
    echo "P6 live automatic matrix passed: ${artifact_root#$repo_dir/}/automatic.txt"
    exit 0
fi

rm -f "$state_root/geom"
: >"$PIROSTATS_QML_TRACE"
if [[ "$planar" == true ]]; then
    start_viewer planar desktop 900x900 "$test_root/logs/interactive.log"
    sleep 2
    kill -0 "$viewer_pid" 2>/dev/null || {
        cat "$test_root/logs/interactive.log" >&2
        exit 1
    }
    cat <<EOF
Automatic horizontal+vertical matrix passed. Planar desktop window remains open.

Manual desktop checklist:
  1. Hide viewer toolbar with its far-right slashed-eye button.
  2. Confirm transparent desktop text is readable on the wallpaper.
  3. Right-click stats → Configure PiroStats → Appearance. Toggle background,
     desktop outline, and font size; each applies cleanly without clipping.
  4. Wheel paging changes one page per gesture and content remains aligned.
  5. Inspect .test-artifacts/p6/qt/contact-sheet.png for dark/light/overlay pages.

Close plasmoidviewer when finished. Evidence root: $artifact_root
EOF
else
    start_viewer horizontal topedge 1200x80 "$test_root/logs/interactive.log"
    wait_for_file "$state_root/geom"
    cat <<EOF
Automatic horizontal+vertical matrix passed. Horizontal window remains open.

Manual interaction checklist:
  1. Hover panel: tooltip appears, aligns, and updates.
  2. Middle-click panel: persistent popup opens; middle-click again closes it.
  3. Scroll one burst: exactly one page change. Pause >200 ms; scroll again:
     exactly one further page change. Quick reverse must still work.
  4. Resize window: panel remains readable and $state_root/geom changes.
  5. Inspect .test-artifacts/p6/qt/contact-sheet.png.

Close plasmoidviewer when finished. Evidence root: $artifact_root
EOF
fi
wait "$viewer_pid"
viewer_pid=""
cp "$PIROSTATS_QML_TRACE" "$artifact_root/commands-interactive.tsv"
cp "$test_root/logs/interactive.log" "$artifact_root/interactive.log"
