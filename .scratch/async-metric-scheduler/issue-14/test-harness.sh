#!/usr/bin/env bash
set -euo pipefail

DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
readonly DIR
ROOT=$(CDPATH='' cd -- "$DIR/../../.." && pwd)
readonly ROOT
readonly HARNESS="$DIR/harness.sh"
readonly PROCESS_TRACE_AWK="$DIR/process-trace.awk"

trace=$(mktemp "${TMPDIR:-/tmp}/plasma-top-issue14-trace.XXXXXXXX")
trap 'rm -f -- "$trace"; [[ -z ${source_bin-} ]] || rm -f -- "$source_bin"; [[ -z ${evidence-} ]] || rm -rf -- "$evidence"; [[ -z ${summary_evidence-} ]] || rm -rf -- "$summary_evidence"' EXIT
cat >"$trace" <<'EOF'
50 9.000000 execve("/usr/bin/time", ["time"], 0x0) = 0
100 10.000000 clone(child_stack=NULL, flags=SIGCHLD) = 101
101 10.100000 execve("/owned root/wrapper-bin/busctl", ["busctl", "--system", "call"], 0x0) = 0
101 10.110000 execve("/usr/bin/busctl-helper", ["busctl-helper"], 0x0) = 0
101 10.200000 clone(child_stack=NULL, flags=SIGCHLD) = 102
102 10.300000 execve("/missing-one/helper", ["helper"], 0x0) = -1 ENOENT
102 10.310000 execve("/missing-two/helper", ["helper"], 0x0) = -1 ENOENT
102 10.320000 execve("/usr/bin/helper", ["helper"], 0x0) = 0
102 10.330000 exit_group(0) = ?
100 10.350000 clone(child_stack=NULL, flags=CLONE_VM|CLONE_THREAD) = 104
104 10.355000 clone(child_stack=NULL, flags=SIGCHLD) = 108
108 10.357000 execve("/usr/bin/thread-child", ["thread-child"], 0x0) = 0
108 10.359000 exit_group(0) = ?
104 10.360000 exit(0) = ?
101 10.400000 exit_group(0) = ?
100 10.500000 clone3({flags=CLONE_VM|CLONE_VFORK}, 88 <unfinished ...>
103 10.510000 execve("/usr/bin/resumed", ["resumed"], 0x0 <unfinished ...>
100 10.520000 <... clone3 resumed>) = 103
103 10.530000 <... execve resumed>) = 0
103 10.540000 exit_group(0) = ?
105 10.600000 fork() = 106
106 10.610000 execve("/usr/bin/out-of-order-child", ["child"], 0x0) = 0
106 10.620000 vfork( <unfinished ...>
107 10.630000 execve("/usr/bin/out-of-order-grandchild", ["grandchild"], 0x0 <unfinished ...>
106 10.640000 <... vfork resumed>) = 107
107 10.650000 <... execve resumed>) = 0
107 10.660000 exit(0) = ?
106 10.670000 _exit(0) = ?
105 10.680000 execveat(3, "missing", ["missing"], 0x0, 0) = -1 ENOENT
105 10.690000 exit_group(0) = ?
100 10.700000 fork( <unfinished ...>
100 10.710000 <... fork resumed>) = 105
100 12.000000 exit_group(0) = ?
EOF
[[ $(awk -v daemon_pid=100 -v window_start=10 -v window_end=11 -f "$PROCESS_TRACE_AWK" "$trace" "$trace") == $'7\t6\t6' ]]

[[ $("$HARNESS" --deadline-add 123456.123456,1.1) == 123457.223456000 ]]
"$HARNESS" --cleanup-self-test | grep -q '^cleanup self-test passed$'
"$HARNESS" --busctl-wrapper-self-test | grep -q '^busctl wrapper self-test passed$'
"$HARNESS" --startup-snapshot-self-test | grep -q '^startup snapshot self-test passed$'

