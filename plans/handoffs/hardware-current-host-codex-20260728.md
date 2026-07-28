# HARDWARE current-host handoff — Codex — 2026-07-28

## Scope and safety

- Phase 7 current-host subset: P7.1, P7.2, P7.4, and P7.5.
- Base before upstream integration: `d6c00df` on
  `rust-migration-base-bootstrap`.
- `origin/main` commits `284f3b6`, `5d1519c`, and `8f4303e` were merged as
  `2e18dad`. The Ruff and QML changes are shared directly. Rust notification
  defaults and their explicit all-enabled test setup were ported separately.
- No package installation, sudo, system package path, production runtime,
  production config/cache, or global Plasma operation was used.
- All writable `HOME`, `XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, `XDG_DATA_HOME`,
  `XDG_RUNTIME_DIR`, configs, styles, and runtime files lived below
  `/tmp/pirostats-phase7`. Cargo used `/tmp/pirostats-phase7/cargo-home` and
  `rust/target`.
- Raw host evidence is ignored under `.test-artifacts/p7-current-host/raw/`.
  It can contain interface, address, process, and hardware identities; none is
  committed. This handoff contains only sanitized aggregates.

## Host and candidate

- AMD Strix Halo integrated GPU, PCI `1002:1586`, `amdgpu`; unsupported as a
  PiroStats GPU metric source.
- Wired route available. No wifi path was exercised.
- Two NVMe temperature sources; no fan, backlight, system battery, supported
  UPower peripheral, Bolt/HID device, Intel GPU, or NVIDIA GPU.
- Candidate built locked, release, with runtime-loaded NVML:

```bash
mkdir -p /tmp/pirostats-phase7/cargo-home
CARGO_HOME=/tmp/pirostats-phase7/cargo-home \
CARGO_TARGET_DIR="$PWD/rust/target" \
  cargo build --manifest-path rust/Cargo.toml --release --locked --features nvml
```

## P7.1 live differential

Five probe pairs were started in parallel against `config/config.toml`. Three
additional pairs used a disposable copy with `top_process` added to
`tooltip.cpumem` so process sampling was actually requested.

```bash
export HOME=/tmp/pirostats-phase7/run/home
export XDG_CONFIG_HOME=/tmp/pirostats-phase7/run/config
export XDG_CACHE_HOME=/tmp/pirostats-phase7/run/cache
export XDG_DATA_HOME=/tmp/pirostats-phase7/run/data
export XDG_RUNTIME_DIR=/tmp/pirostats-phase7/run/runtime
export PIROSTATS_CODE_ROOT="$PWD"

