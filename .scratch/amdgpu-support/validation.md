# AMDGPU live validation

## Candidate and host

Evidence collected on 2026-09-05 UTC using candidate `14cbf4e` plus the ticket-05 diagnostic change that prints AMD source paths and all seven readings in `probe`. The comparison baseline is unchanged commit `3814f0e`, immediately before the AMDGPU plan and implementation. Both release binaries use Rust 1.97.1 and `--locked --features nvml`. Each uses its own revision's shipped `config/config.toml` and sibling `machines.toml`, with 1.5-second poll/history intervals and graphs configured. The machine file has no overrides. AMD temperature notification remains disabled.

The host is Radeon 8060S Graphics, GFX1151/Strix Halo, PCI `0000:c3:00.0`, vendor `1002`, device `1586`, display class `038000`, bound to `amdgpu`. Kernel is `7.2.1-ogc3.1.fc44.x86_64`; the module file is `/lib/modules/7.2.1-ogc3.1.fc44.x86_64/kernel/drivers/gpu/drm/amd/amdgpu/amdgpu.ko`, with matching vermagic and no reported srcversion. Plasma is `plasma-workspace-6.7.4-1.fc44.x86_64` in a KDE Wayland session. VA-API reports Mesa 26.2.1, DRM 3.64, and the same Strix Halo device.

DRM resolves through `/sys/class/drm/card1/device` to `/sys/devices/pci0000:00/0000:00:08.1/0000:c3:00.0`; AMD hwmon is `hwmon6` for this boot. Every input below is mode `0444` and readable by the ordinary user. These indexes describe this run, not discovery assumptions.

| Metric | Relative source | Observed source and conversion |
| --- | --- | --- |
| Device usage | `gpu_busy_percent` | Integer percentage, 2–3% during idle sampling |
| Codec usage | `vcn_busy_percent` | 0% idle; 28–44% during the bounded encoder workload |
| VRAM | `mem_info_vram_used`, `mem_info_vram_total` | Stable idle pair 399,712,256 / 536,870,912 bytes; 381 / 512 MiB after integer conversion |
| Graphics clock | `hwmon/hwmon6/freq1_input`, label `sclk` | Example 818,000,000 Hz → 818 MHz |
| Temperature | `hwmon/hwmon6/temp1_input`, label `edge` | Example 46,000 millidegrees → 46°C |
| Average power | `hwmon/hwmon6/power1_average` | Example 17,385,000 microwatts → 17 W |
| Fan | `hwmon/hwmon6/fan1_input` | Absent; probe returns `None`, tooltip omits it |

The VRAM total is the driver's 512 MiB allocation domain. It is not the APU's physical memory or all GPU-accessible memory. No GTT value is added.

## Direct readings and bounded workloads

Three production `probe` invocations per scenario were bracketed by direct sysfs reads. Each probe performs a cold collection, a one-second warm-up, and a warm collection. The slow temperature/power values may therefore precede the final bracket by about a second. Live clocks, power, allocation, and utilization vary between reads; these are adjacent observations, not an atomic snapshot or an exact-equality claim. Unit conversions and invalid-input boundaries also pass the existing fixture tests.

| Scenario | Probe device usage | Probe codec usage | Probe MHz | Probe °C | Probe W |
| --- | --- | --- | --- | --- | --- |
| Idle | 3, 3, 3 | 0, 0, 0 | 809, 838, 828 | 45, 45, 45 | 10, 12, 12 |
| Vulkan blur | 8, 9, 9 | 0, 0, 0 | 1386, 1281, 1397 | 46, 46, 46 | 16, 19, 17 |
| VA-API H.264 encoding | 4, 4, 4 | 28, 40, 43 | 1001, 1035, 1117 | 45, 46, 46 | 12, 14, 14 |

The graphics workload's corresponding sysfs usage was 6–9%, with codec remaining zero. Encoding sysfs codec rose from zero to 44%. Idle VRAM matched exactly in the stable bracket, and the encoder's final probe pair, 464,101,376 / 536,870,912 bytes, matched its final direct read. Both workloads used installed FFmpeg, 1080p at 60 fps, real-time input pacing, and 12 seconds of video. Both exited successfully in about 11.7 seconds. No host DPM, fan, or power-control file was modified.