summary_evidence=$(mktemp -d "${TMPDIR:-/tmp}/plasma-top-issue14-summary.XXXXXXXX")
mkdir "$summary_evidence/raw"
cat >"$summary_evidence/manifest.txt" <<'EOF'
resource_only=1
implementations=candidate
modes=normal
scenarios=main
repeat=3
EOF
printf 'implementation\tmode\tscenario\trepetition\tdaemon_exit\tforced_kill\tstate_check\tprocess_check\n' >"$summary_evidence/results.tsv"
printf 'implementation\tmode\tscenario\trepetition\tcommand\n' >"$summary_evidence/commands.tsv"
printf 'implementation\tmode\tscenario\trepetition\twindow\twall_seconds\tuser_ticks\tsystem_ticks\trss_peak_kib\tvoluntary_context_switches\tinvoluntary_context_switches\tchild_exec_attempts\tsuccessful_child_execs\tchild_exits_zero\tpublication_writes\tpublication_bytes\n' >"$summary_evidence/metrics.tsv"
ticks=$(getconf CLK_TCK)
for repetition in 1 2 3; do
    printf 'candidate\tnormal\tmain\t%d\t0\t0\tpass:state\tpass:process\n' "$repetition" >>"$summary_evidence/results.tsv"
    printf 'candidate\tnormal\tmain\t%d\ttest\n' "$repetition" >>"$summary_evidence/commands.tsv"
    user_ticks=0 system_ticks=0
    ((repetition == 1)) && system_ticks=$ticks
    ((repetition == 3)) && user_ticks=$ticks
    for window in startup setup steady; do
        printf 'candidate\tnormal\tmain\t%d\t%s\t1\t%d\t%d\t10\t1\t0\tNA:untraced_resource_run\tNA:untraced_resource_run\tNA:untraced_resource_run\t1\t10\n' "$repetition" "$window" "$user_ticks" "$system_ticks" >>"$summary_evidence/metrics.tsv"
    done
    cat >"$summary_evidence/raw/candidate-normal-main-$repetition.time" <<'EOF'
User time (seconds): 0
System time (seconds): 0
Maximum resident set size (kbytes): 10
Voluntary context switches: 1
Involuntary context switches: 0
Exit status: 0
EOF
done
"$DIR/summarize.awk" "$summary_evidence"
awk -F '\t' 'NR > 1 { exit !($7 == "1.000") }' "$summary_evidence/summary.tsv"
head -1 "$summary_evidence/summary.tsv" | grep -q 'leader_thread_voluntary_context_switches_median'
rm -rf -- "$summary_evidence"
summary_evidence=""

output=$("$HARNESS" --dry-run --short)
grep -q $'candidate\tnormal\tmain\t1\t1' <<<"$output"
[[ $(wc -l <<<"$output") -eq 2 ]]

output=$("$HARNESS" --dry-run --resource-only --implementations candidate --modes normal --duration 2 --repeat 1)
[[ $(wc -l <<<"$output") -eq 5 ]]
grep -q $'candidate\tnormal\tprocesses\t1\t2' <<<"$output"

output=$("$HARNESS" --dry-run --implementations candidate --modes normal,cpu0 --scenarios hidden,timeout --repeat 2 --duration 3)
[[ $(wc -l <<<"$output") -eq 9 ]]
grep -q $'candidate\tcpu0\ttimeout\t2\t3' <<<"$output"

if "$HARNESS" --dry-run --implementations candidate --modes host --scenarios main >/dev/null 2>&1; then
    echo "invalid mode unexpectedly passed" >&2
    exit 1
fi

if "$HARNESS" --dry-run --timeout-delay 5.7 >/dev/null 2>&1; then
    echo "short timeout delay unexpectedly passed" >&2
    exit 1
fi

for rejected_scenario in unavailable-service timeout; do
    if "$HARNESS" --dry-run --resource-only --scenarios "$rejected_scenario" >/dev/null 2>&1; then
        echo "resource mode unexpectedly accepted $rejected_scenario" >&2
        exit 1
    fi
done

