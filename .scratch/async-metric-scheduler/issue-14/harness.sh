#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

HARNESS_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
readonly HARNESS_DIR
readonly PROCESS_TRACE_AWK="$HARNESS_DIR/process-trace.awk"
REPO_ROOT=$(CDPATH='' cd -- "$HARNESS_DIR/../../.." && pwd)
readonly REPO_ROOT
readonly DEFAULT_TIMEOUT_CONFIG="$REPO_ROOT/.scratch/async-metric-scheduler/runs/development/fixtures/connections.toml"
readonly BASELINE_COMMIT=7c95731b105d704ba7b78d8cb5deb7a19f9277bf
readonly CANDIDATE_COMMIT=ae53de61ac9e29650cebc7e77da8a6cce2350c4a

# One session contains the harness, daemon, CLI helpers, command groups, and legacy orphaned sleeps.
if [[ ${PLASMA_TOP_ISSUE14_SESSION-} != owned ]]; then
    exec setsid env PLASMA_TOP_ISSUE14_SESSION=owned "$0" "$@"
fi
readonly owned_session=$$

duration=30
repeat=3
sample_interval=0.1
startup_timeout=20
shutdown_timeout=2
timeout_delay=6
timeout_command=ss
timeout_page=connections
timeout_title=CONNECTIONS
features=default
output=""
baseline_root=""
candidate_root="$REPO_ROOT"
baseline_bin=""
candidate_bin=""
baseline_input_bin=""
candidate_input_bin=""
baseline_input_sha256=""
candidate_input_sha256=""
baseline_frozen_sha256=""
candidate_frozen_sha256=""
baseline_assets=""
candidate_assets=""
config="$REPO_ROOT/config/config.toml"
timeout_config="$DEFAULT_TIMEOUT_CONFIG"
implementations=baseline,candidate
modes=normal,cpu0
scenarios=hidden,main,graphs,processes,unavailable-service,timeout
scenarios_explicit=0
resource_only=0
dry_run=0
short=0
owned_root=""
active_pid=""
active_starttime=""
sampler_pid=""
deadline_test=""
cleanup_test=0
busctl_wrapper_test=0
startup_snapshot_test=0

usage() {
    cat <<'EOF'
Usage: harness.sh [options]
  --baseline-root PATH       clean fixed-baseline Git root
  --candidate-root PATH      clean exact-candidate Git root (default: checkout root)
  --baseline-bin PATH        baseline release binary
  --candidate-bin PATH       candidate release binary
  --baseline-assets PATH     baseline PLASMA_TOP_CODE_ROOT (default: baseline root)
  --candidate-assets PATH    candidate PLASMA_TOP_CODE_ROOT (default: candidate root)
  --config PATH              one matched config copied for ordinary scenarios
  --timeout-config PATH      one matched config containing the timeout page
  --features LIST            identical declared Cargo features, or "default"
  --duration SECONDS         exact steady window duration (default: 30)
  --repeat N                 repetitions (default: 3)
  --output PATH              new evidence directory
  --implementations LIST     baseline,candidate subset
  --modes LIST               normal,cpu0 subset
  --scenarios LIST           hidden,main,graphs,processes,unavailable-service,timeout subset
  --resource-only            untraced resource evidence; only hidden,main,graphs,processes
  --sample-interval SECONDS  bounded /proc/publication polling interval (default: 0.1)
  --startup-timeout SECONDS  startup/state verification bound (default: 20)
  --timeout-command NAME     command wrapped only in timeout runs (default: ss)
  --timeout-page NAME        configured timeout page (default: connections)
  --timeout-title TEXT       expected rendered page title (default: CONNECTIONS)
  --timeout-delay SECONDS    wrapper sleep, longer than daemon timeout (default: 6)
  --deadline-add BASE,DELTA  print fixed-precision deadline and exit (test helper)
  --cleanup-self-test        terminate an owned test child and exit
  --busctl-wrapper-self-test verify unavailable-baseline wrapper quoting and exit
  --startup-snapshot-self-test verify launch-zero snapshot arithmetic and exit
  --short                    candidate/normal/main, one 1-second repetition
  --dry-run                  validate and print the matrix without source/binary checks
EOF
}

die() {
    printf 'harness: %s\n' "$*" >&2
    exit 1
}

is_number() {
    [[ $1 =~ ^[0-9]+([.][0-9]+)?$ ]] && awk -v value="$1" 'BEGIN { exit !(value > 0) }'
}

contains_csv() {
    [[ ,$1, == *,$2,* ]]
}

monotonic() {
    awk '{ print $1; exit }' /proc/uptime
}

deadline_add() {
    awk -v base="$1" -v delta="$2" 'BEGIN { printf "%.9f\n", base + delta }'
}

write_launch_snapshot() {
    printf '%s 0 0 0 0 0 0 0\n' "$1" >"$2"
}

snapshot_delta() {
    awk 'NR == FNR { for (i=1;i<=NF;i++) a[i]=$i; next } { printf "%.6f\t%d\t%d\t%d\t%d\t%d\t%d\n",$1-a[1],$2-a[2],$3-a[3],$4-a[4],$5-a[5],$7-a[7],$8-a[8] }' "$1" "$2"
}

proc_fields() {
    sed 's/^[^)]*) //' "/proc/$1/stat" 2>/dev/null
}

