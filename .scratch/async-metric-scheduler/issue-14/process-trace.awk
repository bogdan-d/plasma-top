# Counts distinct process outcomes only in the descendant tree rooted at daemon_pid.
# Invoke with the same trace twice: the first pass records every edge and successful exec, then the second records PIDs with events in the requested window.
function discover_descendants(    changed, edge, parts) {
    descendants[daemon_pid] = 1
    do {
        changed = 0
        for (edge in edges) {
            split(edge, parts, SUBSEP)
            if (descendants[parts[1]] && !descendants[parts[2]]) {
                descendants[parts[2]] = 1
                changed = 1
            }
        }
    } while (changed)
}

NR == FNR {
    if ($0 ~ / (clone3|clone|fork|vfork)\(/) {
        if ($0 ~ /<unfinished \.\.\./) {
            pending_clone[$1] = 1
            pending_clone_thread[$1] = ($0 ~ /CLONE_THREAD/)
        }
        child = $0
        sub(/^.* = /, "", child)
        sub(/[^0-9].*$/, "", child)
        if (child ~ /^[0-9]+$/) {
            edges[$1, child] = 1
            if ($0 ~ /CLONE_THREAD/) {
                clone_thread_child[child] = 1
            }
        }
    } else if ($0 ~ / <\.\.\. (clone3|clone|fork|vfork) resumed>/) {
        child = $0
        sub(/^.* = /, "", child)
        sub(/[^0-9].*$/, "", child)
        if (child ~ /^[0-9]+$/) {
            edges[$1, child] = 1
            if (pending_clone[$1] && pending_clone_thread[$1]) {
                clone_thread_child[child] = 1
            }
        }
        delete pending_clone[$1]
        delete pending_clone_thread[$1]
    }
    if ($0 ~ / execve(at)?\(.* = 0$/ || $0 ~ /<\.\.\. execve(at)? resumed>.* = 0$/) {
        exec_succeeded[$1] = 1
    }
    next
}

{
    if (!closure_ready) {
        discover_descendants()
        closure_ready = 1
    }
    pid = $1
    timestamp = $2
    if (!descendants[pid] || pid == daemon_pid || timestamp < window_start || timestamp > window_end) {
        next
    }
    if ($0 ~ / execve(at)?\(/) {
        if (!attempted[pid]) {
            attempted[pid] = 1
            attempts++
        }
        if ($0 ~ / = 0$/ && !succeeded[pid]) {
            succeeded[pid] = 1
            exec_successes++
        }
    } else if ($0 ~ /<\.\.\. execve(at)? resumed>.* = 0$/ && !succeeded[pid]) {
        succeeded[pid] = 1
        exec_successes++
    }
    if (exec_succeeded[pid] && !clone_thread_child[pid] && !exited_zero[pid] && $0 ~ / (exit_group|_exit|exit)\(0\) = \?$/) {
        exited_zero[pid] = 1
        exits_zero++
    }
}

END {
    printf "%d\t%d\t%d\n", attempts, exec_successes, exits_zero
}
