#!/usr/bin/env bash
# Run the Rust daemon and Plasma applet on a private session bus with disposable per-run XDG roots.
# Nothing is installed under /usr, and the live session bus and PlasmaTop daemon are never touched.
set -euo pipefail

usage() {
    cat <<'EOF'
Usage: tools/qml_verify.sh [--smoke|--config-smoke|--tooltip-config-smoke|--graphs-config-smoke] [--no-build]

Launch an isolated Plasma applet backed by the Rust daemon on a private session bus.

  --smoke     Run two widgets, one presented, through paging, geometry, and a daemon restart, then exit.
  --config-smoke  Open the Daemon settings page in the isolated applet.
  --tooltip-config-smoke  Edit the Main tooltip page in the isolated applet.
  --graphs-config-smoke  Edit the Graphs page in the isolated applet.
  --no-build  Reuse target/release/plasma-top.

Without a smoke flag, close the plasmawindowed window to finish an Application-form inspection.
`plasmawindowed` cannot emulate panel form factors; those checks currently require an explicitly approved real-session pass.
All temporary files and the user-local test applet copies are removed on exit.
EOF
}

smoke=false
config_smoke=false
tooltip_config_smoke=false
graphs_config_smoke=false
build=true
for arg in "$@"; do
    case "$arg" in
    --smoke) smoke=true ;;
    --config-smoke)
        smoke=true
        config_smoke=true
        ;;
    --tooltip-config-smoke)
        smoke=true
        tooltip_config_smoke=true
        ;;
    --graphs-config-smoke)
        smoke=true
        graphs_config_smoke=true
        ;;
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
settings_smoke=false
if [[ "$config_smoke" == true || "$tooltip_config_smoke" == true || "$graphs_config_smoke" == true ]]; then
    settings_smoke=true
fi

repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$repo_dir/target/release/plasma-top"
original_runtime="${XDG_RUNTIME_DIR:-}"
test_root="$(mktemp -d /tmp/plasma-top-qml-verify.XXXXXX)"
bus_name=com.github.bogdan_d.PlasmaTop
object_path=/com/github/bogdan_d/PlasmaTop
interface=com.github.bogdan_d.PlasmaTop1
applet_id=com.github.bogdan-d.plasma-top
second_applet_id=com.github.bogdan-d.plasma-top-verify
bus_pid=""
monitor_pid=""
daemon_pid=""
qml_pids=()

stop() {
    [[ -n "$1" ]] || return 0
    kill -TERM "$1" 2>/dev/null || true
    wait "$1" 2>/dev/null || true
}

cleanup() {
    for pid in "${qml_pids[@]}"; do stop "$pid"; done
    stop "$daemon_pid"
    stop "$monitor_pid"
    stop "$bus_pid"
    if [[ -n "$test_root" && "$test_root" == /tmp/plasma-top-qml-verify.* ]]; then
        rm -r --preserve-root=all --one-file-system -- "$test_root"
    fi
}
trap cleanup EXIT INT TERM

commands=(kpackagetool6 plasmawindowed python3 dbus-daemon dbus-monitor dbus-send)
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
chmod +x "$test_root/bin/plasma-top"

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
# QML messages go to the journal unless stderr logging is forced.
export QT_FORCE_STDERR_LOGGING=1

# Everything below, the daemon and both applets included, talks only to this bus. It has no service directories, so nothing is activated that could outlive it.
export DBUS_SESSION_BUS_ADDRESS="unix:path=$test_root/bus"
cat >"$test_root/bus.conf" <<EOF
<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>$DBUS_SESSION_BUS_ADDRESS</listen>
  <auth>EXTERNAL</auth>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
EOF
dbus-daemon --config-file="$test_root/bus.conf" --nofork >/dev/null 2>&1 &
bus_pid=$!
for _ in $(seq 1 100); do
    [[ -S "$test_root/bus" ]] && break
    sleep 0.02
done
[[ -S "$test_root/bus" ]] || {
    echo "private session bus did not start" >&2
    exit 1
}
dbus-monitor --address "$DBUS_SESSION_BUS_ADDRESS" "type='method_call',interface='$interface'" >"$test_root/calls.log" 2>&1 &
monitor_pid=$!

cp -a "$repo_dir/plasmoid/package/." "$test_root/package/"

# Installed defaults intentionally use /usr/bin/plasma-top. Change only the
# disposable package copy so click and settings commands target this test binary.
python3 - "$test_root/package/contents/config/main.xml" "$test_root/bin/plasma-top" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
path.write_text(
    path.read_text(encoding="utf-8").replace("/usr/bin/plasma-top", sys.argv[2]),
    encoding="utf-8",
)
PY