proc_starttime() {
    local line rest
    [[ -r /proc/$1/stat ]] || return 1
    IFS= read -r line <"/proc/$1/stat" || return 1
    rest=${line##*) }
    local -a fields
    read -r -a fields <<<"$rest"
    printf '%s\n' "${fields[19]}"
}

proc_session() {
    local line rest
    [[ -r /proc/$1/stat ]] || return 1
    IFS= read -r line <"/proc/$1/stat" || return 1
    rest=${line##*) }
    local -a fields
    read -r -a fields <<<"$rest"
    printf '%s\n' "${fields[3]}"
}

proc_group() {
    local line rest
    [[ -r /proc/$1/stat ]] || return 1
    IFS= read -r line <"/proc/$1/stat" || return 1
    rest=${line##*) }
    local -a fields
    read -r -a fields <<<"$rest"
    printf '%s\n' "${fields[2]}"
}

session_members() {
    local stat pid sid line rest
    declare -A seen=()
    for stat in /proc/[0-9]*/stat; do
        [[ -r $stat ]] || continue
        pid=${stat#/proc/}
        pid=${pid%/stat}
        [[ $pid != "$BASHPID" ]] || continue
        IFS= read -r line <"$stat" || continue
        rest=${line##*) }
        local -a fields
        read -r -a fields <<<"$rest"
        sid=${fields[3]}
        if [[ $sid == "$owned_session" && -z ${seen[$pid]+x} ]]; then
            seen[$pid]=1
            printf '%s\n' "$pid"
        fi
    done
}

record_identity() {
    local pid=$1 target=$2 start sid pgid
    start=$(proc_starttime "$pid") || return 1
    sid=$(proc_session "$pid") || return 1
    pgid=$(proc_group "$pid") || return 1
    [[ $sid == "$owned_session" ]] || return 1
    printf '%s\t%s\t%s\t%s\n' "$pid" "$start" "$sid" "$pgid" >>"$target"
}

signal_identity() {
    local signal=$1 pid=$2 start=$3 sid=$4
    [[ $pid != "$owned_session" && -r /proc/$pid/stat ]] || return 0
    [[ $(proc_starttime "$pid") == "$start" && $(proc_session "$pid") == "$sid" && $sid == "$owned_session" ]] || return 0
    kill "-$signal" "$pid" 2>/dev/null || true
}

terminate_owned_session() {
    local identities=$1 now deadline kill_at signal pid start sid pgid found
    deadline=$(deadline_add "$(monotonic)" "$shutdown_timeout")
    kill_at=$(deadline_add "$(monotonic)" 0.2)
    while :; do
        found=0
        now=$(monotonic)
        signal=TERM
        awk -v now="$now" -v end="$kill_at" 'BEGIN { exit !(now >= end) }' && signal=KILL
        while read -r pid; do
            [[ $pid != "$owned_session" ]] || continue
            found=1
            start=$(proc_starttime "$pid") || continue
            sid=$(proc_session "$pid") || continue
            pgid=$(proc_group "$pid") || continue
            [[ $sid == "$owned_session" ]] || continue
            printf '%s\t%s\t%s\t%s\n' "$pid" "$start" "$sid" "$pgid" >>"$identities"
            if [[ $pgid != "$owned_session" && $(proc_starttime "$pid" 2>/dev/null || true) == "$start" && $(proc_session "$pid" 2>/dev/null || true) == "$owned_session" && $(proc_group "$pid" 2>/dev/null || true) == "$pgid" ]]; then
                kill "-$signal" -- "-$pgid" 2>/dev/null || true
            fi
            signal_identity "$signal" "$pid" "$start" "$sid"
        done < <(session_members)
        ((found)) || return 0
        awk -v now="$now" -v end="$deadline" 'BEGIN { exit !(now >= end) }' && return 1
        sleep 0.05
    done
}

cleanup() {
    local status=$? identities="" cleanup_safe=1
    trap - EXIT INT TERM
    [[ -z $owned_root ]] || identities="$owned_root/cleanup-identities.tsv"
    if [[ -n $identities ]]; then
        : >"$identities"
        terminate_owned_session "$identities" || cleanup_safe=0
    fi
    if [[ -n $sampler_pid ]]; then
        wait "$sampler_pid" 2>/dev/null || true
    fi
    if ((status != 0)) && [[ -n $owned_root && -d $owned_root && -n $output && -d $output ]]; then
        cp -a -- "$owned_root" "$output/failed-owned-root"
    fi
    if ((cleanup_safe)); then
        [[ -z $owned_root || ! -d $owned_root ]] || rm -rf -- "$owned_root"
    else
        printf 'harness: owned session members remain; preserving %s\n' "$owned_root" >&2
        status=1
    fi
    exit "$status"
}
trap cleanup EXIT INT TERM

while (($#)); do
    case $1 in
    --baseline-root)
        baseline_root=${2:?}
        shift 2
        ;;
    --candidate-root)
        candidate_root=${2:?}
        shift 2
        ;;
    --baseline-bin)
        baseline_bin=${2:?}
        shift 2
        ;;
    --candidate-bin)
        candidate_bin=${2:?}
        shift 2
        ;;
    --baseline-assets)
        baseline_assets=${2:?}
        shift 2
        ;;
    --candidate-assets)
        candidate_assets=${2:?}
        shift 2
        ;;
    --config)
        config=${2:?}
        shift 2
        ;;
    --timeout-config)
        timeout_config=${2:?}
        shift 2
        ;;
    --features)
        features=${2:?}
        shift 2
        ;;
    --duration)
        duration=${2:?}
        shift 2
        ;;
    --repeat)
        repeat=${2:?}
        shift 2
        ;;
    --output)
        output=${2:?}
        shift 2
        ;;
    --implementations)
        implementations=${2:?}
        shift 2
        ;;
    --modes)
        modes=${2:?}
        shift 2
        ;;
    --scenarios)
        scenarios=${2:?}
        scenarios_explicit=1
        shift 2
        ;;
    --resource-only)
        resource_only=1
        shift
        ;;
    --sample-interval)
        sample_interval=${2:?}
        shift 2
        ;;
    --startup-timeout)
        startup_timeout=${2:?}
        shift 2
        ;;
    --timeout-command)
        timeout_command=${2:?}
        shift 2
        ;;
    --timeout-page)
        timeout_page=${2:?}
        shift 2
        ;;
    --timeout-title)
        timeout_title=${2:?}
        shift 2
        ;;
    --timeout-delay)
        timeout_delay=${2:?}
        shift 2
        ;;
    --deadline-add)
        deadline_test=${2:?}
        shift 2
        ;;
    --cleanup-self-test)
        cleanup_test=1
        shift
        ;;
    --busctl-wrapper-self-test)
        busctl_wrapper_test=1
        shift
        ;;
    --startup-snapshot-self-test)
        startup_snapshot_test=1
        shift
        ;;
    --short)
        short=1
        shift
        ;;
    --dry-run)
        dry_run=1
        shift
        ;;
    -h | --help)
        usage
        exit 0
        ;;
    *) die "unknown option: $1" ;;
    esac
done

if ((resource_only && !scenarios_explicit)); then
    scenarios=hidden,main,graphs,processes