```bash
ffmpeg -hide_banner -loglevel warning -init_hw_device vulkan=gpu:0 -filter_hw_device gpu -re -f lavfi -i testsrc2=size=1920x1080:rate=60 -vf format=yuv420p,hwupload,gblur_vulkan=sigma=8 -t 12 -f null -
ffmpeg -hide_banner -loglevel warning -vaapi_device /dev/dri/renderD128 -re -f lavfi -i testsrc2=size=1920x1080:rate=60 -vf format=nv12,hwupload -c:v h264_vaapi -t 12 -f null -
PLASMA_TOP_CODE_ROOT="$PWD" target/release/plasma-top probe --config config/config.toml
```

One corroborating snapshot used `timeout 10s amdgpu_top --pci 0000:c3:00.0 --single --no-pc -J -n 1`. Version 0.11.0 identified the same GPU and PCI node, 512 MiB VRAM total, 382 MiB used at its earlier capture, 46°C edge, 11 W average power, and no fan. Its `--no-pc` option avoided performance-counter polling. Direct documented sysfs inputs remain authoritative; the tool's other counters and binary `gpu_metrics` output are not PlasmaTop sources or dependencies.

## Release comparison method

The baseline and candidate were measured sequentially, with no compilation or validation GPU workload running alongside them. For each binary, run ordinary one-shot profiling, hidden for 65 seconds, main tooltip for 30 seconds, graphs for 30 seconds, then a separate 10-second main run with `--stimuli`. Steady scenarios do not include stimuli. The 65-second window spans the 60-second inventory budget. The desktop remained active, so these single runs establish observed behavior, not statistical performance or power guarantees.

```bash
PLASMA_TOP_CODE_ROOT="$PWD" target/release/plasma-top profiling --config config/config.toml
PLASMA_TOP_CODE_ROOT="$PWD" target/release/plasma-top profiling --config config/config.toml --duration 65 --scenario hidden
PLASMA_TOP_CODE_ROOT="$PWD" target/release/plasma-top profiling --config config/config.toml --duration 30 --scenario main
PLASMA_TOP_CODE_ROOT="$PWD" target/release/plasma-top profiling --config config/config.toml --duration 30 --scenario graphs
PLASMA_TOP_CODE_ROOT="$PWD" target/release/plasma-top profiling --config config/config.toml --duration 10 --scenario main --stimuli
```

To reproduce the baseline, extract `git archive 3814f0e` into an isolated checkout, build it with `cargo build --release --locked --features nvml`, and run the same commands there. Use a separate target directory or verify the binary hashes when reusing build output. The measured baseline binary SHA-256 is `29f8a516e8218cd39d97c9bd30ae7f3c3f3c3331e7274c67413012b74aedaaae`; the candidate is `73ac9f581b932a08ac50566c441c2ce34c1c19188bbaee42d8190866b2a27412`. Candidate config SHA-256 is `db4b68ab1b5740052ac939f120be47aabe4b47f5bba95ea72f96d49e5dfd50f7`; baseline config is `50260ac6747364493d1588250134bfda2fce9420ec6cc8a8994ba610efca6718`.

One-shot discovery measured 6.50 ms baseline and 7.01 ms candidate. Cold collection measured 39.50 / 40.22 ms; warm collection 0.18 / 0.63 ms. These individual samples are not a regression threshold.

## Release comparison results

Times below are milliseconds. Each cell lists baseline / candidate. Timed publication ends after in-memory HTML rendering; file-write timing is deliberately unavailable in profiling mode.

| Scenario | Process calls | Wake lateness p95 | First paint | Publication lateness p95 |
| --- | --- | --- | --- | --- |
| Hidden, 65 s | 2 / 2 | 1.341 / 1.971 | 9.020 / 5.750 | 1.535 / 2.467 |
| Main, 30 s | 4 / 4 | 2.184 / 2.252 | 104.458 / 105.991 | 2.988 / 2.660 |
| Graphs, 30 s | 4 / 4 | 1.968 / 1.997 | 105.647 / 105.636 | 2.567 / 2.803 |

