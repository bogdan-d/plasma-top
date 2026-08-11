# Rust dependency review

Review every direct dependency here before updating `Cargo.lock`.

| Crate | Version | License | Purpose and footprint |
| --- | --- | --- | --- |
| `nix` | 0.31 | MIT | Safe `flock(2)`, `getuid(2)`, `poll(2)`, and process-group signal wrappers. Pure Rust over `libc`; features limited to `fs`, `poll`, `process`, `signal`, and `user`. Transitive: `bitflags`, `cfg-if`, `libc`; build helper: `cfg_aliases`. |
| `toml` | 1 | MIT OR Apache-2.0 | Production config and test-fixture parsing. Pure Rust. Uses `serde_core`, `serde_spanned`, `toml_datetime`, `toml_parser`, `toml_writer`, `winnow`, and ordered-map support. |
| `serde` | 1 | MIT OR Apache-2.0 | Typed config deserialization with derive support. Pure Rust; adds the `serde_derive` proc macro and reuses `serde_core`. |
| `miniz_oxide` | 0.9 | MIT OR Zlib OR Apache-2.0 | DEFLATE/zlib for graph PNG encoding and focused decoding tests. Pure Rust; transitive dependency: `adler2`. |
| [`nvml-wrapper`](https://github.com/Cldfire/nvml-wrapper) | 0.12 | MIT OR Apache-2.0 | Optional, runtime-loaded NVIDIA NVML adapter. No native build or link step. Uses `libloading`, `nvml-wrapper-sys`, `thiserror`, and proc-macro support. Missing NVML remains non-fatal. |
| [`tokio`](https://github.com/tokio-rs/tokio) | 1 | MIT | Current-thread async shell, bounded channels, timers, child processes and pipe drains, and Unix termination signals. Direct features are exactly `rt`, `sync`, `time`, `process`, `io-util`, and `signal`; PlasmaTop enables no Tokio macros, filesystem, network, or `full` feature. Normal direct transitives are `bytes`, `libc`, `mio`, `pin-project-lite`, `signal-hook-registry`, and `socket2`; zbus's Tokio integration also selects Tokio runtime, filesystem, and networking support internally. |
| [`zbus`](https://github.com/dbus2/zbus) | 5 | MIT | Persistent pure-Rust system/session D-Bus connections for typed UPower, UDisks, logind, and desktop-notification traffic. Defaults are disabled and only `tokio` integration is selected, following the official zbus README. The reviewed normal tree includes async channel/stream support, `rustix`, `serde`, `tracing` as an internal facade, UUID, zbus names/macros, and zvariant; it has no native libdbus dependency. |

PlasmaTop is GPL-2.0-or-later. Dependencies with incompatible licenses, such as GPLv3-only, AGPL, or proprietary terms, are blocked.

## Review checklist

- package identity and repository URL
- SPDX license and GPL-2.0-or-later compatibility
- why the standard library and current dependencies are insufficient
- default features kept or disabled
- native code, build scripts, and proc macros
- transitive footprint from `cargo tree`
- Arch build/runtime impact
- replacement cost and lockfile policy

## Rules

- Keep `Cargo.lock` committed; do not mix dependency additions with bulk upgrades.
- Keep optional native integrations behind Cargo features.
- Production code remains `unsafe`-free. Prefer a reviewed safe wrapper over introducing an unsafe boundary.

The issue-04 lockfile change removes `wait-timeout`, `signal-hook`, and `serde_json` after their last use and adds only the Tokio/zbus resolution shown by `cargo tree -e normal --depth 2`; existing unrelated package versions are unchanged.