fi
if ((resource_only)); then
    for value in ${scenarios//,/ }; do
        [[ $value == hidden || $value == main || $value == graphs || $value == processes ]] || die "--resource-only does not permit scenario: $value"
    done
fi
if ((short)); then
    duration=1 repeat=1 implementations=candidate modes=normal scenarios=main
fi
is_number "$duration" || die "duration must be positive"
is_number "$sample_interval" || die "sample interval must be positive"
is_number "$startup_timeout" || die "startup timeout must be positive"
is_number "$timeout_delay" || die "timeout delay must be positive"
awk -v delay="$timeout_delay" 'BEGIN { exit !(delay >= 6) }' || die "timeout delay must be at least 6 seconds"
[[ $repeat =~ ^[1-9][0-9]*$ ]] || die "repeat must be a positive integer"
[[ $timeout_command =~ ^[A-Za-z0-9._+-]+$ ]] || die "timeout command must be a bare executable name"
[[ $timeout_page =~ ^[A-Za-z0-9_-]+$ ]] || die "timeout page must be a page id"
for value in ${implementations//,/ }; do [[ $value == baseline || $value == candidate ]] || die "unknown implementation: $value"; done
for value in ${modes//,/ }; do [[ $value == normal || $value == cpu0 ]] || die "unknown mode: $value"; done
for value in ${scenarios//,/ }; do
    case $value in hidden | main | graphs | processes | unavailable-service | timeout) ;; *) die "unknown scenario: $value" ;; esac
done
command -v setsid >/dev/null || die "setsid is required"

if [[ -n $deadline_test ]]; then
    [[ $deadline_test == *,* ]] || die "deadline test requires BASE,DELTA"
    deadline_add "${deadline_test%%,*}" "${deadline_test#*,}"
    exit 0
fi
if ((startup_snapshot_test)); then
    snapshot_test=$(mktemp "${TMPDIR:-/tmp}/plasma-top-issue14-snapshot.XXXXXXXX")
    write_launch_snapshot 123.5 "$snapshot_test"
    snapshot_delta_result=$(snapshot_delta "$snapshot_test" <(printf '123.625 2 3 5 7 99 11 13\n'))
    rm -f -- "$snapshot_test"
    [[ $snapshot_delta_result == $'0.125000\t2\t3\t5\t7\t11\t13' ]] || die "startup snapshot self-test failed: $snapshot_delta_result"
    printf 'startup snapshot self-test passed\n'
    exit 0
fi
if ((cleanup_test)); then
    identities=$(mktemp "${TMPDIR:-/tmp}/plasma-top-issue14-cleanup.XXXXXXXX")
    sleep 30 &
    child=$!
    terminate_owned_session "$identities" || die "cleanup self-test left owned members"
    wait "$child" 2>/dev/null || true
    rm -f -- "$identities"
    printf 'cleanup self-test passed\n'
    exit 0
fi

write_busctl_wrapper() {
    local target=$1
    cat >"$target" <<'EOF'
#!/usr/bin/env bash
set -u
: "${PLASMA_TOP_BUSCTL_LOG:?}"
read -r timestamp _ </proc/uptime
{
    printf 'timestamp_monotonic=%q attempt_pid=%q argv=' "$timestamp" "$$"
    printf ' %q' "$0" "$@"
    printf '\n'
} >>"$PLASMA_TOP_BUSCTL_LOG"
exit 69
EOF
    chmod +x "$target"
}

if ((busctl_wrapper_test)); then
    wrapper_test_root=$(mktemp -d "${TMPDIR:-/tmp}/plasma-top issue14 busctl.XXXXXXXX")
    wrapper_test="$wrapper_test_root/wrapper bin/busctl"
    wrapper_test_log="$wrapper_test_root/attempt log"
    mkdir -p -- "$(dirname -- "$wrapper_test")"
    write_busctl_wrapper "$wrapper_test"
    set +e
    PLASMA_TOP_BUSCTL_LOG="$wrapper_test_log" "$wrapper_test" --system 'argument with spaces' "quote'and\"tab"$'\tend'
    wrapper_status=$?
    set -e
    ((wrapper_status == 69)) || die "busctl wrapper self-test exit was $wrapper_status"
    IFS= read -r wrapper_line <"$wrapper_test_log"
    [[ $wrapper_line =~ ^timestamp_monotonic=[0-9]+\.[0-9]+\ attempt_pid=[0-9]+\ argv= ]] || die "busctl wrapper self-test missing timestamp or attempt"
    expected_argv=$(printf ' %q' "$wrapper_test" --system 'argument with spaces' "quote'and\"tab"$'\tend')
    [[ ${wrapper_line#* argv=} == "$expected_argv" ]] || die "busctl wrapper self-test did not preserve exact argv"
    rm -rf -- "$wrapper_test_root"
    printf 'busctl wrapper self-test passed\n'
    exit 0
fi

print_matrix() {
    local implementation mode scenario iteration
    for implementation in ${implementations//,/ }; do
        for mode in ${modes//,/ }; do
            for scenario in ${scenarios//,/ }; do
                for ((iteration = 1; iteration <= repeat; iteration++)); do
                    printf '%s\t%s\t%s\t%d\t%s\n' "$implementation" "$mode" "$scenario" "$iteration" "$duration"
                done
            done
        done
    done
}
if ((dry_run)); then
    printf 'implementation\tmode\tscenario\trepetition\tsteady_seconds\n'
    print_matrix
    exit 0
fi

[[ -f $config ]] || die "config does not exist: $config"
if contains_csv "$scenarios" timeout; then
    [[ -f $timeout_config ]] || die "timeout config does not exist: $timeout_config"
    awk -v page="$timeout_page" '/^\[pages\]$/ { p=1; next } p && /^\[/ { exit } p && /^[[:space:]]*order[[:space:]]*=/ { line=$0; gsub(/[[:space:]]/, "", line); ok=index(line, "order=[\"" page "\"")==1; exit !ok } END { if (!ok) exit 1 }' "$timeout_config" || die "timeout config must put '$timeout_page' first"
fi
if contains_csv "$modes" cpu0; then command -v taskset >/dev/null || die "taskset is required"; fi
((resource_only)) || command -v strace >/dev/null || die "strace is required for matched process tracing"
[[ -x /usr/bin/time ]] || die "/usr/bin/time is required"

verify_source() {
    local label=$1 root=$2 expected=$3
    [[ -d $root/.git || -f $root/.git ]] || die "$label root must retain Git metadata for exact provenance"
    [[ $(git -C "$root" rev-parse HEAD) == "$expected" ]] || die "$label root is not exact commit $expected"
    # This scoped harness is evidence, not a source/build input; all other candidate paths must be clean.
    [[ -z $(git -C "$root" status --porcelain --untracked-files=all -- . ':(exclude).scratch/async-metric-scheduler/issue-14') ]] || die "$label source root is not clean"
}
if contains_csv "$implementations" baseline; then
    [[ -n $baseline_root ]] || die "--baseline-root is required"
    baseline_bin=${baseline_bin:-$baseline_root/target/release/plasma-top}
    baseline_assets=${baseline_assets:-$baseline_root}
    verify_source baseline "$baseline_root" "$BASELINE_COMMIT"
    [[ -x $baseline_bin ]] || die "baseline binary is not executable"
fi
if contains_csv "$implementations" candidate; then
    candidate_bin=${candidate_bin:-$candidate_root/target/release/plasma-top}
    candidate_assets=${candidate_assets:-$candidate_root}
    verify_source candidate "$candidate_root" "$CANDIDATE_COMMIT"
    [[ -x $candidate_bin ]] || die "candidate binary is not executable"
fi

output=${output:-$HARNESS_DIR/evidence-$(date -u +%Y%m%dT%H%M%SZ)}
[[ ! -e $output ]] || die "output already exists: $output"
mkdir -p -- "$output/raw" "$output/binaries"
output=$(CDPATH='' cd -- "$output" && pwd)
owned_root=$(mktemp -d "${TMPDIR:-/tmp}/plasma-top-issue14.XXXXXXXX")

freeze_binary() {
    local label=$1 source=$2 before after frozen
    local target="$output/binaries/$label-plasma-top"
    before=$(sha256sum "$source" | awk '{print $1}')
    cp -- "$source" "$target"
    chmod 0555 "$target"
    after=$(sha256sum "$source" | awk '{print $1}')
    frozen=$(sha256sum "$target" | awk '{print $1}')
    [[ $before == "$after" ]] || die "$label input binary changed while it was frozen"
    [[ $before == "$frozen" ]] || die "$label frozen binary does not match its input"
    if [[ $label == baseline ]]; then
        baseline_input_bin=$source baseline_input_sha256=$before baseline_frozen_sha256=$frozen baseline_bin=$target
    else
        candidate_input_bin=$source candidate_input_sha256=$before candidate_frozen_sha256=$frozen candidate_bin=$target
    fi
}
contains_csv "$implementations" baseline && freeze_binary baseline "$baseline_bin"
contains_csv "$implementations" candidate && freeze_binary candidate "$candidate_bin"

relative_hashes() {
    local root=$1
    shift
    (cd "$root" && find "$@" -type f -print0 | sort -z | xargs -0 sha256sum)
}
source_manifest() {
    local label=$1 root=$2 assets=$3 input_bin=$4 input_sha=$5 frozen_sha=$6 expected=$7
    printf '%s_commit=%s\n%s_status=clean\n%s_root=%s\n%s_assets=%s\n' "$label" "$expected" "$label" "$label" "$root" "$label" "$assets"
    printf '%s_binary_input_path=%s\n%s_binary_input_sha256=%s\n' "$label" "$input_bin" "$label" "$input_sha"
    printf '%s_binary_frozen_relative_path=binaries/%s-plasma-top\n%s_binary_frozen_sha256=%s\n' "$label" "$label" "$label" "$frozen_sha"
    relative_hashes "$root" Cargo.toml Cargo.lock src >"$output/$label-source-files.sha256"
    relative_hashes "$assets" config style lang >"$output/$label-asset-files.sha256"
    printf '%s_source_manifest_sha256=%s\n' "$label" "$(sha256sum "$output/$label-source-files.sha256" | awk '{print $1}')"
    printf '%s_asset_manifest_sha256=%s\n' "$label" "$(sha256sum "$output/$label-asset-files.sha256" | awk '{print $1}')"
}
{
    printf 'harness_sha256=%s\ncaptured_utc=%s\nsession_id=%s\n' "$(sha256sum "$0" | awk '{print $1}')" "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$owned_session"
    if ((resource_only)); then
        printf 'resource_only=1\nevidence_kind=untraced resource evidence for CPU, RSS, publication, leader-thread /proc context switches, and whole-tree GNU-time context switches\nchild_fault_evidence=authoritative child counts and fault proof live in the normal traced matrix\nprocess_trace_parser=not used\n'
    else
        printf 'resource_only=0\nevidence_kind=matched process-traced child and fault evidence\nprocess_trace_parser_sha256=%s\n' "$(sha256sum "$PROCESS_TRACE_AWK" | awk '{print $1}')"
    fi
    printf 'fixed_baseline_commit=%s\nfixed_candidate_commit=%s\n' "$BASELINE_COMMIT" "$CANDIDATE_COMMIT"
    printf 'build_command=cargo build --release --locked%s\nfeatures=%s\n' "$([[ $features == default ]] || printf ' --features %s' "$features")" "$features"
    printf 'rustc=%s\ncargo=%s\n' "$(rustc --version)" "$(cargo --version)"
    printf 'host=%s\nkernel=%s\narchitecture=%s\n' "$(uname -n)" "$(uname -r)" "$(uname -m)"
    printf 'duration=%s\nrepeat=%s\nimplementations=%s\nmodes=%s\nscenarios=%s\nsample_interval=%s\naffinity=%s\n' "$duration" "$repeat" "$implementations" "$modes" "$scenarios" "$sample_interval" "$(taskset -pc $$ 2>/dev/null || printf unavailable)"
    printf 'config_source=%s\nconfig_source_sha256=%s\ntimeout_config_source=%s\ntimeout_config_source_sha256=%s\n' "$config" "$(sha256sum "$config" | awk '{print $1}')" "$timeout_config" "$(sha256sum "$timeout_config" | awk '{print $1}')"
    printf 'notification_protocol=each source config is copied, then every key in its existing [notifications] table is changed to false; source files are never modified\n'
    printf 'asset_protocol=each implementation uses assets from its exact source commit; relative manifests are retained separately because version-specific assets are not asserted equal\n'
    if ((resource_only)); then
        printf 'process_count_protocol=untraced resource runs do not collect or parse child process evidence; child metrics are NA:untraced_resource_run\n'
        printf 'measurement_perturbation=daemon launches directly under /usr/bin/time without ptrace\n'
    else
        printf 'process_count_protocol=matched strace -f process tracing counts only the daemon-rooted clone/exec/exit tree and excludes the strace/time/env/taskset launch chain\n'
        printf 'measurement_perturbation=strace process tracing affects both implementations; candidate unavailable-service additionally traces connect\n'
    fi
    printf 'unavailable_service_protocol=matched user-visible absence uses implementation-specific safe boundaries: baseline PATH-injects a failing busctl and candidate uses a nonexistent child-only system-bus address\n'
    printf 'startup_protocol=launch-inclusive wall and cumulative CPU begin immediately before spawn; candidate hidden readiness includes implementation-specific present, tooltip-prime, and dismiss actions\n'
    printf 'sampling_protocol=daemon RSS sampling begins immediately after PID discovery and can miss only the pre-discovery interval; GNU time instrumented-tree peak RSS covers the full process lifetime\n'
    printf 'retained_artifacts_protocol=every run retains its transformed config, final panel and tooltip HTML, samples, identities, command, time output, and state report\n'
    printf 'wakeups=unavailable: /proc does not expose scheduler wakeups\nhardware_counters=unavailable: perf not collected\nenergy=unavailable: never inferred\n'
    contains_csv "$implementations" baseline && source_manifest baseline "$baseline_root" "$baseline_assets" "$baseline_input_bin" "$baseline_input_sha256" "$baseline_frozen_sha256" "$BASELINE_COMMIT"
    contains_csv "$implementations" candidate && source_manifest candidate "$candidate_root" "$candidate_assets" "$candidate_input_bin" "$candidate_input_sha256" "$candidate_frozen_sha256" "$CANDIDATE_COMMIT"
} >"$output/manifest.txt"
printf 'implementation\tmode\tscenario\trepetition\twindow\twall_seconds\tuser_ticks\tsystem_ticks\tchild_user_ticks\tchild_system_ticks\trss_peak_kib\tvoluntary_context_switches\tinvoluntary_context_switches\tchild_exec_attempts\tsuccessful_child_execs\tchild_exits_zero\tpublication_writes\tpublication_bytes\tpublication_method\n' >"$output/metrics.tsv"
printf 'implementation\tmode\tscenario\trepetition\tdaemon_exit\tforced_kill\tshutdown_seconds\tstate_check\tprocess_check\tprotocol_note\n' >"$output/results.tsv"
printf 'implementation\tmode\tscenario\trepetition\tcommand\n' >"$output/commands.tsv"

copy_config_without_notifications() {
    local source=$1 target=$2
    awk '
        /^\[notifications\][[:space:]]*$/ { section=1; print; next }
        section && /^\[/ { section=0 }
        section && /^[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*=/ { sub(/=.*/, "= false") }
        { print }
    ' "$source" >"$target"
    awk '/^\[notifications\]$/ { section=1; seen=1; next } section && /^\[/ { section=0 } section && /^[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*=/ && $0 !~ /=[[:space:]]*false([[:space:]]*(#.*)?)?$/ { bad=1 } END { exit !seen || bad }' "$target"
}

write_snapshot() {
    local pid=$1 target=$2 now rest voluntary involuntary
    now=$(monotonic)
    rest=$(proc_fields "$pid")
    voluntary=$(awk '/^voluntary_ctxt_switches:/ { print $2 }' "/proc/$pid/status")
    involuntary=$(awk '/^nonvoluntary_ctxt_switches:/ { print $2 }' "/proc/$pid/status")
    awk -v now="$now" -v voluntary="$voluntary" -v involuntary="$involuntary" '{ print now, $12, $13, $14, $15, $22, voluntary, involuntary }' <<<"$rest" >"$target"
}

task_children() {
    local parent=$1 file children child
    for file in /proc/"$parent"/task/*/children; do
        [[ -r $file ]] || continue
        children=$(cat "$file" 2>/dev/null || true)
        for child in $children; do printf '%s\n' "$child"; done
    done | sort -un
}

sample_run() {
    local pid=$1 runtime=$2 phase_file=$3 log=$4 identities=$5 stop_file=$6
    local panel_sig="" tooltip_sig="" phase now rest rss voluntary involuntary path sig size parent child
    local -a queue
    declare -A seen=()
    while [[ -r /proc/$pid/stat && ! -e $stop_file ]]; do
        phase=$(cat "$phase_file") now=$(monotonic)
        if [[ -r /proc/$pid/status ]]; then
            rest=$(proc_fields "$pid") rss=$(awk '{print $22}' <<<"$rest")
            voluntary=$(awk '/^voluntary_ctxt_switches:/ {print $2}' "/proc/$pid/status")
            involuntary=$(awk '/^nonvoluntary_ctxt_switches:/ {print $2}' "/proc/$pid/status")
            printf 'proc\t%s\t%s\t%s\t%s\t%s\n' "$phase" "$now" "$rss" "$voluntary" "$involuntary" >>"$log"
        fi
        for path in panel.html tooltip.html; do
            [[ -f $runtime/$path ]] || continue
            sig=$(stat -Lc '%Y:%s:%i' "$runtime/$path" 2>/dev/null || true) size=$(stat -Lc '%s' "$runtime/$path" 2>/dev/null || printf 0)
            if [[ $path == panel.html && $sig != "$panel_sig" ]] || [[ $path == tooltip.html && $sig != "$tooltip_sig" ]]; then
                printf 'publication\t%s\t%s\t%s\t%s\n' "$phase" "$now" "$path" "$size" >>"$log"
                [[ $path == panel.html ]] && panel_sig=$sig || tooltip_sig=$sig
            fi
        done
        queue=("$pid")
        while ((${#queue[@]})); do
            parent=${queue[0]} queue=("${queue[@]:1}")
            while read -r child; do
                [[ -n $child ]] || continue
                queue+=("$child")
                if [[ -z ${seen[$child]+x} ]] && record_identity "$child" "$identities"; then
                    seen[$child]=1
                    printf 'child\t%s\t%s\t%s\n' "$phase" "$now" "$child" >>"$log"
                fi
            done < <(task_children "$parent")
        done
        sleep "$sample_interval"
    done
}

wait_until() {
    local deadline=$1
    while awk -v now="$(monotonic)" -v end="$deadline" 'BEGIN {exit !(now<end)}'; do sleep 0.02; done
}

wait_for_file() {
    local path=$1 deadline=$2
    while [[ ! -s $path ]]; do
        [[ -r /proc/$active_pid/stat ]] || return 1
        awk -v now="$(monotonic)" -v end="$deadline" 'BEGIN {exit !(now>=end)}' && return 1
        sleep 0.05
    done
}

wait_for_fresh_text() {
    local path=$1 text=$2 before=$3 deadline=$4 signature
    while :; do
        signature=$(stat -Lc '%Y:%s:%i' "$path" 2>/dev/null || true)
        [[ $signature != "$before" ]] && grep -Fq "$text" "$path" 2>/dev/null && return 0
        [[ -r /proc/$active_pid/stat ]] || return 1
        awk -v now="$(monotonic)" -v end="$deadline" 'BEGIN {exit !(now>=end)}' && return 1
        sleep 0.05
    done
}

run_cli() {
    local bin=$1 assets=$2 run_root=$3 path_value=$4
    shift 4
    env -i LC_ALL=C HOME="$run_root/home" XDG_CONFIG_HOME="$run_root/config-home" XDG_CACHE_HOME="$run_root/cache-home" XDG_DATA_HOME="$run_root/data-home" XDG_RUNTIME_DIR="$run_root/runtime-home" TMPDIR="$run_root/tmp" PLASMA_TOP_CODE_ROOT="$assets" PATH="$path_value" DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS-}" "$bin" "$@"
}

verify_html() {
    [[ -s $1 && -s $2 ]] && grep -Eq 'class="[^\"]*panel|class="panel' "$1" && grep -q 'class="tooltip"' "$2" && ! grep -Eqi '(^|[^[:alpha:]])(nan|inf)([^[:alpha:]]|$)' "$1" "$2"
}

process_counts() {
    local trace=$1 daemon_pid=$2 window_start=$3 window_end=$4
    awk -v daemon_pid="$daemon_pid" -v window_start="$window_start" -v window_end="$window_end" -f "$PROCESS_TRACE_AWK" "$trace" "$trace"
}

window_metrics() {
    local implementation=$1 mode=$2 scenario=$3 iteration=$4 window=$5 start=$6 end=$7 samples=$8 trace=$9 daemon_pid=${10} epoch_start=${11} epoch_end=${12}
    local wall user system child_user child_system voluntary involuntary rss_peak publications bytes attempts exec_successes exits_zero
    IFS=$'\t' read -r wall user system child_user child_system voluntary involuntary < <(snapshot_delta "$start" "$end")
    rss_peak=$(awk -F '\t' -v phase="$window" -v pagesize="$(getconf PAGESIZE)" '$1=="proc"&&$2==phase&&$4>max{max=$4} END{printf "%d",max*pagesize/1024}' "$samples")
    read -r publications bytes < <(awk -F '\t' -v phase="$window" '$1=="publication"&&$2==phase{count++;bytes+=$5} END{print count+0,bytes+0}' "$samples")
    if ((resource_only)); then
        child_user=NA:untraced_resource_run
        child_system=NA:untraced_resource_run
        attempts=NA:untraced_resource_run
        exec_successes=NA:untraced_resource_run
        exits_zero=NA:untraced_resource_run
    else
        IFS=$'\t' read -r attempts exec_successes exits_zero < <(process_counts "$trace" "$daemon_pid" "$epoch_start" "$epoch_end")
    fi
    printf '%s\t%s\t%s\t%d\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\tbounded-stat-poll-%ss\n' "$implementation" "$mode" "$scenario" "$iteration" "$window" "$wall" "$user" "$system" "$child_user" "$child_system" "$rss_peak" "$voluntary" "$involuntary" "$attempts" "$exec_successes" "$exits_zero" "$publications" "$bytes" "$sample_interval" >>"$output/metrics.tsv"
}

verify_state() {
    local runtime=$1 report=$2 path relative bad=0
    : >"$report"
    while IFS= read -r -d '' path; do
        relative=${path#"$runtime"/}
        case $relative in
        tooltip.html | panel.html | state | state/page | state/page.lock | state/npages | state/geom | state/presented | state/presented.lock | state/presented/14001) printf 'intentional\t%s\n' "$relative" >>"$report" ;;
        *.tmp | *.new | .*tmp* | */.*tmp*)
            printf 'atomic-temp\t%s\n' "$relative" >>"$report"
            bad=1
            ;;
        *)
            printf 'outside-allowlist\t%s\n' "$relative" >>"$report"
            bad=1
            ;;
        esac
    done < <(find "$runtime" -mindepth 1 -print0 2>/dev/null)
    return "$bad"
}

run_one() {
    local implementation=$1 mode=$2 scenario=$3 iteration=$4 bin assets protocol_note scenario_config expected_page=0 expected_text=""
    local run_id run_root runtime config_copy path_value deadline daemon_pid="" daemon_start trace_file="" trace_expression launch_command
    local shutdown_start shutdown_end daemon_exit forced_kill=0 setup_sig steady_deadline timeout_elapsed="NA" timeout_detail="NA" process_check state_check
    local startup_monotonic startup_epoch startup_end_epoch setup_end_epoch steady_start_epoch steady_end_epoch
    run_id="$implementation-$mode-$scenario-$iteration" run_root="$owned_root/$run_id"
    mkdir -p "$run_root"/{home,config-home,cache-home,data-home,runtime-home,tmp,wrapper-bin,config}
    runtime="$run_root/runtime-home/plasma-top"
    if [[ $implementation == baseline ]]; then
        bin=$baseline_bin assets=$baseline_assets protocol_note='baseline hidden is user-hidden but legacy daemon has no presentation protocol and continues tooltip work'
    elif [[ $scenario == hidden ]]; then
        bin=$candidate_bin assets=$candidate_assets protocol_note='candidate hidden startup readiness includes implementation-specific present, tooltip-prime, and dismiss actions; setup observes >1s dismissal grace'
    else
        bin=$candidate_bin assets=$candidate_assets protocol_note='candidate presentation lease is created before daemon launch'
    fi
    scenario_config=$config
    case $scenario in graphs) expected_page=1 expected_text=GRAPHS ;; processes) expected_page=2 expected_text='TOP PROCESSES' ;; timeout) scenario_config=$timeout_config expected_page=1 expected_text=$timeout_title ;; esac
    config_copy="$run_root/config/config.toml"
    copy_config_without_notifications "$scenario_config" "$config_copy"
    cp "$config_copy" "$output/raw/$run_id.config.toml"
    printf 'source_sha256=%s\ncopy_sha256=%s\n' "$(sha256sum "$scenario_config" | awk '{print $1}')" "$(sha256sum "$config_copy" | awk '{print $1}')" >"$output/raw/$run_id.config-evidence"
    [[ ! -f $(dirname "$scenario_config")/machines.toml ]] || cp "$(dirname "$scenario_config")/machines.toml" "$run_root/config/machines.toml"
    path_value=$PATH
    if [[ $scenario == timeout ]]; then
        cat >"$run_root/wrapper-bin/$timeout_command" <<'EOF'
#!/bin/sh
: "${PLASMA_TOP_TIMEOUT_LOG:?}"
: "${PLASMA_TOP_TIMEOUT_DELAY:?}"
read -r started _ </proc/uptime
printf 'start=%s pid=%s ppid=%s\n' "$started" "$$" "$PPID" >>"$PLASMA_TOP_TIMEOUT_LOG"
exec sleep "$PLASMA_TOP_TIMEOUT_DELAY"
EOF
        chmod +x "$run_root/wrapper-bin/$timeout_command"
        path_value="$run_root/wrapper-bin:$PATH"
    fi
    if [[ $scenario == unavailable-service && $implementation == baseline ]]; then
        write_busctl_wrapper "$run_root/wrapper-bin/busctl"
        path_value="$run_root/wrapper-bin:$PATH"
    fi
    local -a affinity=() child_env=(env -i LC_ALL=C HOME="$run_root/home" XDG_CONFIG_HOME="$run_root/config-home" XDG_CACHE_HOME="$run_root/cache-home" XDG_DATA_HOME="$run_root/data-home" XDG_RUNTIME_DIR="$run_root/runtime-home" TMPDIR="$run_root/tmp" PLASMA_TOP_CODE_ROOT="$assets" PATH="$path_value" DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS-}")
    if [[ $scenario == timeout ]]; then child_env+=(PLASMA_TOP_TIMEOUT_LOG="$run_root/timeout-wrapper.log" PLASMA_TOP_TIMEOUT_DELAY="$timeout_delay"); fi
    if [[ $scenario == unavailable-service && $implementation == baseline ]]; then child_env+=(PLASMA_TOP_BUSCTL_LOG="$run_root/busctl-wrapper.log"); fi
    [[ $mode != cpu0 ]] || affinity=(taskset -c 0)
    local -a launch_prefix=()
    if ((resource_only)); then
        [[ -z ${DBUS_SYSTEM_BUS_ADDRESS-} ]] || child_env+=(DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SYSTEM_BUS_ADDRESS")
    else
        trace_file="$output/raw/$run_id.process.strace"
        trace_expression=process
        if [[ $scenario == unavailable-service && $implementation == candidate ]]; then
            child_env+=(DBUS_SYSTEM_BUS_ADDRESS="unix:path=$run_root/nonexistent-system-bus.sock")
            trace_expression=process,connect
        elif [[ -n ${DBUS_SYSTEM_BUS_ADDRESS-} ]]; then child_env+=(DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SYSTEM_BUS_ADDRESS"); fi
        launch_prefix=(strace -f -qq -ttt -e "trace=$trace_expression" -s 256 -o "$trace_file")
    fi

    # Candidate presentation can be established before daemon launch; baseline has no equivalent protocol.
    if [[ $implementation == candidate && $scenario != hidden ]]; then run_cli "$bin" "$assets" "$run_root" "$path_value" present 14001; fi
    printf -v launch_command '%s ' "${launch_prefix[@]}" /usr/bin/time -v "${affinity[@]}" "$bin" daemon --config "$config_copy"
    printf '%s\t%s\t%s\t%d\t%s\n' "$implementation" "$mode" "$scenario" "$iteration" "${launch_command% }" >>"$output/commands.tsv"
    printf '%q ' "${launch_prefix[@]}" /usr/bin/time -v -o "$output/raw/$run_id.time" "${child_env[@]}" "${affinity[@]}" "$bin" daemon --config "$config_copy" >"$output/raw/$run_id.command"
    printf '\n' >>"$output/raw/$run_id.command"
    sed -i 's/ $//' "$output/raw/$run_id.command"
    printf 'startup\n' >"$run_root/phase"
    : >"$run_root/samples.tsv"
    : >"$run_root/identities.tsv"
    deadline=$(deadline_add "$(monotonic)" "$startup_timeout")
    startup_epoch=$(date +%s.%N)
    read -r startup_monotonic _ </proc/uptime
    write_launch_snapshot "$startup_monotonic" "$run_root/start.snapshot"
    "${launch_prefix[@]}" /usr/bin/time -v -o "$output/raw/$run_id.time" "${child_env[@]}" "${affinity[@]}" "$bin" daemon --config "$config_copy" >"$output/raw/$run_id.out" 2>"$output/raw/$run_id.err" &
    local launcher_pid=$!
    while [[ -z $daemon_pid ]]; do
        while read -r pid; do
            [[ $(readlink "/proc/$pid/exe" 2>/dev/null || true) == "$(readlink -f "$bin")" ]] && daemon_pid=$pid && break
        done < <(session_members)
        [[ -r /proc/$launcher_pid/stat ]] || die "$run_id exited before daemon PID capture"
        awk -v now="$(monotonic)" -v end="$deadline" 'BEGIN{exit !(now>=end)}' && die "$run_id daemon PID capture timed out"
        [[ -n $daemon_pid ]] || sleep 0.01
    done
    active_pid=$daemon_pid active_starttime=$(proc_starttime "$daemon_pid") daemon_start=$active_starttime
    [[ $(proc_session "$daemon_pid") == "$owned_session" ]] || die "$run_id escaped owned session"
    record_identity "$daemon_pid" "$run_root/identities.tsv"
    sample_run "$daemon_pid" "$runtime" "$run_root/phase" "$run_root/samples.tsv" "$run_root/identities.tsv" "$run_root/sampler.stop" &
    sampler_pid=$!

    wait_for_file "$runtime/panel.html" "$deadline" || die "$run_id did not publish panel"
    if [[ $implementation == candidate && $scenario == hidden ]]; then
        run_cli "$bin" "$assets" "$run_root" "$path_value" present 14001
        wait_for_file "$runtime/tooltip.html" "$deadline" || die "$run_id could not prime tooltip"
        run_cli "$bin" "$assets" "$run_root" "$path_value" dismiss 14001
    else wait_for_file "$runtime/tooltip.html" "$deadline" || die "$run_id did not publish tooltip"; fi
    verify_html "$runtime/panel.html" "$runtime/tooltip.html" || die "$run_id published invalid HTML"
    write_snapshot "$daemon_pid" "$run_root/startup-end.snapshot"
    startup_end_epoch=$(date +%s.%N)
    printf 'setup\n' >"$run_root/phase"

    setup_sig=$(stat -Lc '%Y:%s:%i' "$runtime/tooltip.html")
    if ((expected_page > 0)); then
        for ((step = 0; step < expected_page; step++)); do run_cli "$bin" "$assets" "$run_root" "$path_value" page next; done
        wait_for_fresh_text "$runtime/tooltip.html" "$expected_text" "$setup_sig" "$deadline" || die "$run_id did not freshly render $expected_text"
        if [[ $scenario == timeout ]]; then
            local timeout_start timeout_end timeout_pid
            while [[ ! -s $run_root/timeout-wrapper.log ]]; do sleep 0.02; done
            timeout_start=$(sed -n 's/^start=\([^ ]*\).*/\1/p' "$run_root/timeout-wrapper.log" | head -1)
            timeout_pid=$(sed -n 's/.* pid=\([0-9]*\).*/\1/p' "$run_root/timeout-wrapper.log" | head -1)
            while [[ -r /proc/$timeout_pid/stat ]]; do sleep 0.02; done
            timeout_end=$(monotonic)
            timeout_elapsed=$(awk -v a="$timeout_start" -v b="$timeout_end" 'BEGIN { printf "%.3f", b-a }')
            awk -v elapsed="$timeout_elapsed" 'BEGIN { exit !(elapsed >= 4.5 && elapsed <= 5.7) }' || die "$run_id wrapper termination was not around 5s ($timeout_elapsed)"
            printf 'end=%s status=killed-before-configured-delay elapsed=%s configured_delay=%s\n' "$timeout_end" "$timeout_elapsed" "$timeout_delay" >>"$run_root/timeout-wrapper.log"
            local post_attempt_deadline
            post_attempt_deadline=$(deadline_add "$timeout_end" 1)
            while ! grep -Eiq 'ss:.*(timed out|terminated|unavailable)' "$runtime/tooltip.html"; do
                awk -v now="$(monotonic)" -v end="$post_attempt_deadline" 'BEGIN { exit !(now >= end) }' && break
                sleep 0.02
            done
            if grep -Eiq 'ss:.*(timed out|terminated|unavailable)' "$runtime/tooltip.html"; then
                timeout_detail='published:ss-timeout-detail'
            elif grep -Fq "$timeout_title" "$runtime/tooltip.html" && grep -Fq ':after' "$runtime/tooltip.html"; then
                timeout_detail="protocol-limitation:$implementation-command-page-parser-renders-timeout-as-dash-colon-after"
            elif grep -Fq "$timeout_title" "$runtime/tooltip.html"; then
                timeout_detail="protocol-limitation:$implementation-has-no-visible-post-attempt-failure-detail"
            else
                cp "$runtime/tooltip.html" "$output/raw/$run_id.timeout-failed-tooltip.html" 2>/dev/null || true
                die "$run_id did not publish supported timeout detail"
            fi
            cp "$runtime/tooltip.html" "$output/raw/$run_id.timeout-post-attempt-tooltip.html"
            printf 'timeout_detail=%s\n' "$timeout_detail" >>"$run_root/timeout-wrapper.log"
        fi
    fi
    if [[ $implementation == candidate && $scenario == hidden ]]; then
        deadline=$(deadline_add "$(monotonic)" 1.1)
        wait_until "$deadline"
        [[ ! -e $runtime/state/presented/14001 ]] || die "$run_id hidden lease remained"
    fi
    [[ $(tr -d '[:space:]' <"$runtime/state/page") == "$expected_page" ]] || die "$run_id wrong page after setup"
    if [[ $scenario == unavailable-service ]]; then
        deadline=$(deadline_add "$(monotonic)" "$startup_timeout")
        if [[ $implementation == baseline ]]; then
            while ! grep -Eq 'argv=.* --system([[:space:]]|$)' "$run_root/busctl-wrapper.log" 2>/dev/null; do
                awk -v now="$(monotonic)" -v end="$deadline" 'BEGIN{exit !(now>=end)}' && die "$run_id has no logged busctl --system attempt"
                sleep 0.05
            done
            protocol_note='matched user-visible unavailable service; baseline safely intercepts PATH busctl --system, logs exact argv/timestamp/attempt, and returns 69 without a real bus call'
        else
            while ! grep -Eq 'connect\(.*nonexistent-system-bus\.sock.*= -1' "$trace_file"; do
                awk -v now="$(monotonic)" -v end="$deadline" 'BEGIN{exit !(now>=end)}' && die "$run_id has no proved failed system-bus connect/call"
                sleep 0.05
            done
            protocol_note='matched user-visible unavailable service; candidate retains a child-only nonexistent DBUS_SYSTEM_BUS_ADDRESS and failed-connect strace proof'
        fi
    fi

    setup_end_epoch=$(date +%s.%N)
    write_snapshot "$daemon_pid" "$run_root/setup-end.snapshot"
    steady_start_epoch=$setup_end_epoch
    write_snapshot "$daemon_pid" "$run_root/steady-start.snapshot"
    printf 'steady\n' >"$run_root/phase"
    steady_deadline=$(deadline_add "$(monotonic)" "$duration")
    wait_until "$steady_deadline"
    # Deadline snapshot is intentionally first; all health/output checks are post-window.
    write_snapshot "$daemon_pid" "$run_root/steady-end.snapshot" || die "$run_id daemon was absent at steady deadline"
    steady_end_epoch=$(date +%s.%N)
    [[ $(proc_starttime "$daemon_pid") == "$daemon_start" ]] || die "$run_id daemon identity changed"
    [[ $(tr -d '[:space:]' <"$runtime/state/page") == "$expected_page" ]] || die "$run_id page changed"
    [[ -z $expected_text ]] || grep -Fq "$expected_text" "$runtime/tooltip.html" || die "$run_id missing page title"
    verify_html "$runtime/panel.html" "$runtime/tooltip.html" || die "$run_id final HTML invalid"
    if [[ $scenario == timeout ]]; then
        [[ $timeout_detail == published:* || $timeout_detail == protocol-limitation:* ]] || die "$run_id missing truthful timeout result"
    fi

    cp "$runtime/panel.html" "$output/raw/$run_id.panel.html"
    cp "$runtime/tooltip.html" "$output/raw/$run_id.tooltip.html"
    # Stop cooperatively so the sampler reaps its foreground sleep before session leak detection.
    : >"$run_root/sampler.stop"
    deadline=$(deadline_add "$(monotonic)" "$(deadline_add "$sample_interval" 1)")
    while kill -0 "$sampler_pid" 2>/dev/null; do
        awk -v now="$(monotonic)" -v end="$deadline" 'BEGIN { exit !(now >= end) }' && die "$run_id sampler did not stop within the sampling interval"
        sleep 0.01
    done
    wait "$sampler_pid" || die "$run_id sampler failed during shutdown"
    sampler_pid=""
    cp "$run_root/samples.tsv" "$output/raw/$run_id.samples.tsv"
    cp "$run_root/identities.tsv" "$output/raw/$run_id.identities.tsv"
    [[ ! -f $run_root/timeout-wrapper.log ]] || cp "$run_root/timeout-wrapper.log" "$output/raw/$run_id.timeout-wrapper.log"
    if [[ $scenario == timeout ]]; then
        # Signal during the active retry just before its own deadline so both daemons leave their blocking/async command path and honor graceful shutdown.
        local active_timeout_start active_timeout_pid graceful_signal_at
        active_timeout_start=$(sed -n 's/^start=\([^ ]*\).*/\1/p' "$run_root/timeout-wrapper.log" | tail -1)
        active_timeout_pid=$(sed -n 's/.* pid=\([0-9]*\).*/\1/p' "$run_root/timeout-wrapper.log" | tail -1)
        if [[ -r /proc/$active_timeout_pid/stat ]]; then
            graceful_signal_at=$(deadline_add "$active_timeout_start" 4.8)
            wait_until "$graceful_signal_at"
        fi
    fi
    shutdown_start=$(monotonic)
    kill -TERM "$daemon_pid"
    deadline=$(deadline_add "$shutdown_start" "$shutdown_timeout")
    while [[ -r /proc/$daemon_pid/stat && $(proc_starttime "$daemon_pid") == "$daemon_start" ]]; do
        if awk -v now="$(monotonic)" -v end="$deadline" 'BEGIN{exit !(now>=end)}'; then
            forced_kill=1
            signal_identity KILL "$daemon_pid" "$daemon_start" "$owned_session"
            break
        fi
        sleep 0.02
    done
    set +e
    wait "$launcher_pid"
    daemon_exit=$?
    set -e
    shutdown_end=$(monotonic)
    active_pid="" active_starttime=""
    ((forced_kill == 0)) || die "$run_id required forced daemon termination"
    ((daemon_exit == 0)) || die "$run_id daemon exit was $daemon_exit, expected 0"
    [[ ! -f $run_root/busctl-wrapper.log ]] || cp "$run_root/busctl-wrapper.log" "$output/raw/$run_id.busctl-wrapper.log"
    {
        printf 'window\tepoch_start\tepoch_end\n'
        printf 'startup\t%s\t%s\n' "$startup_epoch" "$startup_end_epoch"
        printf 'setup\t%s\t%s\n' "$startup_end_epoch" "$setup_end_epoch"
        printf 'steady\t%s\t%s\n' "$steady_start_epoch" "$steady_end_epoch"
    } >"$output/raw/$run_id.windows.tsv"
    window_metrics "$implementation" "$mode" "$scenario" "$iteration" startup "$run_root/start.snapshot" "$run_root/startup-end.snapshot" "$run_root/samples.tsv" "$trace_file" "$daemon_pid" "$startup_epoch" "$startup_end_epoch"
    window_metrics "$implementation" "$mode" "$scenario" "$iteration" setup "$run_root/startup-end.snapshot" "$run_root/setup-end.snapshot" "$run_root/samples.tsv" "$trace_file" "$daemon_pid" "$startup_end_epoch" "$setup_end_epoch"
    window_metrics "$implementation" "$mode" "$scenario" "$iteration" steady "$run_root/steady-start.snapshot" "$run_root/steady-end.snapshot" "$run_root/samples.tsv" "$trace_file" "$daemon_pid" "$steady_start_epoch" "$steady_end_epoch"

    verify_state "$runtime" "$output/raw/$run_id.state.tsv" && state_check='pass:no_atomic_temps_and_only_intentionally_retained_protocol_files' || state_check='fail:unexpected_state_detected_and_owned_root_will_be_deleted'
    local leftovers="$run_root/leftover-identities.tsv" remaining="$run_root/remaining-identities.tsv" pid
    : >"$leftovers"
    while read -r pid; do [[ $pid == "$owned_session" ]] || record_identity "$pid" "$leftovers" || true; done < <(session_members)
    if [[ -s $leftovers ]]; then
        process_check='fail:owned_session_members_detected_then_identity_verified_and_terminated'
        terminate_owned_session "$leftovers" || {
            : >"$remaining"
            while read -r pid; do [[ $pid == "$owned_session" ]] || record_identity "$pid" "$remaining" || true; done < <(session_members)
            cp "$remaining" "$output/raw/$run_id.remaining-identities.tsv"
            die "$run_id owned session members remained after bounded termination"
        }
    else process_check='pass:no_owned_session_members_after_daemon_exit'; fi
    verify_state "$runtime" "$output/raw/$run_id.state.tsv" || die "$run_id left state outside allowlist"
    [[ $process_check == pass:* ]] || [[ $scenario == timeout && $implementation == baseline ]] || die "$run_id left unexpected session members"
    printf '%s\t%s\t%s\t%d\t%d\t%d\t%.6f\t%s\t%s\t%s; timeout_elapsed=%s; timeout_detail=%s\n' "$implementation" "$mode" "$scenario" "$iteration" "$daemon_exit" "$forced_kill" "$(awk -v a="$shutdown_start" -v b="$shutdown_end" 'BEGIN{print b-a}')" "$state_check" "$process_check" "$protocol_note" "$timeout_elapsed" "$timeout_detail" >>"$output/results.tsv"
    rm -rf -- "$run_root"
}

while IFS=$'\t' read -r implementation mode scenario iteration _; do run_one "$implementation" "$mode" "$scenario" "$iteration"; done < <(print_matrix)
if contains_csv "$implementations" baseline; then
    [[ $(sha256sum "$baseline_bin" | awk '{print $1}') == "$baseline_frozen_sha256" ]] || die "baseline frozen binary changed during the matrix"
fi
if contains_csv "$implementations" candidate; then
    [[ $(sha256sum "$candidate_bin" | awk '{print $1}') == "$candidate_frozen_sha256" ]] || die "candidate frozen binary changed during the matrix"
fi
for scenario in ${scenarios//,/ }; do
    find "$output/raw" -maxdepth 1 -name "*-$scenario-*.config.toml" -print0 | sort -z | xargs -0 sha256sum >"$output/$scenario-config-copies.sha256"
    [[ $(awk '{ print $1 }' "$output/$scenario-config-copies.sha256" | sort -u | wc -l) -le 1 ]] || die "$scenario transformed configs were not identical"
done
find "$owned_root" -mindepth 1 -print -quit | grep -q . && die "owned root not empty after runs"
printf 'completed output=%s\n' "$output"
