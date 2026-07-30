#!/usr/bin/env bash
# Remove shipped files while preserving user configuration.
set -uo pipefail

usage() {
    cat <<'EOF'
Usage: ./uninstall.sh [--system] [--dry-run|--dry]

  By default, uninstalls user-local files.
  --system  Uninstall system-wide files.
  --dry-run, --dry  Print resolved paths and planned commands without changing files.
EOF
}

print_command() {
    printf '  '
    printf '%q ' "$@"
    printf '\n'
}

canonical_path() {
    realpath -m -- "$1"
}

MODE=user
DRY_RUN=false
system_set=false
dry_run_set=false
for argument in "$@"; do
    case "$argument" in
    --system)
        [[ "$system_set" == false ]] || {
            echo "[error] duplicate argument: $argument" >&2
            exit 2
        }
        MODE=system
        system_set=true
        ;;
    --dry-run | --dry)
        [[ "$dry_run_set" == false ]] || {
            echo "[error] duplicate dry-run argument: $argument" >&2
            exit 2
        }
        DRY_RUN=true
        dry_run_set=true
        ;;
    -h | --help)
        usage
        exit 0
        ;;
    *)
        echo "[error] unknown argument: $argument" >&2
        usage >&2
        exit 2
        ;;
    esac
done
if [[ "$MODE" == user && -n "${DESTDIR:-}" ]]; then
    echo "[error] DESTDIR requires --system" >&2
    exit 2
fi

APPLET_ID="com.github.bogdan-d.plasma-top"

