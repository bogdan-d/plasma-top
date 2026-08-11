#!/usr/bin/env bash
# Verify the unchanged Plasma applet against a daemon in disposable roots.
# shellcheck disable=SC2097,SC2098
# distrobox starts with host XDG/HOME to reach host sockets; the viewer inside
# is handed disposable roots via `env`, so the temp assignments and the
# re-expansions are deliberate and do not see each other.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: tools/plasma_live_matrix.sh [--no-build] [--interactive|--planar]

Uses KDE's plasmoidviewer to exercise real horizontal and vertical compact representations, then its planar representation.
Automatic checks prove load, presentation leases, geometry publication, orientation, watcher refresh, lazy tooltip reads, planar activation, multiple instances, and runtime-root discipline.
On X11, automatic checks also exercise real hover presentation and dismissal; Wayland records this check as skipped because plasmoidviewer ignores requested window coordinates.
--interactive keeps a horizontal instance open for hover/pin/wheel/resize validation.
--planar opens the desktop form for background/outline/font/config-page validation.

Evidence is written to .test-artifacts/plasma/live/.
No system or production runtime path is modified.
On immutable hosts, plasmoidviewer may be a Distrobox export or supplied by an existing Distrobox named plasma-top-plasma-sdk.
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
    -h | --help)
        usage
        exit 0
        ;;
    *)
        echo "unknown argument: $1" >&2
        usage >&2
        exit 2
        ;;
    esac
    shift
done

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_dir/target/release/plasma-top"
artifact_root="$repo_dir/.test-artifacts/plasma/live"
original_runtime="${XDG_RUNTIME_DIR:-}"
original_home="$HOME"
original_config="${XDG_CONFIG_HOME:-$HOME/.config}"
original_cache="${XDG_CACHE_HOME:-$HOME/.cache}"
original_data="${XDG_DATA_HOME:-$HOME/.local/share}"
viewer_container="plasma-top-plasma-sdk"
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

if command -v distrobox >/dev/null 2>&1 &&
    distrobox list 2>/dev/null | grep -q "| $viewer_container "; then
    viewer="distrobox"
elif command -v plasmoidviewer >/dev/null 2>&1; then
    viewer_path="$(command -v plasmoidviewer)"
    if grep -Fqx '# distrobox_binary' "$viewer_path" 2>/dev/null; then
        exported_container="$(awk '$1 == "#" && $2 == "name:" { print $3; exit }' "$viewer_path")"
        if [[ -n "$exported_container" ]] && ! distrobox list 2>/dev/null | grep -q "| $exported_container "; then
            echo "plasmoidviewer export targets missing Distrobox: $exported_container" >&2
            exit 1
        fi
    fi
    viewer="host"
else
    echo "plasmoidviewer unavailable: install plasma-sdk or provide existing Distrobox $viewer_container" >&2
    exit 1
fi
for command in python3 awk; do
    command -v "$command" >/dev/null 2>&1 || {
        echo "required command not found: $command" >&2
        exit 1
    }
done
if [[ "$build" == true ]]; then
    cargo build --manifest-path "$repo_dir/Cargo.toml" --release --locked
fi
[[ -x "$binary" ]] || {
    echo "Rust binary not found: $binary" >&2
    exit 1
}

rm -rf "$artifact_root"
mkdir -p "$artifact_root"
test_root="$(mktemp -d "$artifact_root/run.XXXXXX")"
mkdir -p "$test_root"/{runtime,config/plasma-top,cache,data,home,package,bin,logs}
chmod 700 "$test_root/runtime"

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
export PLASMA_TOP_QML_TRACE="$test_root/commands.tsv"
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

cp "$repo_dir/config/config.toml" "$XDG_CONFIG_HOME/plasma-top/config.toml"
cp "$repo_dir/config/machines.toml" "$XDG_CONFIG_HOME/plasma-top/machines.toml"
cp -a "$repo_dir/plasmoid/package/." "$test_root/package/"

