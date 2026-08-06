# Async metric scheduler

Status: ready-for-agent

## Objective

Convert PlasmaTop's daemon orchestration to async while making requested metrics deadline-reliable and power-conscious on low-power Linux hardware. Async is a means, not the acceptance criterion: the result must meet display deadlines and must not cause a meaningful hidden-idle energy regression against the synchronous baseline.

## Current evidence

The current daemon executes discovery, collection, notification, formatting, atomic publication, and 100 ms page checks serially on one thread. On the development host, a 32-thread AMD Ryzen AI MAX+ 395, `./plasma-top profiling --config config/config.toml` measured config loading at 2.24 ms, hardware discovery at 14.87 ms, cold collection at 44.04 ms, and warm collection at 0.68 ms. Cold collection was dominated by SMART reads: 23.46 ms and 11.32 ms for two NVMe devices, followed by disk temperatures at 5.03 ms and 2.35 ms. These numbers are development evidence only, not low-power claims.

The highest-risk stalls are timeout-bound commands and D-Bus calls, broad `/proc` scans, HID/NVML calls, mount `statvfs`, and graph rasterization. CPU/memory arithmetic and warm local pseudo-file reads are not established bottlenecks. Current stateful CPU, network, disk, process, and Intel GPU readers depend on ordered prior counters and must never commit out of order.

## Domain semantics

- A `MetricSample<T>` contains a value and the monotonic instant captured immediately around its source read.
- Domain owners are CPU, memory, network, disk, power, NVIDIA, Intel GPU, process, external-status files, D-Bus services, and page commands. Owners use plain typed job/result enums and loops; there is no actor framework.
- One owner may support multiple job kinds and freshness budgets, but it never overlaps the same mutable state with itself.
- The scheduler owns cadence and failure backoff. Sensor readers perform one real attempt when invoked and do not hide independent TTL scheduling.
- A `DisplaySnapshot` is assembled from latest completed samples at a publication deadline. It replaces `ReadingsSnapshot`; `assembled_at` replaces `collected_at`.
- `HardwareInventory` replaces `HardwareSnapshot` and represents progressively discovered, hotplug-aware current hardware.
- A failed attempt retains the latest successful sample and records failure/attempt time for diagnostics. Confirmed hardware or config removal invalidates related samples.
- Samples remain retained while configured/discoverable after demand deactivation so first presentation can show the last frame immediately.
- Jobs carry a configuration generation and source identity. Obsolete or out-of-order completions never commit.

## Demand and cadence

The hidden demand set contains panel metrics, enabled notification metrics, and all histories required by a configured graphs page. Presenting the tooltip adds main-tooltip metrics; selecting a presented page adds that page's jobs. CPU and memory lose their unconditional special case and run only when demanded.

Initial freshness budgets preserve current behavior before laptop retuning:

| Job family | Initial freshness budget |
| --- | ---: |
| Display-rate local metrics and NVML | `display.poll_interval` |
| Histories | `display.history_interval` |
| NVIDIA command fallback | 3 s |
| Network identity/Wi-Fi | 10 s |
| Panel top processes | 15 s |
| Disk temperature and fan speed | 30 s |
| Intel GPU usage | 30 s |
| System and UPower peripheral batteries | 30 s maximum reconciliation, with signal-triggered refresh |
| Bolt battery | 1 h |
| SMART | Existing per-drive-class configured intervals |
| Peripheral discovery | UPower signals plus reconnect enumeration |
| Missing default route | Existing 60 s retry until a measured native netlink replacement lands |
| System-update and server files | Initial read plus inotify changes |
| Fastfetch page | Existing 30 s budget while presented |
| Connections, process, and graph page work | Display cadence while selected and presented |

Configured graph histories remain warm while hidden. CPU-core history is page-owned and runs only while the CPU-cores page is selected and presented. A history deadline appends the latest available value, including a carried value after failure, but appends nothing before the first sample and never synthesizes catch-up points.

Fast display-rate jobs start 50 ms before each display deadline. Publication does not wait past the deadline; missed job ticks are skipped, no owner overlaps itself, and no burst catch-up runs. Long cadences phase-lock to scheduler deadlines and due events are coalesced so one wake can dispatch multiple jobs.

Failure retries use bounded exponential backoff capped by the normal freshness budget. First-valid counter samples may retry promptly when needed to establish a delta. Signal bursts and repeated due events coalesce by owner and job, with at most one pending follow-up.

## Scheduler and runtime

The pure scheduler accepts typed time, config, inventory, presentation, completion, failure, and lifecycle events and emits start, cancel, publish, notify, and rescan actions. It has no Tokio dependency and is tested without sleeping.

The Tokio shell uses a manually built current-thread runtime. A small bounded event channel carries correctness events; duplicate visibility, file, and config changes coalesce to latest state. Critical task exit terminates the daemon nonzero for systemd restart, while ordinary boundary failures remain typed results and back off.

Known tiny `/proc` and `/sys` reads may run on the event thread only while profiling proves they stay within budget. Broad PID/fd scans, `statvfs`, HID, NVML, graph rasterization, and any measured budget violator use one bounded blocking lane initially. Normal formatting stays inline while measured below budget; expensive page rendering uses the blocking lane.

Shutdown cancels timers and async tasks, kills external process groups, rejects late completions, and waits up to 500 ms. Native blocking work that cannot be safely cancelled is abandoned after the runtime shutdown budget and ends with the process. No lock is held across an await.

## I/O services

