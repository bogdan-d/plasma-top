#!/usr/bin/awk -f
# Derive median issue-14 tables from one complete harness evidence directory.

BEGIN {
    FS = OFS = "\t"
    if (ARGC != 2) {
        fail("usage: summarize.awk EVIDENCE_DIR")
    }
    root = ARGV[1]
    delete ARGV[1]

    load_manifest(root "/manifest.txt")
    require_manifest("implementations")
    require_manifest("modes")
    require_manifest("scenarios")
    require_manifest("repeat")
    require_manifest("resource_only")
    implementation_count = split(run_manifest["implementations"], implementations, ",")
    mode_count = split(run_manifest["modes"], modes, ",")
    scenario_count = split(run_manifest["scenarios"], scenarios, ",")
    repetitions = run_manifest["repeat"] + 0
    expected_runs = implementation_count * mode_count * scenario_count * repetitions

    "getconf CLK_TCK" | getline ticks
    close("getconf CLK_TCK")
    if (ticks !~ /^[1-9][0-9]*$/) {
        fail("could not read SC_CLK_TCK")
    }

    load_results(root "/results.tsv")
    load_commands(root "/commands.tsv")
    load_metrics(root "/metrics.tsv")
    if (result_count != expected_runs || command_count != expected_runs || metric_count != expected_runs * 3) {
        fail("expected " expected_runs " results and commands, and " expected_runs * 3 " metric windows")
    }

    write_summary(root "/summary.tsv")
    if (has_implementation("baseline") && has_implementation("candidate")) {
        write_comparison(root "/comparison.tsv")
    }
    write_instrumented_tree(root "/instrumented-tree.tsv")
}

function fail(message) {
    print "summarize.awk: " message > "/dev/stderr"
    exit 1
}

function load_manifest(path, line, separator, status) {
    while ((status = getline line < path) > 0) {
        separator = index(line, "=")
        if (separator) {
            run_manifest[substr(line, 1, separator - 1)] = substr(line, separator + 1)
        }
    }
    close(path)
    if (status < 0) {
        fail("cannot read " path)
    }
}

function require_manifest(name) {
    if (!(name in run_manifest)) {
        fail("manifest is missing " name)
    }
}

function read_header(path, fields, line, count, i) {
    if ((getline line < path) <= 0) {
        fail("cannot read header from " path)
    }
    count = split(line, header_columns, FS)
    for (i = 1; i <= count; i++) {
        fields[header_columns[i]] = i
    }
    return count
}

function require_field(fields, name, path) {
    if (!(name in fields)) {
        fail(path " is missing " name)
    }
}

function load_results(path, line, columns, status, run) {
    read_header(path, result_field)
    require_field(result_field, "implementation", path)
    require_field(result_field, "mode", path)
    require_field(result_field, "scenario", path)
    require_field(result_field, "repetition", path)
    require_field(result_field, "daemon_exit", path)
    require_field(result_field, "forced_kill", path)
    require_field(result_field, "state_check", path)
    require_field(result_field, "process_check", path)
    while ((status = getline line < path) > 0) {
        split(line, columns, FS)
        if (columns[result_field["daemon_exit"]] != "0" || columns[result_field["forced_kill"]] != "0" ||
            index(columns[result_field["state_check"]], "pass:") != 1 || index(columns[result_field["process_check"]], "pass:") != 1) {
            fail("one or more runs failed health or cleanup checks")
        }
        run = columns[result_field["implementation"]] "-" columns[result_field["mode"]] "-" columns[result_field["scenario"]] "-" columns[result_field["repetition"]]
        if (run in result_run_seen) {
            fail("duplicate result for " run)
        }
        result_run_seen[run] = 1
        result_runs[++result_count] = run
    }
    close(path)
    if (status < 0) {
        fail("cannot read " path)
    }
}

function load_commands(path, line, status) {
    if ((getline line < path) <= 0) {
        fail("cannot read header from " path)
    }
    while ((status = getline line < path) > 0) {
        command_count++
    }
    close(path)
    if (status < 0) {
        fail("cannot read " path)
    }
}