# Trace QML's shell-backed reads/actions without changing applet code.
cat >"$test_root/bin/cat" <<'EOF'
#!/usr/bin/env bash
printf '%s\tcat\t%s\n' "$(date +%s.%N)" "$*" >>"$PLASMA_TOP_QML_TRACE"
exec /usr/bin/cat "$@"
EOF
cat >"$test_root/bin/backend" <<EOF
#!/usr/bin/env bash
exec "$binary" "\$@"
EOF
cat >"$test_root/bin/plasma-top" <<EOF
#!/usr/bin/env bash
printf '%s\\tplasma-top\\t%s\\n' "\$(date +%s.%N)" "\$*" >>"\$PLASMA_TOP_QML_TRACE"
failure_marker="$test_root/fail-next-\$1"
if [[ -e "\$failure_marker" ]]; then
    rm -f "\$failure_marker"
    exit 1
fi
exec "$test_root/bin/backend" "\$@"
EOF
chmod +x "$test_root/bin/backend" "$test_root/bin/cat" "$test_root/bin/plasma-top"

python3 - "$test_root/package/contents/config/main.xml" "$test_root/bin/plasma-top" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = "/usr/bin/plasma-top"
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

"$test_root/bin/backend" daemon >"$test_root/logs/daemon.log" 2>&1 &
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

runtime_root="$XDG_RUNTIME_DIR/plasma-top"
state_root="$runtime_root/state"
wait_for_file "$runtime_root/panel.html"

start_viewer() {
    local formfactor="$1" location="$2" size="$3" log="$4"
    # Keep automatic windows away from usual pointer/top-panel positions so an
    # accidental hover cannot invalidate the lazy-tooltip assertion.
    local args=(-a com.github.bogdan-d.plasma-top -f "$formfactor" -l "$location" -s "$size" -x 1000 -y 1000)
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
            PLASMA_TOP_CODE_ROOT="$PLASMA_TOP_CODE_ROOT" \
            PLASMA_TOP_QML_TRACE="$PLASMA_TOP_QML_TRACE" \
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
    awk -F '\t' -v suffix="/$name" '$2 == "cat" && $3 ~ suffix "$" {n++} END {print n+0}' "$PLASMA_TOP_QML_TRACE"
}

count_actions() {
    local action="$1"
    awk -F '\t' -v action="$action" '$2 == "plasma-top" && $3 ~ "^" action " [1-9][0-9]*$" {n++} END {print n+0}' "$PLASMA_TOP_QML_TRACE"
}

lease_count() {
    find "$state_root/presented" -maxdepth 1 -type f -printf '%f\n' 2>/dev/null | awk '/^[1-9][0-9]*$/ {n++} END {print n+0}'
}

wait_for_no_leases() {
    for _ in $(seq 1 120); do
        [[ "$(lease_count)" == 0 ]] && return 0
        kill -0 "$daemon_pid" 2>/dev/null || break
        sleep 0.05
    done
    return 1
}

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
    return 1
}

wait_for_tooltip_change() {
    local before="$1"
    for _ in $(seq 1 120); do
        [[ "$(stat -c %y "$runtime_root/tooltip.html")" != "$before" ]] && return 0
        kill -0 "$daemon_pid" 2>/dev/null || break
        sleep 0.05
    done
    return 1
}

verify_qml_static_contract() {
    local lease_qml="$repo_dir/plasmoid/package/contents/ui/PresentationLease.qml"
    grep -Fq 'interval: 30000' "$lease_qml"
    grep -Fq 'running: root.presented' "$lease_qml"
    grep -Fq 'property int retriesRemaining: 1' "$lease_qml"
    grep -Fq 'Component.onDestruction:' "$lease_qml"
    grep -Fq 'command("dismiss")' "$lease_qml"
    echo "PASS QML heartbeat presented-only=30s retry_budget=one clean-removal dismiss=best-effort" >>"$artifact_root/automatic.txt"
}