.venv/bin/python ./pirostats probe --config config/config.toml >python-probe.txt &
rust/target/release/pirostats probe --config config/config.toml >rust-probe.txt &
wait
```

Comparison rules:

- Diff-derived CPU, network, and process values use each implementation's
  built-in one-second warm-up. First history sample must be zero.
- CPU usage and process CPU allow 3 percentage points because the windows start
  and stop on different scheduler ticks.
- Temperatures allow 2 C; memory percentages/GiB and disk percentages/GiB must
  match exactly; uptime allows 1 second.
- Network rates allow 10% when nonzero, or 128 B/s absolute near zero.
- Load averages compare after Rust's two-decimal diagnostic formatting.
- Instantaneous CPU frequency is range-checked only. On this 32-core boost-heavy
  host it changed by GHz between adjacent reads; paired percentage tolerance is
  not meaningful.

Results:

- CPU usage matched in four pairs; one differed by 1 point. First CPU history
  was `[0]` in every run.
- CPU temperature matched exactly in all five pairs. Frequency ranges were
  Python 1.97–4.86 GHz and Rust 1.78–4.93 GHz; all values were valid live
  samples. Turbo state matched.
- Memory usage, used/total GiB, memory history, uptime, and disk usage matched
  exactly. Load averages matched at Rust's two-decimal precision.
- Wired interface/IP presence matched. Nonzero network-rate differences were
  at most 7.8%; zero/near-zero behavior matched.
- Both found the same two NVMe temperature paths and values. Both had no fan,
  backlight, battery, wifi, Intel, NVIDIA, HID, updates-file, or server-status
  reading.
- Process top-three command ordering matched in all three requested pairs;
  corresponding CPU values differed by at most 3 points. Names/PIDs remain only
  in ignored raw evidence.
- Python emitted an empty SMART map because this Python environment lacks the
  optional GI UDisks path. Rust reached UDisks through `busctl`, detected the
  same NVMe drives, and reported unknown health (`None`) after non-fatal SMART
  update failures. This is extra available live coverage, not a health mismatch.
- Diagnostic syntax differs by language (`Some`, struct names, boolean case,
  and sorted-map order). Semantic values above were compared, not bytes.

No unexplained current-host parity difference remains.

## P7.2 isolated soak

A 36.6-second Rust daemon run used copied config/style/lang assets and disabled
all notification categories in that disposable config. Events: first paint,
`page next`, `page next`, `page prev`, dark-style edit, valid overlay config
edit, malformed TOML, five seconds of last-good publication, config recovery,
25 seconds of cache/history advancement, process/runtime inspection, SIGTERM.

Invocation:

```bash
/tmp/pirostats-phase7/run_soak.sh "$PWD"
```

Results:

- First publication observed in 103 ms by the external 100 ms poller; daemon's
  own log reported first paint at 18 ms.
- Page wake samples: 1213, 52, and 1214 ms. Slow samples waited for the 1.5 s
  display poll boundary; all selected the correct page and remained bounded.
- Style reload: 478 ms. Valid config reload: 1491 ms.
- Malformed TOML logged once for that mtime. Panel stayed nonempty and continued
  publishing from last-good config; restored config recovered normally.
- End RSS: 5160 KiB. No child process remained at inspection.
- Runtime root contained only `panel.html`, `tooltip.html`, and state files.
  SIGTERM returned zero and removed panel/tooltip/page/npages. Stable
  `state/page.lock` remained by design; no process leaked.

Suspend/resume, route switching, and disk hotplug were not performed because
this current-host run forbids host/hardware mutation. Existing deterministic
state/cache/interface-switch fixtures remain their evidence.

## P7.4 performance

Method: same host, release Rust, `.venv` Python, identical copied config/assets,
notifications disabled for both, sequential commands, five samples per one-shot
case. Medians below include process startup and each diagnostic's intentional
one-second warm-up (process page includes its extra 500 ms sample). Peak RSS and
children were sampled every 5 ms. Daemons ran 15 seconds, sampled every 20 ms,
then received five alternating page actions and SIGTERM. Raw samples and ranges
are in `performance.json`.

| Case | Python ms | Rust ms | Rust delta |
|---|---:|---:|---:|
| probe | 1099.24 | 1050.13 | -4.5% |
| profiling | 106.31 | 51.13 | -51.9% |
| panel render | 1098.45 | 1064.27 | -3.1% |
| main tooltip render | 1105.76 | 1060.85 | -4.1% |
| processes page | 1615.16 | 1574.27 | -2.5% |
| CPU cores page | 1105.12 | 1063.43 | -3.8% |
| connections page | 1126.13 | 1075.19 | -4.5% |
| fastfetch page | 1125.27 | 1080.04 | -4.0% |
| graphs page | 1113.85 | 1065.05 | -4.4% |

| Daemon metric | Python | Rust |
|---|---:|---:|
| startup to first paint | 87.31 ms | 31.09 ms |
| median RSS | 22248 KiB | 4732 KiB |
| one-core CPU over 15 s | 0.40% | 0.13% |
| median page wake | 101.57 ms | 102.03 ms |
| steady child processes | 0 | 0 |

Profiling internals (five-sample medians): Python/Rust warm collection was
0.63/0.31 ms. With SMART enabled, cold collection was 9.36/33.48 ms and hardware
discovery 4.08/10.80 ms because Rust exercised UDisks while Python GI could not.
With SMART disabled for both, cold collection was 8.80/7.59 ms; discovery was
3.94/5.62 ms. The remaining 1.68 ms discovery difference is Rust's command-backed
UPower probe, also unavailable through this Python environment. End-to-end
startup still favored Rust. Thus both repeatable >10% subphase differences were
investigated and attributed to extra adapter work, not slower equivalent work.

`strace -f -e trace=process` around one probe observed 3 Python versus 7 Rust
child creations. Rust's extra short-lived calls are `busctl` UDisks/UPower work;
neither daemon retained a child. No steady subprocess regression exists.

## P7.5 degradation and bounds

A separate 20-second daemon run used `PATH=/nonexistent`, disposable assets and
runtime, then rendered `connections` and `fastfetch` pages under the same absent
command environment.

```bash
/tmp/pirostats-phase7/run_degrade.sh "$PWD"
```

- First paint succeeded. Missing D-Bus/commands/NVML/battery/HID stayed non-fatal.
- CPU consumed 2 clock ticks over 20 seconds; RSS moved 4100 to 4200 KiB.
- Log stayed at 3 lines/180 bytes. One `notify-send` absence was logged; no retry
  storm occurred.
- Runtime files stayed bounded to panel, tooltip, page, and npages. No child or
  daemon process leaked; SIGTERM returned zero.
- Connections degraded to `no listening sockets`; fastfetch rendered
  `fastfetch: not found`.

## Unavailable hardware gaps and fixture proof

- Intel GPU live path: unavailable. Covered by `sensors::gpu_intel` fixture
  tests for detection, fdinfo parsing, first sample, diffs, cache, reset, and
  absence.
- NVIDIA NVML and `nvidia-smi` fallback live paths: unavailable. Covered by 10
  `sensors::gpu_nvidia` tests plus collector integration tests for success,
  initialization/read failure, malformed/absent fallback, TTL, clamps, and
  histories.
- System/peripheral battery and UPower live paths: unavailable. Covered by
  `sensors::power` tests for enumeration, sysfs/UPower fallback, malformed and
  absent service, caching, and zero/missing values.
- Bolt/HID live path: unavailable. Covered by 16 `sensors::hid` tests plus Bolt
  power tests for discovery, exact packets, timeout, mismatch, short I/O,
  absent features, levels, and cache behavior.
- AMD GPU absence is not Intel/NVIDIA coverage and no AMD support is claimed.

## Exact verification

```bash
cargo fmt --manifest-path rust/Cargo.toml -- --check
cargo check --manifest-path rust/Cargo.toml --all-targets
cargo clippy --manifest-path rust/Cargo.toml --all-targets --all-features -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-targets --all-features
cargo doc --manifest-path rust/Cargo.toml --no-deps
.venv/bin/python -m pytest tests/ -q
.venv/bin/ruff check .
.venv/bin/vulture src/ tests/ pirostats tests/vulture_whitelist.py --min-confidence 60
```

Results: 507 Rust library + 26 integration tests; Python 175 passed + 1 optional
skip; Ruff and Vulture green. One Rust notification test initially exposed an
incomplete upstream-default port; its all-categories corpus now enables the
three quiet-by-default categories explicitly and the aggregate gate is green.

## Gate status

Current-host P7.1/P7.2/P7.4/P7.5 subset passes. Gate P7 remains open for user
acceptance of unavailable Intel/NVIDIA/battery/HID live gaps and for any desired
external-host evidence. Suspend/network-switch/disk-hotplug live events also
remain unexercised under current-host mutation constraints. No new deviation is
requested.