# Appends QML to the end of a component file, inside its root object.
append_qml() {
    python3 - "$1" "$2" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
body, close = path.read_text(encoding="utf-8").rsplit("}", 1)
path.write_text(body + sys.argv[2] + "}" + close, encoding="utf-8")
PY
}

if [[ "$settings_smoke" == true ]]; then
    page_name=ConfigDaemon.qml
    if [[ "$tooltip_config_smoke" == true ]]; then page_name=ConfigTooltip.qml; fi
    if [[ "$graphs_config_smoke" == true ]]; then page_name=ConfigGraphs.qml; fi
    python3 - "$test_root/package/contents/config/config.qml" "$page_name" <<'PY'
from pathlib import Path
import sys

categories = Path(sys.argv[1])
categories.write_text(
    categories.read_text(encoding="utf-8").replace(
        'source: "config/ConfigAppearance.qml"',
        f'source: "config/{sys.argv[2]}"',
        1,
    ),
    encoding="utf-8",
)
PY
    append_qml "$test_root/package/contents/ui/main.qml" '
    Timer {
        interval: 300
        running: true
        onTriggered: {
            var action = Plasmoid.internalAction("configure")
            if (action) action.trigger()
        }
    }
'
    if [[ "$graphs_config_smoke" == true ]]; then
        edit='
                root.moveChart(4, -1)
                root.moveChart(3, -1)
                root.moveChart(2, -1)
                root.moveChart(1, -1)
                root.removeChart(2)
                historyField.text = "120"
'
    elif [[ "$tooltip_config_smoke" == true ]]; then
        edit='
                root.moveSection(1, -1)
                root.setSectionEnabled(4, false)
                root.addItem(1, "swap_usage")
'
    else
        edit='
                pollField.text = "2.5"
'
    fi
    append_qml "$test_root/package/contents/ui/config/$page_name" '
    Timer {
        id: testSaveTimer
        interval: 100
        running: true
        repeat: true
        onTriggered: {
            if (root.loaded && !root.busy) {
                testSaveTimer.stop()
'"$edit"'
                root.save()
            }
        }
    }
'
fi

if [[ "$smoke" == true && "$settings_smoke" == false ]]; then
    # A probe reports what each widget displays. The second copy, with its own plugin id, is the presented widget; it also pages and sends a fixed geometry, so every method signature crosses the bus from Qt.
    probe='
    property int panelUpdates: 0
    onOutputTextChanged: panelUpdates += 1
    Timer {
        interval: 200
        running: true
        repeat: true
        onTriggered: console.warn("[plasma-top-verify]", Plasmoid.metaData.pluginId, "presented=" + widget.presented, "lifetime=" + widget.serviceLifetime, "panelUpdates=" + widget.panelUpdates, "tooltip=" + widget.tooltipText.length)
    }
'
    # plasmawindowed shows every widget planar and expanded, which always presents; like a panel widget, presentation here follows hover alone.
    python3 - "$test_root/package/contents/ui/main.qml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
presented = "readonly property bool presented: tooltipHovered || expanded || planar"
assert presented in text
path.write_text(text.replace(presented, "readonly property bool presented: tooltipHovered"), encoding="utf-8")
PY
    cp -a "$test_root/package" "$test_root/package-presented"
    append_qml "$test_root/package/contents/ui/main.qml" "$probe"
    append_qml "$test_root/package-presented/contents/ui/main.qml" "$probe"'
    Timer {
        interval: 500
        running: true
        onTriggered: {
            widget.tooltipHovered = true
            widget.wheelStep(-120)
            if (!widget.geometry) {
                widget.geometry = [300, 8.5, false, 8.5]
                widget.sendGeometry()
            }
        }
    }
'
    python3 - "$test_root/package-presented/metadata.json" "$second_applet_id" <<'PY'
import json
from pathlib import Path
import sys

path = Path(sys.argv[1])
metadata = json.loads(path.read_text(encoding="utf-8"))
metadata["KPlugin"]["Id"] = sys.argv[2]
path.write_text(json.dumps(metadata, indent=1), encoding="utf-8")
PY
    kpackagetool6 --type Plasma/Applet --install "$test_root/package-presented" >/dev/null
fi

kpackagetool6 --type Plasma/Applet --install "$test_root/package" >/dev/null