All six steady runs reported zero skipped display deadlines, zero first paints over 250 ms, zero publications over 50 ms, and clean shutdown below 7 ms. The candidate introduced no subprocess count increase. The AMD discovery/attempt code uses direct filesystem reads and contains no subprocess boundary. D-Bus calls were 22 / 18 hidden and 11 / 11 for both presented scenarios; those host-service counts are not AMD acquisition.

| Candidate scenario | AMD fast captured | AMD slow captured / cancelled | AMD inventory captured | GPU history captured |
| --- | --- | --- | --- | --- |
| Hidden, 65 s | 44 | 0 / 0 | 2 | 43 |
| Main, 30 s | 22 | 1 / 1 | 1 | 19 |
| Graphs, 30 s | 22 | 1 / 1 | 1 | 19 |

Hidden graph demand reads usage and codec without a slow AMD job. Presented AMD fields add the slow job, with one successful capture in these 30-second windows. Every AMD fast attempt succeeded. Main/graphs startup also recorded a cancelled slow attempt; it is not counted as a successful sample. The separate config-stimulus run captured two slow samples and cancelled one attempt.

AMDGPU discovery took 0.151 ms at p95 in the hidden run. Fast AMD reads took 0.910 ms at p50, 12.911 ms at p95, and 29.465 ms at p99 there; sysfs latency is variable even without subprocesses. Graph page work grew from 1.885 / 2.624 ms at p50/p95 on the baseline to 2.748 / 3.185 ms on the candidate, which draws the additional GPU chart. Both graph runs recorded 22 accepted page renders, 21 rejected attempts, and one cancelled attempt. Thus rejection counts did not increase in this comparison.

Both separate stimulus runs completed all six actions without timeout. Candidate page, control, and config publication p95 were 52.058, 51.959, and 55.928 ms, with no sample over 100 ms. These stimuli toggle presentation/page and overlay config; they do not prove that newly demanded hardware can be discovered and published within 100 ms.

## Lifecycle and capability checks

A separate production daemon used a fresh `/tmp/plasma-amd05-runtime-*` root with disposable XDG runtime, config, cache, and data paths. The user's installed daemon, widget, and configuration were not changed. The check copied shipped config, presented instance `50505`, removed all AMD tooltip items and the graphs page through atomic config replacement, restored the config, selected graphs, dismissed presentation, sent SIGTERM, and restarted the daemon against the same disposable runtime.

| Check | Observed completion |
| --- | --- |
| Initial panel | 25 ms |
| AMD tooltip after presentation | 50 ms |
| AMD rows removed after demand removal | 76 ms |
| AMD rows restored after demand restoration | 1556 ms |
| AMD graph HTML after page command | 76 ms |
| Tooltip publication after dismissal | No mtime change across 2 seconds |
| SIGTERM shutdown | 7 ms, exit 0 |
| AMD tooltip after restart | 76 ms |

These are file observations polled every 25 ms, not scheduler-instrumented latency. In particular, restoring removed hardware demand took about one poll and is not evidence of a sub-100 ms capability activation guarantee. The runtime root contained only `panel.html`, `tooltip.html`, and `state/`; no unsupported AMD fan row appeared.

Capability disappearance, recovery, invalid values, partial failures, PCI replacement, and incomplete discovery were exercised through the existing AMD fixture tests in the full suite. Relevant cases include `optional_hwmon_and_vram_capabilities_are_independent`, `attempts_retain_failed_fields_update_siblings_and_respect_demand`, `source_reconciliation_invalidates_only_affected_samples`, and `local_discovery_merges_amd_success_failure_and_confirmed_removal`. No physical removal or writable sysfs mutation was attempted.

## Evidence files

Raw reports, source samples, workload commands and exit statuses, probe output, and rendered artifacts are retained locally under `.test-artifacts/amdgpu-validation/`. They are ignored development artifacts. This report retains the relevant measurements without committing process lists or unrelated device identifiers.

## Gates and visual review