if [[ -n ${PLASMA_TOP_ISSUE14_SHORT_BIN-} ]]; then
    source_bin=$(mktemp "${TMPDIR:-/tmp}/plasma-top-issue14-input.XXXXXXXX")
    cp -- "$PLASMA_TOP_ISSUE14_SHORT_BIN" "$source_bin"
    chmod +x "$source_bin"
    source_hash=$(sha256sum "$source_bin" | awk '{print $1}')
    evidence=$(mktemp -u "${TMPDIR:-/tmp}/plasma-top-issue14-test.XXXXXXXX")
    "$HARNESS" --short --sample-interval 1 --candidate-root "$ROOT" --candidate-bin "$source_bin" --output "$evidence" &
    harness_pid=$!
    while [[ ! -s $evidence/manifest.txt ]]; do
        kill -0 "$harness_pid" 2>/dev/null || {
            wait "$harness_pid"
            exit 1
        }
        sleep 0.02
    done
    printf 'source binary replaced after evidence initialization\n' >"$source_bin"
    wait "$harness_pid"
    frozen="$evidence/binaries/candidate-plasma-top"
    [[ -x $frozen && $(stat -c '%a' "$frozen") == 555 ]]
    frozen_hash=$(sha256sum "$frozen" | awk '{print $1}')
    grep -Fqx "candidate_binary_input_path=$source_bin" "$evidence/manifest.txt"
    grep -Fqx "candidate_binary_input_sha256=$source_hash" "$evidence/manifest.txt"
    grep -Fqx "candidate_binary_frozen_relative_path=binaries/candidate-plasma-top" "$evidence/manifest.txt"
    grep -Fqx "candidate_binary_frozen_sha256=$frozen_hash" "$evidence/manifest.txt"
    grep -Fq "$frozen daemon --config" "$evidence/commands.tsv"
    grep -q $'candidate\tnormal\tmain\t1\tsteady' "$evidence/metrics.tsv"
    grep -q $'candidate\tnormal\tmain\t1' "$evidence/results.tsv"
    grep -q 'pass:no_owned_session_members_after_daemon_exit' "$evidence/results.tsv"
    grep -q 'pass:no_atomic_temps_and_only_intentionally_retained_protocol_files' "$evidence/results.tsv"
    awk -F '\t' 'NR == 2 { exit !($14 >= 0 && $15 >= 0 && $16 >= 0) }' "$evidence/metrics.tsv"
    [[ -s $evidence/raw/candidate-normal-main-1.process.strace ]]
    startup_epoch=$(awk -F '\t' '$1 == "startup" { print $2 }' "$evidence/raw/candidate-normal-main-1.windows.tsv")
    first_trace_epoch=$(awk 'NR == 1 { print $2; exit }' "$evidence/raw/candidate-normal-main-1.process.strace")
    awk -v startup="$startup_epoch" -v first_trace="$first_trace_epoch" 'BEGIN { exit !(startup <= first_trace) }'
    awk '/^\[notifications\]$/ { section=1; next } section && /^\[/ { exit } section && /^[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*=/ && $0 !~ /=[[:space:]]*false$/ { exit 1 }' "$evidence/raw/candidate-normal-main-1.config.toml"
    rm -rf -- "$evidence"
    rm -f -- "$source_bin"
    source_bin=""

    evidence=$(mktemp -u "${TMPDIR:-/tmp}/plasma-top-issue14-resource-test.XXXXXXXX")
    "$HARNESS" --resource-only --short --sample-interval 1 --candidate-root "$ROOT" --candidate-bin "$PLASMA_TOP_ISSUE14_SHORT_BIN" --output "$evidence"
    grep -q 'evidence_kind=untraced resource evidence' "$evidence/manifest.txt"
    awk -F '\t' 'NR > 1 { exit !($9 == "NA:untraced_resource_run" && $10 == "NA:untraced_resource_run" && $14 == "NA:untraced_resource_run" && $15 == "NA:untraced_resource_run" && $16 == "NA:untraced_resource_run") }' "$evidence/metrics.tsv"
    [[ ! -e $evidence/raw/candidate-normal-main-1.process.strace ]]
    grep -q '^/usr/bin/time ' "$evidence/raw/candidate-normal-main-1.command"
    for retained in config.toml panel.html tooltip.html samples.tsv identities.tsv; do
        [[ -s $evidence/raw/candidate-normal-main-1.$retained ]]
    done
    "$DIR/summarize.awk" "$evidence"
    grep -q 'NA:untraced_resource_run' "$evidence/summary.tsv"
fi

echo "issue-14 harness tests passed"