function load_metrics(path, line, columns, status, implementation, mode, scenario, window, group, count, field_index) {
    read_header(path, metric_field)
    split("implementation mode scenario window wall_seconds user_ticks system_ticks rss_peak_kib voluntary_context_switches involuntary_context_switches child_exec_attempts successful_child_execs child_exits_zero publication_writes publication_bytes", required_metric_fields, " ")
    for (field_index in required_metric_fields) {
        require_field(metric_field, required_metric_fields[field_index], path)
    }
    while ((status = getline line < path) > 0) {
        split(line, columns, FS)
        implementation = columns[metric_field["implementation"]]
        mode = columns[metric_field["mode"]]
        scenario = columns[metric_field["scenario"]]
        window = columns[metric_field["window"]]
        group = implementation SUBSEP mode SUBSEP scenario SUBSEP window
        count = ++group_count[group]
        if (!(group in group_seen)) {
            group_seen[group] = 1
            group_labels[++group_total] = implementation OFS mode OFS scenario OFS window
        }
        for (field_index in required_metric_fields) {
            metric[group, required_metric_fields[field_index], count] = columns[metric_field[required_metric_fields[field_index]]]
        }
        metric_count++
    }
    close(path)
    if (status < 0) {
        fail("cannot read " path)
    }
}

function sort_strings(source, count, destination, i, previous) {
    for (i = 1; i <= count; i++) {
        destination[i] = source[i]
        for (previous = i; previous > 1 && destination[previous] < destination[previous - 1]; previous--) {
            swap = destination[previous]
            destination[previous] = destination[previous - 1]
            destination[previous - 1] = swap
        }
    }
}

function numeric_median(group, field, count, i, previous, value) {
    count = group_count[group]
    for (i = 1; i <= count; i++) {
        sorted_numbers[i] = metric[group, field, i] + 0
        for (previous = i; previous > 1 && sorted_numbers[previous] < sorted_numbers[previous - 1]; previous--) {
            value = sorted_numbers[previous]
            sorted_numbers[previous] = sorted_numbers[previous - 1]
            sorted_numbers[previous - 1] = value
        }
    }
    value = count % 2 ? sorted_numbers[(count + 1) / 2] : (sorted_numbers[count / 2] + sorted_numbers[count / 2 + 1]) / 2
    for (i = 1; i <= count; i++) {
        delete sorted_numbers[i]
    }
    return value
}

function cpu_median(group, count, i, previous, value) {
    count = group_count[group]
    for (i = 1; i <= count; i++) {
        sorted_numbers[i] = (metric[group, "user_ticks", i] + metric[group, "system_ticks", i]) / ticks
        for (previous = i; previous > 1 && sorted_numbers[previous] < sorted_numbers[previous - 1]; previous--) {
            value = sorted_numbers[previous]
            sorted_numbers[previous] = sorted_numbers[previous - 1]
            sorted_numbers[previous - 1] = value
        }
    }
    value = count % 2 ? sorted_numbers[(count + 1) / 2] : (sorted_numbers[count / 2] + sorted_numbers[count / 2 + 1]) / 2
    for (i = 1; i <= count; i++) {
        delete sorted_numbers[i]
    }
    return value
}

function median_or_na(group, field, count, i, value, unavailable) {
    count = group_count[group]
    unavailable = 0
    for (i = 1; i <= count; i++) {
        value = metric[group, field, i]
        if (value == "NA:untraced_resource_run") {
            unavailable++
        } else if (index(value, "NA:") == 1) {
            fail("unsupported unavailable value for " field)
        }
    }
    if (unavailable == count) {
        return "NA:untraced_resource_run"
    }
    if (unavailable) {
        fail("mixed available and unavailable values for " field)
    }
    return sprintf("%.0f", numeric_median(group, field))
}

function write_summary(path, i, parts, group, child_attempts, successful_children, successful_child_exits, cpu, rss, voluntary, involuntary, publications, bytes) {
    print "implementation", "mode", "scenario", "window", "repetitions", "wall_median_s", "daemon_cpu_median_s", "sampled_peak_rss_median_kib", "leader_thread_voluntary_context_switches_median", "leader_thread_involuntary_context_switches_median", "child_process_attempts_median", "successful_children_median", "successful_child_exits_median", "publication_changes_median", "publication_bytes_median" > path
    sort_strings(group_labels, group_total, sorted_groups)
    for (i = 1; i <= group_total; i++) {
        split(sorted_groups[i], parts, FS)
        group = parts[1] SUBSEP parts[2] SUBSEP parts[3] SUBSEP parts[4]
        if (group_count[group] != repetitions) {
            fail("expected " repetitions " repetitions for " sorted_groups[i])
        }
        cpu = sprintf("%.3f", cpu_median(group))
        rss = sprintf("%.0f", numeric_median(group, "rss_peak_kib"))
        voluntary = sprintf("%.0f", numeric_median(group, "voluntary_context_switches"))
        involuntary = sprintf("%.0f", numeric_median(group, "involuntary_context_switches"))
        child_attempts = median_or_na(group, "child_exec_attempts")
        successful_children = median_or_na(group, "successful_child_execs")
        successful_child_exits = median_or_na(group, "child_exits_zero")
        publications = sprintf("%.0f", numeric_median(group, "publication_writes"))
        bytes = sprintf("%.0f", numeric_median(group, "publication_bytes"))
        print parts[1], parts[2], parts[3], parts[4], group_count[group], sprintf("%.3f", numeric_median(group, "wall_seconds")), cpu, rss, voluntary, involuntary, child_attempts, successful_children, successful_child_exits, publications, bytes >> path
        summary_value[group, "daemon_cpu_median_s"] = cpu
        summary_value[group, "sampled_peak_rss_median_kib"] = rss
        summary_value[group, "leader_thread_voluntary_context_switches_median"] = voluntary
        summary_value[group, "child_process_attempts_median"] = child_attempts
        summary_value[group, "publication_changes_median"] = publications
        summary_value[group, "publication_bytes_median"] = bytes
    }
    close(path)
}