fail() {
    echo "$1" >&2
    for log in "$test_root/qml.log" "$test_root/daemon.log" "$test_root/calls.log"; do
        [[ -f "$log" ]] || continue
        printf '\n== %s ==\n' "$(basename "$log")" >&2
        tail -n 40 "$log" >&2
    done
    exit 1
}

name_owner() {
    dbus-send --session --print-reply=literal --dest=org.freedesktop.DBus / org.freedesktop.DBus.GetNameOwner "string:$bus_name" 2>/dev/null | tr -d ' '
}

start_daemon() {
    "$binary" daemon >>"$test_root/daemon.log" 2>&1 &
    daemon_pid=$!
    for _ in $(seq 1 100); do
        if [[ -n "$(name_owner)" ]] &&
            dbus-send --session --print-reply=literal --dest="$bus_name" "$object_path" org.freedesktop.DBus.Properties.Get "string:$interface" string:PanelHtml 2>/dev/null | grep -q .; then
            return 0
        fi
        kill -0 "$daemon_pid" 2>/dev/null || break
        sleep 0.05
    done
    echo "daemon did not serve $bus_name" >&2
    cat "$test_root/daemon.log" >&2
    return 1
}

start_daemon

plasmawindowed "$applet_id" >"$test_root/qml.log" 2>&1 &
qml_pids+=($!)
if [[ "$smoke" == true && "$settings_smoke" == false ]]; then
    sleep 1
    # plasmawindowed is a unique application: the second launch hands its widget to the first process and exits, so both widgets share one bus connection, as in plasmashell.
    timeout 20 plasmawindowed "$second_applet_id" >>"$test_root/qml.log" 2>&1 ||
        fail "second plasmawindowed launch did not hand over its widget"
fi

if [[ "$smoke" != true ]]; then
    cat <<EOF
Isolated QML verification running.
  test root: $test_root
  bus:       $DBUS_SESSION_BUS_ADDRESS
  daemon:    $daemon_pid
  applet:    ${qml_pids[0]}

Application-form inspection only; this host does not exercise compact panel behavior.
Close plasmawindowed to clean up. Verify horizontal/vertical geometry, hover, pinning, and wheel behavior only in an explicitly approved real Plasma session.
The live session bus and production daemon are not touched.
EOF
    wait "${qml_pids[0]}"
    qml_pids=()
    exit 0
fi

check_alive() {
    local pid
    for pid in "${qml_pids[@]}"; do
        kill -0 "$pid" 2>/dev/null || fail "plasmawindowed exited during smoke test"
    done
    if grep -Eiq '(^| )(error|fatal):|failed to load|is not installed|ReferenceError|TypeError|\[plasma-top\] .* failed:' "$test_root/qml.log"; then
        fail "QML errors detected"
    fi
}

# Latest probe value of one field for one widget.
probe_value() {
    { grep -F "[plasma-top-verify] $1 " "$test_root/$2" || true; } | tail -n 1 | { grep -Eo "$3=[^ \"]*" || true; } | cut -d= -f2
}

# Waits until a widget's probe field satisfies a numeric test(1) comparison such as -ge.
wait_probe() {
    local value
    for _ in $(seq 1 100); do
        value="$(probe_value "$1" "$2" "$3")"
        if [[ "$value" =~ ^[0-9]+$ ]] && test "$value" "$4" "$5"; then
            return 0
        fi
        sleep 0.1
    done
    fail "$1: expected $3 $4 $5, last value '${value:-none}'"
}

# Counts monitored calls of one member whose following argument line matches a pattern.
calls_of() {
    awk -v member="member=$1" -v pattern="$2" '
        index($0, member) { pending = 1; next }
        pending && $0 ~ pattern { count++ }
        pending && /^method call/ { pending = 0 }
        END { print count + 0 }
    ' "$test_root/calls.log"
}