verify_compact_case() {
    local formfactor="$1" location="$2" size="$3" expected_vertical="$4"
    local log="$test_root/logs/$formfactor.log"
    rm -f "$state_root/geom"
    : >"$PLASMA_TOP_QML_TRACE"
    if [[ "$formfactor" == horizontal ]]; then
        : >"$test_root/fail-next-dismiss"
    fi
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
        cp "$PLASMA_TOP_QML_TRACE" "$artifact_root/commands-$formfactor-failed.tsv"
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
    [[ "$usable" != 0 && "$advance" != 0 && "$tooltip_advance" != 0 &&
        "$vertical" == "$expected_vertical" ]] || {
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
    [[ "$tooltip_before" == 0 && "$tooltip_after" == 0 ]] || {
        echo "$formfactor tooltip read while neither hovered nor pinned" >&2
        cp "$PLASMA_TOP_QML_TRACE" "$artifact_root/commands-$formfactor-failed.tsv"
        return 1
    }
    [[ "$(count_actions present)" == 0 && "$(lease_count)" == 0 ]] || {
        echo "$formfactor reported presentation while hidden" >&2
        return 1
    }
    if [[ "$formfactor" == horizontal ]]; then
        (("$(count_actions dismiss)" == 2)) || {
            echo "dismiss failure did not receive exactly one prompt retry" >&2
            return 1
        }
    fi

    if [[ "$formfactor" == horizontal && -z "${WAYLAND_DISPLAY:-}" ]]; then
        tooltip_before="$(count_reads tooltip.html)"
        YDOTOOL_SOCKET="$ydotool_socket" ydotool mousemove --absolute -x 1050 -y 1040
        for _ in $(seq 1 120); do
            if (("$(count_actions present)" > 0)) && (("$(lease_count)" > 0)) && (("$(count_reads tooltip.html)" > tooltip_before)); then
                break
            fi
            sleep 0.05
        done
        if ! (("$(count_actions present)" > 0)) || ! (("$(lease_count)" > 0)) || ! (("$(count_reads tooltip.html)" > tooltip_before)); then
            echo "horizontal hover did not present and read retained tooltip" >&2
            return 1
        fi
        YDOTOOL_SOCKET="$ydotool_socket" ydotool mousemove --absolute -x 0 -y 1700
        wait_for_no_leases || {
            echo "horizontal hover exit did not dismiss presentation" >&2
            return 1
        }
        echo "PASS horizontal hover presentation=create,read,dismiss" >>"$artifact_root/automatic.txt"
    elif [[ "$formfactor" == horizontal ]]; then
        echo "SKIP horizontal hover automatic: Wayland ignores plasmoidviewer coordinates" >>"$artifact_root/automatic.txt"
    fi

    cp "$PLASMA_TOP_QML_TRACE" "$artifact_root/commands-$formfactor.tsv"
    cp "$state_root/geom" "$artifact_root/geom-$formfactor"
    cp "$log" "$artifact_root/$formfactor.log"
    printf 'PASS %s geometry=%s panel_reads=%s->%s tooltip_reads=%s->%s\n' \
        "$formfactor" "$(tr -d '\n' <"$state_root/geom")" \
        "$panel_before" "$panel_after" "$tooltip_before" "$tooltip_after" \
        >>"$artifact_root/automatic.txt"
    stop_viewer
}

verify_planar_case() {
    local log="$test_root/logs/planar.log"
    : >"$PLASMA_TOP_QML_TRACE"
    : >"$test_root/fail-next-present"
    start_viewer planar desktop 900x900 "$log"

    for _ in $(seq 1 120); do
        if (($(lease_count) > 0)) && [[ -s "$runtime_root/tooltip.html" ]]; then
            break
        fi
        kill -0 "$viewer_pid" 2>/dev/null || break
        sleep 0.05
    done
    kill -0 "$viewer_pid" 2>/dev/null || {
        echo "planar plasmoidviewer exited during launch" >&2
        cat "$log" >&2
        return 1
    }
    (($(lease_count) > 0)) || {
        echo "planar QML did not create a presentation lease" >&2
        return 1
    }
    [[ -s "$runtime_root/tooltip.html" ]] || {
        echo "daemon did not publish tooltip.html for planar presentation" >&2
        return 1
    }
    (($(count_actions present) > 0)) || {
        echo "planar present command missing from trace" >&2
        return 1
    }
    (($(count_actions present) == 2)) || {
        echo "present failure did not receive exactly one prompt retry" >&2
        return 1
    }
    sleep 0.2
    (($(count_reads tooltip.html) > 0)) || {
        echo "planar QML did not read retained tooltip" >&2
        return 1
    }
    if grep -Eiq '(^| )(error|fatal):|failed to load|is not installed|ReferenceError|TypeError|binding loop' "$log"; then
        echo "planar QML errors detected" >&2
        cat "$log" >&2
        return 1
    fi

    cp "$PLASMA_TOP_QML_TRACE" "$artifact_root/commands-planar.tsv"
    cp "$log" "$artifact_root/planar.log"
    lease="$(find "$state_root/presented" -maxdepth 1 -type f -printf '%f\n' 2>/dev/null | awk '/^[1-9][0-9]*$/ { print; exit }')"
    stop_viewer
    if [[ -n "$lease" && -e "$state_root/presented/$lease" ]] && ! expire_stale_lease "$state_root/presented/$lease"; then
        echo "daemon did not expire planar crash lease" >&2
        return 1
    fi
    echo "PASS planar presentation lease=create,read crash=expired-if-retained present_retry=one" >>"$artifact_root/automatic.txt"
}

verify_multiple_leases() {
    "$test_root/bin/plasma-top" present 900001
    "$test_root/bin/plasma-top" present 900002
    [[ -e "$state_root/presented/900001" && -e "$state_root/presented/900002" ]] || {
        echo "multiple presentation leases were not created" >&2
        return 1
    }
    "$test_root/bin/plasma-top" dismiss 900001
    [[ ! -e "$state_root/presented/900001" && -e "$state_root/presented/900002" ]] || {
        echo "dismissing one instance disturbed another lease" >&2
        return 1
    }
    tooltip_before="$(stat -c %y "$runtime_root/tooltip.html")"
    "$test_root/bin/plasma-top" page next
    wait_for_tooltip_change "$tooltip_before" || {
        echo "daemon stopped tooltip publication while second lease remained" >&2
        return 1
    }
    "$test_root/bin/plasma-top" dismiss 900002
    [[ ! -e "$state_root/presented/900002" ]] || {
        echo "final presentation lease was not removed" >&2
        return 1
    }
    sleep 1.2
    tooltip_before="$(stat -c %y "$runtime_root/tooltip.html")"
    "$test_root/bin/plasma-top" page prev
    sleep 0.5
    [[ "$(stat -c %y "$runtime_root/tooltip.html")" == "$tooltip_before" ]] || {
        echo "daemon published tooltip after final lease dismissal" >&2
        return 1
    }
    echo "PASS multiple presentation leases active-after-one hidden-after-last" >>"$artifact_root/automatic.txt"
}

seed_retained_tooltip() {
    "$test_root/bin/plasma-top" present 990000
    wait_for_file "$runtime_root/tooltip.html"
    "$test_root/bin/plasma-top" dismiss 990000
    wait_for_no_leases
    sleep 1.2
}

verify_stale_expiry() {
    local lease_path="$state_root/presented/900003"
    "$test_root/bin/plasma-top" present 900003
    [[ -e "$lease_path" ]] || return 1
    expire_stale_lease "$lease_path"
    echo "PASS stale presentation lease expired after safe backdate" >>"$artifact_root/automatic.txt"
}

: >"$artifact_root/automatic.txt"
verify_qml_static_contract
seed_retained_tooltip
verify_compact_case horizontal topedge 1200x80 0
verify_compact_case vertical leftedge 80x1200 1
verify_planar_case
verify_multiple_leases
verify_stale_expiry

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
    echo "Plasma live automatic matrix passed: ${artifact_root#"$repo_dir"/}/automatic.txt"
    exit 0
fi

rm -f "$state_root/geom"
: >"$PLASMA_TOP_QML_TRACE"
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
  3. Right-click stats → Configure PlasmaTop → Appearance. Toggle background,
     desktop outline, and font size; each applies cleanly without clipping.
  4. Wheel paging changes one page per gesture and content remains aligned.
  5. Inspect .test-artifacts/plasma/qt/contact-sheet.png for dark/light/overlay pages.

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
  5. Inspect .test-artifacts/plasma/qt/contact-sheet.png.

Close plasmoidviewer when finished. Evidence root: $artifact_root
EOF
fi
wait "$viewer_pid"
viewer_pid=""
cp "$PLASMA_TOP_QML_TRACE" "$artifact_root/commands-interactive.tsv"
cp "$test_root/logs/interactive.log" "$artifact_root/interactive.log"