function has_implementation(name, i) {
    for (i = 1; i <= implementation_count; i++) {
        if (implementations[i] == name) {
            return 1
        }
    }
    return 0
}

function write_comparison(path, mode_index, scenario_index, window_index, mode, scenario, window, baseline, candidate, child_delta) {
    print "mode", "scenario", "window", "cpu_delta_s", "sampled_peak_rss_delta_kib", "leader_thread_voluntary_context_switch_delta", "child_attempt_delta", "publication_change_delta", "publication_byte_delta" > path
    for (mode_index = 1; mode_index <= mode_count; mode_index++) {
        mode = modes[mode_index]
        for (scenario_index = 1; scenario_index <= scenario_count; scenario_index++) {
            scenario = scenarios[scenario_index]
            for (window_index = 1; window_index <= 3; window_index++) {
                window = window_index == 1 ? "startup" : (window_index == 2 ? "setup" : "steady")
                baseline = "baseline" SUBSEP mode SUBSEP scenario SUBSEP window
                candidate = "candidate" SUBSEP mode SUBSEP scenario SUBSEP window
                if (!(baseline in group_seen) || !(candidate in group_seen)) {
                    fail("missing comparison group for " mode "/" scenario "/" window)
                }
                child_delta = run_manifest["resource_only"] == "1" ? "NA:untraced_resource_run" : sprintf("%.0f", summary_value[candidate, "child_process_attempts_median"] - summary_value[baseline, "child_process_attempts_median"])
                print mode, scenario, window,
                    sprintf("%.3f", summary_value[candidate, "daemon_cpu_median_s"] - summary_value[baseline, "daemon_cpu_median_s"]),
                    sprintf("%.0f", summary_value[candidate, "sampled_peak_rss_median_kib"] - summary_value[baseline, "sampled_peak_rss_median_kib"]),
                    sprintf("%.0f", summary_value[candidate, "leader_thread_voluntary_context_switches_median"] - summary_value[baseline, "leader_thread_voluntary_context_switches_median"]),
                    child_delta,
                    sprintf("%.0f", summary_value[candidate, "publication_changes_median"] - summary_value[baseline, "publication_changes_median"]),
                    sprintf("%.0f", summary_value[candidate, "publication_bytes_median"] - summary_value[baseline, "publication_bytes_median"]) >> path
            }
        }
    }
    close(path)
}

function time_value(path, label, line, trimmed, prefix, status) {
    prefix = label ":"
    while ((status = getline line < path) > 0) {
        trimmed = line
        sub(/^[[:space:]]*/, "", trimmed)
        if (index(trimmed, prefix) == 1) {
            trimmed = substr(trimmed, length(prefix) + 1)
            sub(/^[[:space:]]*/, "", trimmed)
            close(path)
            return trimmed
        }
    }
    close(path)
    if (status < 0) {
        fail("cannot read " path)
    }
    fail("missing " label " in " path)
}

function write_instrumented_tree(path, i, run, time_path, user, system_time, rss, voluntary, involuntary, status) {
    print "run", "instrumented_tree_user_s", "instrumented_tree_system_s", "instrumented_tree_peak_rss_kib", "instrumented_tree_voluntary_context_switches", "instrumented_tree_involuntary_context_switches", "exit_status" > path
    sort_strings(result_runs, result_count, sorted_runs)
    for (i = 1; i <= result_count; i++) {
        run = sorted_runs[i]
        time_path = root "/raw/" run ".time"
        user = time_value(time_path, "User time (seconds)")
        system_time = time_value(time_path, "System time (seconds)")
        rss = time_value(time_path, "Maximum resident set size (kbytes)")
        voluntary = time_value(time_path, "Voluntary context switches")
        involuntary = time_value(time_path, "Involuntary context switches")
        status = time_value(time_path, "Exit status")
        if (status != "0") {
            fail("incomplete or failed /usr/bin/time evidence")
        }
        print run, user, system_time, rss, voluntary, involuntary, status >> path
    }
    close(path)
}
