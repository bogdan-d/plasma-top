# Rust dependency review

Review every direct dependency here before updating `Cargo.lock`.

| Crate | Version | License | Purpose and footprint |
|---|---|---|---|
| `nix` | 0.29 | MIT | Safe `flock(2)`, `getuid(2)`, and `poll(2)` wrappers. Pure Rust over `libc`; features limited to `fs`, `poll`, `process`, and `user`. Transitive: `bitflags`, `cfg-if`, `libc`; build helper: `cfg_aliases`. |
| `toml` | 1 | MIT OR Apache-2.0 | Production config and test-fixture parsing. Pure Rust. Uses `serde_core`, `serde_spanned`, `toml_datetime`, `toml_parser`, `toml_writer`, `winnow`, and ordered-map support. |
| `serde` | 1 | MIT OR Apache-2.0 | Typed config deserialization with derive support. Pure Rust; adds the `serde_derive` proc macro and reuses `serde_core`. |
| `miniz_oxide` | 0.8 | MIT OR Zlib OR Apache-2.0 | DEFLATE/zlib for graph PNG encoding and focused decoding tests. Pure Rust; transitive dependency: `adler2`. |
| [`nvml-wrapper`](https://github.com/Cldfire/nvml-wrapper) | 0.11 | MIT | Optional, runtime-loaded NVIDIA NVML adapter. No native build or link step. Uses `libloading`, `nvml-wrapper-sys`, `thiserror`, and proc-macro support. Missing NVML remains non-fatal. |
| [`wait-timeout`](https://github.com/alexcrichton/wait-timeout) | 0.2 | MIT OR Apache-2.0 | Timeout-bound child-process waits unavailable in `std::process`. No transitive dependencies. |
| [`signal-hook`](https://github.com/vorner/signal-hook) | 0.3 | MIT OR Apache-2.0 | Safe SIGINT/SIGTERM flag registration. Uses `signal-hook-registry` and `libc`; no native build step. |
| [`serde_json`](https://github.com/serde-rs/json) | 1 | MIT OR Apache-2.0 | Decode `busctl --json=short` replies without native D-Bus bindings. Pure Rust; uses `itoa`, `memchr`, and `zmij`. |

PiroStats is GPL-2.0-or-later. Dependencies with incompatible licenses, such as
GPLv3-only, AGPL, or proprietary terms, are blocked.

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
- Production code remains `unsafe`-free. Prefer a reviewed safe wrapper over
  introducing an unsafe boundary.
