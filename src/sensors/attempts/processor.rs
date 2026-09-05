use super::*;

/// Performs one CPU-owner source attempt with no cadence policy of its own.
pub(crate) fn attempt_cpu(
    state: &mut cpu::CpuState,
    proc_root: &Path,
    _sys_root: &Path,
    hw: &HardwareInventory,
    caps: &BTreeSet<Capability>,
    clock: &mut dyn FnMut() -> ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> CpuResult {
    let usage_at = clock().monotonic;
    let usage = timed(timings, "cpu_usage", || {
        cpu::read_cpu_usage_once(proc_root, state)
    });
    let usage = match usage {
        cpu::CpuUsageReadOutcome::Value(value) => ReadOutcome::Value(value),
        cpu::CpuUsageReadOutcome::Baseline | cpu::CpuUsageReadOutcome::InvalidDelta => {
            ReadOutcome::Baseline
        }
        cpu::CpuUsageReadOutcome::Failed => ReadOutcome::Failed,
    };
    let usage = commit_attempt(&mut state.usage, usage, usage_at);
    let temperature = if caps.contains(&Capability::CpuTemperature) && hw.cpu_temp_path.is_some() {
        if state.temperature_source != hw.cpu_temp_path {
            state.temperature = Default::default();
            state.temperature_source.clone_from(&hw.cpu_temp_path);
        }
        let captured_at = clock().monotonic;
        let value = timed(timings, "cpu_temp", || {
            hwmon::read_path_millidegrees_celsius(hw.cpu_temp_path.as_deref())
        });
        commit_attempt(
            &mut state.temperature,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.temperature.invalidate();
        state.temperature_source = None;
        cached_attempt(&state.temperature)
    };
    let frequency_mhz = if caps.contains(&Capability::CpuFrequency) {
        if state.frequency_source != hw.cpu_freq_path {
            state.frequency_mhz = Default::default();
            state.frequency_source.clone_from(&hw.cpu_freq_path);
        }
        let captured_at = clock().monotonic;
        let value = timed(timings, "cpu_freq", || {
            cpu::read_cpu_frequency_mhz(proc_root, hw.cpu_freq_path.as_deref())
        });
        commit_attempt(
            &mut state.frequency_mhz,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.frequency_mhz.invalidate();
        state.frequency_source = None;
        cached_attempt(&state.frequency_mhz)
    };
    let turbo = if caps.contains(&Capability::CpuTurbo)
        && hw.cpu_turbo_supported
        && hw.cpu_turbo_path.is_some()
    {
        if state.turbo_source != hw.cpu_turbo_path {
            state.turbo.invalidate();
            state.turbo_source.clone_from(&hw.cpu_turbo_path);
        }
        let captured_at = clock().monotonic;
        let value = timed(timings, "cpu_turbo", || {
            cpu::read_cpu_turbo_path(hw.cpu_turbo_path.as_deref())
        });
        commit_attempt(
            &mut state.turbo,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.turbo.invalidate();
        state.turbo_source = None;
        cached_attempt(&state.turbo)
    };
    let uptime_seconds = if caps.contains(&Capability::Uptime) {
        let captured_at = clock().monotonic;
        let value = timed(timings, "uptime", || cpu::read_uptime_seconds(proc_root));
        commit_attempt(
            &mut state.uptime_seconds,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.uptime_seconds.invalidate();
        cached_attempt(&state.uptime_seconds)
    };
    let load_average = if caps.contains(&Capability::LoadAverage) {
        let captured_at = clock().monotonic;
        let value = timed(timings, "load_avg", || cpu::read_load_average(proc_root))
            .map(|(one, five, fifteen)| LoadAverage { one, five, fifteen });
        commit_attempt(
            &mut state.load_average,
            value.map_or(ReadOutcome::Failed, ReadOutcome::Value),
            captured_at,
        )
    } else {
        state.load_average.invalidate();
        cached_attempt(&state.load_average)
    };

    CpuResult {
        usage,
        temperature,
        frequency_mhz,
        turbo,
        history: state
            .cpu_history_sample_at
            .map(|captured_at| MetricSample::new(state.cpu_history.clone(), captured_at)),
        uptime_seconds,
        load_average,
    }
}

/// Performs one per-core CPU owner attempt without cadence policy.
pub(crate) fn attempt_cpu_cores(
    state: &mut cpu::CpuState,
    proc_root: &Path,
    clock: ClockSnapshot,
    timings: &mut Option<&mut Timings>,
) -> CpuCoreResult {
    let attempted_at = clock.monotonic;
    let value = timed(timings, "cpu_cores", || {
        cpu::read_cpu_cores_once(proc_root, state)
    });
    let value = match value {
        cpu::CpuCoreReadOutcome::Value(value) => ReadOutcome::Value(value),
        cpu::CpuCoreReadOutcome::Baseline | cpu::CpuCoreReadOutcome::InvalidDelta => {
            ReadOutcome::Baseline
        }
        cpu::CpuCoreReadOutcome::Failed => ReadOutcome::Failed,
    };
    let usage = commit_attempt(&mut state.core_usage, value, attempted_at);
    let history = state
        .cpu_core_history
        .iter()
        .any(|values| !values.is_empty())
        .then(|| {
            MetricSample::new(
                state.cpu_core_history.clone(),
                state.cpu_core_history_sample_at.unwrap_or(attempted_at),
            )
        });
    CpuCoreResult { usage, history }
}
