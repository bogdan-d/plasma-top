# Synchronous baseline

Captured 2026-08-09 on the development host before async refactoring. This is development evidence only and is not laptop power evidence.

## Identity

The exact baseline checkout is commit `7c95731b105d704ba7b78d8cb5deb7a19f9277bf` (`docs: add .pi/ to .gitignore for tooling cache exclusion`). The planning commit supplied for this task is `1f76696` (`docs: plan async metric scheduler`). The working tree was clean before this issue's changes.

Release build command:

```text
cargo build --release --locked --features nvml
```

Toolchain: `rustc 1.97.1 (8bab26f4f 2026-07-14) (Homebrew)` and `cargo 1.97.1 (c980f4866 2026-06-30) (Homebrew)`. The measured binary is `target/release/plasma-top` with SHA-256 `c8187fa4f07103fd695fe9b63620a7f04f136bd6fc87f503c5b78833f2d33d0d`.

## Host

| Field | Value |
| --- | --- |
| Hostname | `FW-DESK-BZ` |
| OS | Bazzite Stable `F44.20260802` |
| Kernel | `7.1.5-ogc5.1.fc44.x86_64` |
| Architecture | `x86_64` |
| Hardware | Framework Desktop (AMD Ryzen AI Max 300 Series) |
| CPU | AMD RYZEN AI MAX+ 395 w/ Radeon 8060S |
| Logical CPUs | 32 |
| Power evidence | Not collected; this host is development-only evidence |

## Config and availability

All runs use `config/config.toml` and the shipped assets from the checkout. Config SHA-256 is `50260ac6747364493d1588250134bfda2fce9420ec6cc8a8994ba610efca6718`; `Cargo.lock` SHA-256 is `fc4689c863aebc4c265f5434f985be6b88728e5c562fd976c90886732579ef17`. The configured deep-dive pages are `graphs` and `processes`, with `display.poll_interval = 1.5` and `display.history_interval = 1.5`.

Available commands were `/usr/bin/busctl`, `/usr/bin/notify-send`, `/usr/bin/ip`, `/usr/bin/iw`, `/usr/bin/ss`, `/usr/bin/fastfetch`, and `/usr/bin/smartctl`. `nvidia-smi` was absent. The user D-Bus and plasmashell services were active; system D-Bus names for UPower and UDisks2 were present, although their systemd units reported inactive because they were bus-activated. `org.freedesktop.Notifications` was present on the user bus.

Probe output is saved in `runs/probe.out`. It recorded no system or peripheral batteries, no Intel GPU, no NVIDIA GPU, no fan readings, and no update or server status files. It found two NVMe devices and the `enp191s0` network device. Missing values remained absent rather than blocking the rest of the snapshot.

## Commands and results

Each scenario ran three times with `/usr/bin/time -f 'real_s=%e user_s=%U sys_s=%S max_rss_kb=%M'`. Raw stdout and timing files are under `runs/`; the complete timing table is `runs/timings.tsv`.

| Scenario | Exact command | Median real time | Median RSS |
| --- | --- | ---: | ---: |
| Hidden-equivalent collection | `target/release/plasma-top profiling --config config/config.toml` | 0.05 s | 7,360 KiB |
| Main tooltip | `target/release/plasma-top render --config config/config.toml --component tooltip --format text --page full` | 1.06 s | 15,388 KiB |
| Configured `graphs` page | `target/release/plasma-top render --config config/config.toml --component tooltip --format text --page graphs` | 1.06 s | 15,352 KiB |
| Configured `processes` page | `target/release/plasma-top render --config config/config.toml --component tooltip --format text --page processes` | 1.57 s | 15,028 KiB |

The profiling command reports `load_config() = 0.66 ms`, `discover_hardware() = 12.56 ms`, cold collection `= 40.64 ms`, and warm collection `= 0.58 ms` in the first run. Cold collection was dominated by `disk_smart[nvme0n1] = 25.02 ms`, `disk_smart[nvme1n1] = 8.85 ms`, `hd_temp[nvme0n1] = 5.05 ms`, and `net_info = 0.95 ms`.

The `render` command intentionally performs its existing one-second warm-up sleep; the `processes` case also performs its existing 500 ms page warm-up. Those wall times are command baselines, not first-paint latency measurements. The current daemon's first paint is a separate synchronous path and writes panel and tooltip before entering its polling loop.

Unavailable-service behavior was reproducible through the real probe and the existing page fixture coverage: missing hardware and missing files produce absent readings, and a missing page executable renders `ss: not found` without invoking the command. A real unavailable-service run was not forced on the host because changing system services would invalidate the host baseline.

Timeout behavior was reproducible in the new focused adapter characterization test, `command_runner_returns_timeout_without_waiting_for_child_exit`, which runs `/bin/sh -c 'sleep 1'` with a 10 ms deadline and asserts the typed error. Existing page coverage also asserts that page commands use their five-second command timeout and preserve `ss: timed out` on adapter failure. No production service was deliberately stalled during baseline capture.

## Characterization coverage

Existing focused tests cover the contracts needed before scheduler extraction:

| Contract | Existing characterization |
| --- | --- |
| Counter first sample, delta ordering, reset, and history cadence | `src/sensors/cpu.rs`: `read_cpu_usage_first_sample_is_zero_and_seeds_history`, `read_cpu_usage_computes_delta_caps_at_ninety_nine_and_trims_history`, `read_cpu_usage_skips_history_until_interval_elapses_and_handles_reset`; `src/sensors/network.rs`: `read_net_speed_resets_on_counter_rollback_and_zero_dt` |
| Retry and cache failure behavior | `src/sensors/process.rs`: `read_top_process_cached_retries_immediately_after_none_cache`; `src/sensors/gpu_nvidia.rs`: `nvml_read_failure_falls_back_but_retries_nvml_next_poll`; `src/sensors/power.rs`: `read_disk_smart_cached_caches_failure_until_ttl_expires` |
| Page registry, page order, page wake, and isolated state | `src/page_commands.rs`: `build_pages_keeps_full_and_skips_unknown_ids`, `registry_matches_python_page_metadata`; `src/daemon.rs`: `isolated_lifecycle_paints_wakes_keeps_last_good_and_cleans_up`; `tests/cli_daemon.rs`: `page_command_only_touches_isolated_state_subtree` |
| First paint and runtime cleanup | `src/daemon.rs`: `isolated_lifecycle_paints_wakes_keeps_last_good_and_cleans_up`; `tests/runtime_atomic.rs` atomic publication tests |
| Reload last-good behavior | `src/daemon.rs`: `isolated_lifecycle_paints_wakes_keeps_last_good_and_cleans_up`; `src/config/mod.rs`: config loading and invalid-section tests |
| Notification ordering, latch, failure, and absence behavior | `src/notify.rs`: `every_notification_type_emits_exact_ordered_payloads`, `facade_failure_is_reported_but_does_not_stop_or_rearm_processing`, `disabled_and_absent_inputs_leave_state_silent_and_unchanged` |
| Page command cache and timeout degradation | `src/page_commands.rs`: `run_command_ttl_cache_skips_second_invocation`, `run_command_refreshes_at_ttl_boundary`, `run_command_surfaces_adapter_failure_with_page_executable`, `run_command_returns_not_found_without_lookup_hit`; `src/adapters.rs`: `command_runner_returns_timeout_without_waiting_for_child_exit` |

No async scheduler, Tokio runtime, owner state, or production orchestration refactoring was started in issue 01.