All commands in `docs/DEVELOPMENT.md` passed: locked fetch with unchanged lockfile, fmt, all-target/all-feature check and Clippy with warnings denied, 880 tests, docs, repository gate, shell syntax, disposable user installation, and package-layout/upgrade/uninstall checks. No shell script changed. The checkout command `./plasma-top profiling --config config/config.toml` also completed; release measurements above use the release binary directly.

`tools/qt_render_matrix.sh --no-build` passed all 24 dark/light/overlay outputs. The contact sheet and full-size AMD tooltip captures were inspected. Usage, codec, VRAM pair and threshold color, MHz, Celsius, and watts render without overlap; absent fan remains omitted. The graphs page shows green GPU usage and orange AMD codec labels. The matrix's `qt_shot.py` hardcodes white ambient text, so plain unclassified values in its light capture are white. Supplemental panel/tooltip/graphs captures wrapped the disposable light HTML in `<div style="color:#232629">` to emulate the applet's theme text color. Those captures were inspected separately; this does not change product HTML or CSS or prove live theme switching. Ticket 04's nonzero synthetic graph captures remain complementary history-line evidence.

`tools/qml_verify.sh --smoke --no-build` passed in a disposable application-form applet: hidden reads were gated, stale presentation leases expired, and the destruction dismiss callback was present. It did not modify the installed widget or validate compact-panel interaction.

## Installed-widget evidence

On 2026-09-05, Bogdan reported running `./install.sh` and supplied `Screenshot_20260905_043516.png` from the installed widget. The screenshot was visually reviewed. Its dark tooltip shows AMD usage 2%, codec 0%, VRAM 441 / 512 MiB with 86% in the critical color, graphics clock 767 MHz, edge temperature 46°C, and average power 15 W. All six supported rows fit without visible overlap or clipping, and no AMD fan row appears. The VRAM percentage agrees with the displayed allocation pair after integer rounding. This is live installed-tooltip evidence, supplementing the direct-source comparisons above; the screenshot itself is not a synchronized sysfs comparison.

Installation is complete by user report. In response to the follow-up about hover/pinning, wheel paging to AMD graphs, and light/dark theme switching, Bogdan confirmed that everything seems to work fine. This records user acceptance of the installed widget, alongside the screenshot and automated evidence. It is not a separately instrumented interaction or alternate-panel-orientation measurement. Ticket 05 is complete. No legacy `radeon`, AMD discrete GPU, AMD fan hardware, suspend/resume, physical hotplug, live notification delivery, or plasmashell CPU/power claim is made.


## Final review corrections

A final subagent review on 2026-09-05 reproduced two gaps beyond the original fixture coverage. A healthy platform DRM device without PCI attributes could abort AMD discovery, and a single capability change could invalidate every retained field in its scheduler group. Discovery now excludes confirmed unrelated drivers before requiring PCI attributes, while incomplete AMD inspection still fails reconciliation. Scheduler invalidation carries the changed AMD metric set through serial and async execution, preserving sibling readings and capture times. Queued partial invalidations accumulate; whole-job removal and device replacement still clear the complete job. The existing job-replacement method moved into the scheduler's demand module to keep the parent below its size warning.

The follow-up review also identified a related codec-history leak after source replacement. AMD history now uses the sensor owner's retained, deadline-eligible codec outcome without reviving a separate stale codec value. Already collected graph points and unaffected usage remain intact. NVIDIA and Intel retain their existing decoder behavior.

Four new tests cover platform DRM coexistence, scheduler-driven fast/slow capability removal during sibling read failures, coalesced invalidations, and codec-source replacement/failure/recovery through async workers. Existing async history coverage now verifies that partial invalidation preserves both retained deadlines and sibling codec values. The synthetic AMD history test models failed-read retention with the real owner's `Value` outcome; the worker test separately verifies that a transient codec read failure retains its valid value.

Validation passed after these corrections with Rust 1.97.1: locked fetch with unchanged lockfile, fmt, all-target/all-feature check and Clippy with warnings denied, 884 tests comprising 853 unit and 31 integration tests, rustdoc, repository gate, shell syntax, disposable package-layout/upgrade/uninstall checks, and disposable user-install checks. The live measurements and installed-widget evidence above describe the earlier candidate; these corrections were validated with fixtures and repository gates.
