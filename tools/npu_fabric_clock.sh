#!/usr/bin/env bash
# Hold or release the Strix Halo fabric clock (FCLK) around manual NPU validation runs.
# Overlapping GPU and NPU work can hang the machine or corrupt NPU results while FCLK changes speed, so NPU tests pin it at its top DPM level first.
# Holding costs about 2.3 W at idle; `with` releases it even when the workload fails or is interrupted.
# Privileged use goes through a root-owned copy at INSTALLED, allowed by sudoers for `hold` and `release` only.
set -euo pipefail

INSTALLED=/usr/local/sbin/plasma-top-npu-fabric-clock
HOLD_TIMEOUT_TENTHS=100

usage() {
    cat >&2 <<'EOF'
usage: tools/npu_fabric_clock.sh hold | release | status
       tools/npu_fabric_clock.sh with -- COMMAND [ARG...]

hold, release  pin FCLK at its top level or return it to auto (root only)
status         report the current state (no root needed)
with           hold through the installed sudo-allowed copy, run COMMAND as the
               current user, and release on exit or interruption
EOF
}

# Root always writes the real sysfs; an alternate root is only for unprivileged status checks against fixtures.
sysfs_root() {
    if [[ -n "${PLASMA_TOP_SYSFS:-}" ]]; then
        if [[ "$EUID" -eq 0 ]]; then
            echo "refusing PLASMA_TOP_SYSFS as root" >&2
            return 1
        fi
        echo "$PLASMA_TOP_SYSFS"
    else
        echo /sys
    fi
}

# Prints the single Strix Halo iGPU (1002:1586) bound to amdgpu with FCLK controls.
apu_device() {
    local sys card device
    local -a found=()
    sys="$(sysfs_root)" || return 1
    for card in "$sys"/class/drm/card[0-9]*; do
        device="$card/device"
        [[ -e "$device/pp_dpm_fclk" && -e "$device/power_dpm_force_performance_level" ]] || continue
        [[ "$(cat "$device/vendor" 2>/dev/null)" == 0x1002 && "$(cat "$device/device" 2>/dev/null)" == 0x1586 ]] || continue
        [[ "$(basename "$(readlink -f "$device/driver")")" == amdgpu ]] || continue
        found+=("$(readlink -f "$device")")
    done
    mapfile -t found < <(printf '%s\n' "${found[@]}" | sort -u | grep .)
    if ((${#found[@]} != 1)); then
        echo "expected exactly one Strix Halo amdgpu device with FCLK controls under $sys, found ${#found[@]}" >&2
        return 1
    fi
    echo "${found[0]}"
}

write_control() {
    if ! { echo "$2" >"$1"; } 2>/dev/null; then
        echo "could not write '$2' to $1" >&2
        return 1
    fi
}

# Observed means manual DPM with the time-filtered FCLK from gpu_metrics v3.0 (offset 182) within 2% of the top level.
# The `pp_dpm_fclk` `*` marker is not used: kernel 7.2 marks only an exact match, and sampled FCLK rarely equals a level.
observed_top() {
    local level top_mhz revisions average
    level="$(cat "$1/power_dpm_force_performance_level" 2>/dev/null)" || return 1
    [[ "$level" == manual ]] || return 1
    top_mhz="$(grep . "$1/pp_dpm_fclk" | tail -n 1 | sed -n 's/^[0-9]*: *\([0-9]*\)[Mm][Hh]z.*/\1/p')"
    revisions="$(od -An -tu1 -j2 -N2 "$1/gpu_metrics" 2>/dev/null | xargs)"
    average="$(od -An -tu2 -j182 -N2 "$1/gpu_metrics" 2>/dev/null | xargs)"
    [[ "$top_mhz" =~ ^[0-9]+$ && "$revisions" == "3 0" && "$average" =~ ^[0-9]+$ ]] || return 1
    ((average * 100 >= top_mhz * 98))
}

restore_auto() {
    if write_control "$1/power_dpm_force_performance_level" auto &&
        [[ "$(cat "$1/power_dpm_force_performance_level" 2>/dev/null)" == auto ]]; then
        return 0
    fi
    echo "$1: could not restore auto; run release" >&2
    return 1
}

hold() {
    local device="$1" level tenths=0
    level="$(grep . "$device/pp_dpm_fclk" | tail -n 1 | cut -d: -f1)" || level=""
    if [[ ! "$level" =~ ^[0-9]+$ ]]; then
        echo "$device: could not read the top FCLK level" >&2
        return 1
    fi
    if ! write_control "$device/power_dpm_force_performance_level" manual ||
        ! write_control "$device/pp_dpm_fclk" "$level"; then
        restore_auto "$device" || true
        return 1
    fi
    until observed_top "$device" || ((tenths >= HOLD_TIMEOUT_TENTHS)); do
        sleep 0.1
        tenths=$((tenths + 1))
    done
    if ! observed_top "$device"; then
        echo "$device: sampled FCLK did not reach level $level" >&2
        restore_auto "$device" || true
        return 1
    fi
    echo "$device: held at FCLK level $level"
}

release() {
    restore_auto "$1" || return 1
    echo "$1: released (auto)"
}

status() {
    local level
    if ! level="$(cat "$1/power_dpm_force_performance_level" 2>/dev/null)"; then
        echo "$1: performance level unreadable" >&2
        return 1
    fi
    if observed_top "$1"; then
        echo "$1: held, sampled FCLK at its top level"
    else
        echo "$1: not held, performance level $level"
    fi
}

release_installed() {
    sudo -n "$INSTALLED" release || echo "release failed; run: sudo $INSTALLED release" >&2
}

with_held() {
    if [[ "$EUID" -eq 0 ]]; then
        echo "with runs COMMAND as the current user; do not run it as root" >&2
        return 1
    fi
    sudo -n "$INSTALLED" hold
    trap release_installed EXIT
    trap 'exit 130' INT
    trap 'exit 143' TERM
    "$@"
}

main() {
    local action="${1:-status}" device
    case "$action" in
    hold | release)
        if [[ "$EUID" -ne 0 ]]; then
            echo "$action writes amdgpu sysfs controls; run it through sudo" >&2
            exit 1
        fi
        ;;
    status) ;;
    with)
        shift
        [[ "${1:-}" == -- ]] && shift
        if (($# == 0)); then
            usage
            exit 2
        fi
        with_held "$@"
        exit
        ;;
    *)
        usage
        exit 2
        ;;
    esac
    device="$(apu_device)"
    case "$action" in
    hold) hold "$device" ;;
    release) release "$device" ;;
    status) status "$device" ;;
    esac
}

main "$@"