The command service accepts typed program/argv/timeout requests through a bounded channel, allows at most two external commands initially, creates a process group per child, drains stdout/stderr concurrently, retains at most 1 MiB combined output, reports truncation, and kills/reaps the process group on timeout or shutdown.

The D-Bus services own persistent system and session zbus connections, allow at most two in-flight calls per bus, serialize reconnect attempts with bounded backoff, and fail waiting requests promptly while disconnected. UPower and UDisks use typed requests/replies instead of flattened string bodies. Desktop notifications use the session bus directly and retain existing urgency, icon, timeout, latch, and failure semantics.

UPower device/property signals trigger coalesced battery refreshes, while a 30 s maximum reconciliation protects against missed signals. logind `PrepareForSleep` pauses dispatch before suspend; resume invalidates counter-rate baselines, retains display values, refreshes volatile inventory/route state, and immediately schedules demanded jobs.

File watching uses nonblocking nix inotify through Tokio `AsyncFd`. Stable parent directories are watched so atomic rename remains visible. Queue overflow, ignored watches, and directory recreation trigger a full state rescan and watch re-arm; required initial watch failure is fatal with an exact path/cause. There is no periodic mtime fallback.

## Startup, publication, and notifications

Startup performs local `/proc` and `/sys` discovery first while D-Bus, route, peripheral, and slow hardware discovery continue concurrently. Panel-demand jobs run immediately; first paint waits until all complete or 200 ms, then publishes available truthful values and must meet a provisional p95 target of 250 ms.

Scheduled publication has a provisional p99 lateness target of 50 ms. Panel and tooltip are built from the same `DisplaySnapshot` and keep the existing two-file atomic rename protocol. Identical bytes skip their write independently. While no tooltip is presented, only `panel.html` is rebuilt and written; the last `tooltip.html` remains for immediate first display.

Tooltip activation shows retained content immediately, starts newly demanded work, and permits one coalesced tooltip-only refresh when initial work completes or the 100 ms activation deadline arrives. Page/control/config events target HTML within 100 ms. Tooltip deactivation has a 1 s grace to avoid hover thrash; valid configured sensor completions remain useful after deactivation, while page-only commands/renders cancel after grace.

Notifications are armed only after first panel publication. Thereafter completed samples are evaluated immediately rather than waiting for display publication, while existing hysteresis/latches prevent duplicates and obsolete generations never notify.

## Presentation protocol

QML reports each presented tooltip with a per-applet lease under `<runtime>/state/presented/`; nothing new persists directly under the watched runtime root. `plasma-top present <instance-id>` creates or refreshes a numeric instance lease, and `plasma-top dismiss <instance-id>` removes it. QML presents immediately on hover, pin, or planar/full representation, refreshes every 30 s while presented, dismisses on close/destruction, and the daemon expires leases after 90 s. Any live lease means a tooltip is presented.

The daemon and QML visibility protocol require a matched package/session restart; no mixed-version readiness fallback is provided. Packaging and upgrade documentation must state this requirement, and live validation must cover login, package upgrade, hover, pin, planar representation, multiple instances, applet removal, plasmashell crash, stale lease expiry, and daemon restart.

## Configuration

Existing valid intervals remain unchanged during architecture migration. Cadence values become validated durations; non-finite values or values below 100 ms fail config loading, preserving last-good reload behavior. Concurrency limits remain conservative internal constants rather than user settings.

After laptop A/B validation, fixed per-job freshness budgets are retuned for low power. Demand scoping and failure backoff remain the only adaptive policy unless measurements justify a broader controller. Native netlink may replace route commands only as a measured follow-up.

## Profiling and acceptance

Existing one-shot `profiling` cold/warm behavior remains. `profiling --duration <seconds> --scenario hidden|main|<page>` adds a scheduler-aware run without writing daemon runtime files; `main` is the default scenario. Reports include per-job attempts/success/failure, process and D-Bus counts, queue and run p50/p95/p99, maximum sample age, missed deadlines, render/write time, and publication lateness. Detailed histograms exist only in profiling mode.

Provisional SLOs are first paint p95 at or below 250 ms, page/click/config HTML p99 at or below 100 ms, scheduled publication p99 no more than 50 ms late, shutdown at or below 500 ms, and no sensor wait on the publication critical path.

Final low-power acceptance compares release builds from a fixed synchronous commit and the async candidate on the laptop with identical config and environment. Scenarios are hidden tooltip, main tooltip, each expensive page, unavailable service, and timeout. Record daemon CPU time, wakeups/context switches, child-process count, RSS, queue/run/publication latency, and available RAPL or battery energy. Async does not ship if it causes a meaningful hidden-idle energy regression or misses deadlines after reasonable tuning; optimization wins over async breadth.

## Implementation sequence

1. [Capture baseline and scheduler contracts](issues/01-baseline-and-contracts.md).
2. [Introduce the new domain model and extract owner state](issues/02-domain-model-and-owner-state.md).
3. [Build the pure scheduler](issues/03-pure-scheduler.md).
4. [Add Tokio, typed command/D-Bus services, and the Rust 1.87 transition](issues/04-async-io-services.md).
5. [Cut daemon orchestration over to async owners](issues/05-async-daemon-cutover.md).
6. [Add inotify and the tooltip presentation protocol](issues/06-event-and-presentation-protocol.md).
7. [Add scheduler profiling and validate on the development host](issues/07-profiling-and-development-validation.md).
8. [Run laptop low-power A/B validation](issues/08-laptop-power-validation.md).
9. [Retune fixed freshness budgets from laptop evidence](issues/09-retune-freshness-budgets.md).