if [[ "$settings_smoke" == true ]]; then
    sleep 3
    check_alive
    if [[ "$config_smoke" == true ]]; then
        grep -Eq $'\tplasma-top\tconfig show$' "$PLASMA_TOP_QML_TRACE" || fail "Daemon settings page did not request user config"
        [[ -s "$XDG_CONFIG_HOME/plasma-top/config.toml" ]] || fail "Daemon settings page did not initialize user config"
        grep -Eq $'\tplasma-top\tconfig apply 2\.5 ' "$PLASMA_TOP_QML_TRACE" || fail "Daemon settings page did not submit edited values"
        grep -Eq '^poll_interval[[:space:]]*=[[:space:]]*2\.5' "$XDG_CONFIG_HOME/plasma-top/config.toml" || fail "Daemon settings page did not save the edited interval"
        echo "QML config smoke passed: user config initialized and edited through the Daemon page"
    fi
    if [[ "$tooltip_config_smoke" == true ]]; then
        grep -Eq $'\tplasma-top\tconfig tooltip show$' "$PLASMA_TOP_QML_TRACE" || fail "Main tooltip page did not request user config"
        grep -Eq $'\tplasma-top\tconfig tooltip apply ' "$PLASMA_TOP_QML_TRACE" || fail "Main tooltip page did not submit edited layout"
        python3 - "$binary" <<'PY'
import json
import subprocess
import sys

layout = json.loads(subprocess.check_output([sys.argv[1], "config", "tooltip", "show"], text=True))
sections = layout["sections"]
assert [section["key"] for section in sections[:2]] == ["drives", "cpumem"], sections
assert not next(section for section in sections if section["key"] == "batteries")["enabled"]
assert "swap_usage" in sections[1]["items"], sections[1]["items"]
PY
        echo "QML tooltip config smoke passed: section order and readings edited through the Main tooltip page"
    fi
    if [[ "$graphs_config_smoke" == true ]]; then
        grep -Eq $'\tplasma-top\tconfig graphs show$' "$PLASMA_TOP_QML_TRACE" || fail "Graphs page did not request user config"
        grep -Eq $'\tplasma-top\tconfig graphs apply ' "$PLASMA_TOP_QML_TRACE" || fail "Graphs page did not submit edited layout"
        python3 - "$binary" <<'PY'
import json
import subprocess
import sys

settings = json.loads(subprocess.check_output([sys.argv[1], "config", "graphs", "show"], text=True))
assert settings["order"] == ["network", "cpu", "gpu", "npu", "disk", "temperature"], settings
assert settings["history_length"] == 120, settings
PY
        echo "QML Graphs config smoke passed: chart visibility, order, and history length edited through the Graphs page"
    fi
    exit 0
fi

hidden=qml.log
shown=qml.log

# Initial hydration, then a pushed panel value, on both widgets; only the presented one receives tooltip text.
wait_probe "$applet_id" "$hidden" panelUpdates -ge 2
wait_probe "$second_applet_id" "$shown" panelUpdates -ge 2
wait_probe "$second_applet_id" "$shown" tooltip -gt 0
check_alive
[[ "$(probe_value "$applet_id" "$hidden" presented)" == false ]] || fail "the first widget must stay hidden"
[[ "$(probe_value "$applet_id" "$hidden" tooltip)" == 0 ]] || fail "the hidden widget laid out tooltip text"

# Every signature as Qt encodes it: StepPage(i), Present(t), SetGeometry(ddbd).
(($(calls_of StepPage '^ +int32 1$') >= 1)) || fail "no StepPage(int32 1) reached the daemon"
(($(calls_of Present '^ +uint64 [1-9][0-9]*$') == 1)) || fail "expected exactly one Present before the restart"
(($(calls_of SetGeometry '^ +double 300$') >= 1)) || fail "no SetGeometry(ddbd) reached the daemon"
grep -A4 'member=SetGeometry' "$test_root/calls.log" | grep -Eq '^ +boolean false$' || fail "SetGeometry lacks its boolean argument"

# A daemon restart while presented: the presented widget presents again and keeps showing text throughout.
stop "$daemon_pid"
daemon_pid=""
sleep 0.5
check_alive
[[ "$(probe_value "$second_applet_id" "$shown" tooltip)" != 0 ]] || fail "daemon absence cleared the tooltip"
start_daemon
wait_probe "$second_applet_id" "$shown" lifetime -ge 2
for _ in $(seq 1 50); do
    (($(calls_of Present '^ +uint64 [1-9][0-9]*$') >= 2)) && break
    sleep 0.1
done
(($(calls_of Present '^ +uint64 [1-9][0-9]*$') == 2)) || fail "the presented widget did not present exactly once to the restarted daemon"
updates="$(probe_value "$applet_id" "$hidden" panelUpdates)"
wait_probe "$applet_id" "$hidden" panelUpdates -gt "$updates"
check_alive
[[ "$(probe_value "$applet_id" "$hidden" tooltip)" == 0 ]] || fail "the hidden widget laid out tooltip text after the restart"

echo "QML smoke passed: hydration, pushed values, all method signatures, one presented widget of two, and daemon restart recovery"