prepare_runtime_cache() {
    local runtime_base runtime_dir cache_base cache_dir
    if [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
        [[ "$XDG_RUNTIME_DIR" == /* ]] || {
            echo "[error] refusing unsafe XDG_RUNTIME_DIR: ${XDG_RUNTIME_DIR:-<empty>}" >&2
            return 1
        }
        runtime_base="$(canonical_path "$XDG_RUNTIME_DIR")"
        [[ "$runtime_base" != / ]] || {
            echo "[error] XDG_RUNTIME_DIR resolves to /" >&2
            return 1
        }
        runtime_dir="$runtime_base/plasma-top"
    else
        runtime_dir="/tmp/plasma-top-$(id -u)"
    fi
    if [[ -n "${XDG_CACHE_HOME:-}" ]]; then
        cache_base="$XDG_CACHE_HOME"
    else
        [[ "${HOME:-}" == /* && "$HOME" != / ]] || {
            echo "[error] refusing unsafe HOME for cache cleanup: ${HOME:-<empty>}" >&2
            return 1
        }
        cache_base="$HOME/.cache"
    fi
    [[ "$cache_base" == /* ]] || {
        echo "[error] refusing unsafe cache root: $cache_base" >&2
        return 1
    }
    cache_base="$(canonical_path "$cache_base")"
    [[ "$cache_base" != / ]] || {
        echo "[error] cache root resolves to /" >&2
        return 1
    }
    cache_dir="$cache_base/plasma-top"
    RUNTIME_REMOVE="$runtime_dir"
    CACHE_REMOVE="$cache_dir"
}

remove_runtime_cache() {
    rm -rf -- "$RUNTIME_REMOVE" "$CACHE_REMOVE" 2>/dev/null || true
}

print_user_dry_run() {
    printf 'Dry run only; no system changes will be made.\n'
    printf 'Mode: user\nInstall tree: %s\nOwnership marker: %s\n' \
        "$LIBDIR" "$LIBDIR/.plasma-top-install"
    printf 'Launcher: %s\nSystemd unit: %s\nIcon: %s\nLicenses: %s\n' \
        "$HOME/.local/bin/plasma-top" "$DATA_HOME/systemd/user/plasma-top.service" \
        "$DATA_HOME/icons/hicolor/scalable/apps/plasma-top.svg" "$DATA_HOME/licenses/plasma-top"
    printf 'Preserved config: %s\n' "${XDG_CONFIG_HOME:-$HOME/.config}/plasma-top"
    if [[ ! -f "$LIBDIR/.plasma-top-install" ]]; then
        printf '\nNo uninstall commands would run: owned-install marker not found.\n'
        return
    fi
    printf 'Runtime data: %s\nCache: %s\n' "$RUNTIME_REMOVE" "$CACHE_REMOVE"
    printf '\n1. Stop service and remove applet\n'
    print_command systemctl --user disable --now plasma-top
    printf '  If kpackagetool6 is available:\n'
    print_command kpackagetool6 --type Plasma/Applet --remove "$APPLET_ID"
    printf '\n2. Remove user-local files\n'
    print_command rm -f -- "$HOME/.local/bin/plasma-top"
    print_command rm -rf -- "$LIBDIR"
    print_command rm -f -- "$DATA_HOME/systemd/user/plasma-top.service"
    print_command rm -f -- "$DATA_HOME/icons/hicolor/scalable/apps/plasma-top.svg"
    print_command rm -rf -- "$DATA_HOME/licenses/plasma-top"
    printf '\n3. Reload service manager and remove runtime/cache data\n'
    print_command systemctl --user daemon-reload
    print_command rm -rf -- "$RUNTIME_REMOVE" "$CACHE_REMOVE"
}

print_system_dry_run() {
    printf 'Dry run only; no system changes will be made.\n'
    printf 'Mode: system\nInstall tree: %s\nLauncher: %s\nSystemd unit: %s\n' \
        "$ROOT/usr/lib/plasma-top" "$ROOT/usr/bin/plasma-top" \
        "$ROOT/usr/lib/systemd/user/plasma-top.service"
    printf 'Icon: %s\nLicenses: %s\nApplet: %s\n' \
        "$ROOT/usr/share/icons/hicolor/scalable/apps/plasma-top.svg" \
        "$ROOT/usr/share/licenses/plasma-top" \
        "$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
    if [[ -z "$ROOT" ]]; then
        printf 'Runtime data: %s\nCache: %s\nPreserved config: %s\n' \
            "$RUNTIME_REMOVE" "$CACHE_REMOVE" "${XDG_CONFIG_HOME:-${HOME:-~}/.config}/plasma-top"
        printf '\n1. Stop user service\n'
        print_command systemctl --user disable --now plasma-top
    fi
    printf '\n2. Remove installed files\n'
    print_command ${SUDO:+$SUDO} rm -f -- "$ROOT/usr/lib/systemd/user/plasma-top.service"
    print_command ${SUDO:+$SUDO} rm -f -- "$ROOT/usr/bin/plasma-top"
    print_command ${SUDO:+$SUDO} rm -rf -- "$ROOT/usr/lib/plasma-top"
    print_command ${SUDO:+$SUDO} rm -f -- "$ROOT/usr/share/icons/hicolor/scalable/apps/plasma-top.svg"
    print_command ${SUDO:+$SUDO} rm -rf -- "$ROOT/usr/share/licenses/plasma-top"
    if [[ -n "$ROOT" ]]; then
        print_command rm -rf -- "$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
        printf '\nDESTDIR staging only; no service or live Plasma commands would run.\n'
        return
    fi
    printf '\n3. Remove global applet, reload service manager, and clear runtime/cache data\n'
    printf '  If kpackagetool6 is available:\n'
    print_command ${SUDO:+$SUDO} kpackagetool6 --type Plasma/Applet --global --remove "$APPLET_ID"
    print_command systemctl --user daemon-reload
    print_command rm -rf -- "$RUNTIME_REMOVE" "$CACHE_REMOVE"
}

if [[ "$MODE" == user ]]; then
    [[ "${HOME:-}" == /* && "$HOME" != / ]] || {
        echo "[error] user uninstall requires an absolute, non-root HOME" >&2
        exit 2
    }
    HOME="$(canonical_path "$HOME")"
    [[ "$HOME" != / ]] || {
        echo "[error] HOME resolves to /" >&2
        exit 2
    }
    DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
    [[ "$DATA_HOME" == /* ]] || {
        echo "[error] XDG_DATA_HOME must be an absolute directory below a data root" >&2
        exit 2
    }
    DATA_HOME="$(canonical_path "$DATA_HOME")"
    [[ "$DATA_HOME" != / && "$DATA_HOME" != "$HOME" &&
        "$DATA_HOME" != /usr && "$DATA_HOME" != /usr/* ]] || {
        echo "[error] refusing unsafe XDG_DATA_HOME: $DATA_HOME" >&2
        exit 2
    }
    LIBDIR="$DATA_HOME/plasma-top"
    if [[ -L "$LIBDIR" ]]; then
        echo "[error] refusing symlinked install root: $LIBDIR" >&2
        exit 1
    fi
    if [[ ! -f "$LIBDIR/.plasma-top-install" ]]; then
        if [[ "$DRY_RUN" == true ]]; then
            print_user_dry_run
        else
            echo "No owned user-local PlasmaTop install found under $DATA_HOME."
        fi
        exit 0
    fi
    prepare_runtime_cache || exit 2
    if [[ "$DRY_RUN" == true ]]; then
        print_user_dry_run
        exit 0
    fi

    systemctl --user disable --now plasma-top 2>/dev/null || true
    if command -v kpackagetool6 >/dev/null; then
        kpackagetool6 --type Plasma/Applet --remove "$APPLET_ID" 2>/dev/null || true
    fi
    rm -f -- "$HOME/.local/bin/plasma-top"
    rm -rf -- "$LIBDIR"
    rm -f -- "$DATA_HOME/systemd/user/plasma-top.service"
    rm -f -- "$DATA_HOME/icons/hicolor/scalable/apps/plasma-top.svg"
    rm -rf -- "$DATA_HOME/licenses/plasma-top"
    systemctl --user daemon-reload 2>/dev/null || true
    remove_runtime_cache
    echo "PlasmaTop user-local install removed."
    echo "Your config in ${XDG_CONFIG_HOME:-$HOME/.config}/plasma-top was kept."
    exit 0
fi

ROOT="${DESTDIR:-}"
if [[ -n "$ROOT" ]]; then
    [[ "$ROOT" == /* && "$ROOT" != / ]] || {
        echo "[error] DESTDIR must be an absolute directory other than /" >&2
        exit 2
    }
    ROOT="$(canonical_path "$ROOT")"
    [[ "$ROOT" != / ]] || {
        echo "[error] DESTDIR resolves to /" >&2
        exit 2
    }
fi
SUDO=""
if [[ -z "$ROOT" && "$(id -u)" -ne 0 ]]; then SUDO=sudo; fi
if [[ -z "$ROOT" ]]; then
    prepare_runtime_cache || exit 2
    if [[ "$DRY_RUN" == true ]]; then
        print_system_dry_run
        exit 0
    fi
    systemctl --user disable --now plasma-top 2>/dev/null || true
fi

if [[ "$DRY_RUN" == true ]]; then
    print_system_dry_run
    exit 0
fi

$SUDO rm -f -- "$ROOT/usr/lib/systemd/user/plasma-top.service"
$SUDO rm -f -- "$ROOT/usr/bin/plasma-top"
$SUDO rm -rf -- "$ROOT/usr/lib/plasma-top"
$SUDO rm -f -- "$ROOT/usr/share/icons/hicolor/scalable/apps/plasma-top.svg"
$SUDO rm -rf -- "$ROOT/usr/share/licenses/plasma-top"

if [[ -n "$ROOT" ]]; then
    $SUDO rm -rf -- "$ROOT/usr/share/plasma/plasmoids/$APPLET_ID"
    echo "PlasmaTop removed from $ROOT"
    exit 0
fi

if command -v kpackagetool6 >/dev/null; then
    $SUDO kpackagetool6 --type Plasma/Applet --global --remove "$APPLET_ID" 2>/dev/null || true
fi
systemctl --user daemon-reload 2>/dev/null || true
remove_runtime_cache
echo "PlasmaTop uninstalled. (Remove the widget from your panel if it is still there.)"
echo "Your config in ~/.config/plasma-top was kept; delete it by hand if wanted."
