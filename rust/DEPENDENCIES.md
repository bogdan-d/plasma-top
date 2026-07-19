# Rust dependency review

This file is the per-dependency review ledger required by `plans/PHASES.md`
step P1.4 and the policy in `plans/ARCHITECTURE.md` ("Dependency policy").

Every crate pulled into `Cargo.toml`'s `[dependencies]` (or any future
`[dev-dependencies]` / `[build-dependencies]`) gets one row here **before** the
`Cargo.lock` update is merged. Lane owners fill a row in their handoff; the
integration owner verifies it before accepting the lane.

## Baseline (Phase 1)

| Crate | Version | License | Purpose | Native/build | Transitive | Reviewed by |
|---|---|---|---|---|---|---|
| _(none)_ | — | — | Phase 1 scaffold is std-only by design | none | none | SCAFFOLD lane |

## Phase 2

| Crate | Version | License | Purpose | Native/build | Transitive | Reviewed by |
|---|---|---|---|---|---|---|
| `nix` | 0.29 | MIT | Safe wrappers for `flock(2)` (page-counter serialization in `runtime::page`) and `getuid(2)` (runtime dir fallback in `runtime::mod`). Chosen over `libc` + scoped `unsafe` so the crate-level `#![deny(unsafe_code)]` stays in effect everywhere; `nix` wraps the syscall boundaries internally. Features kept: `fs` (`Flock`), `user` (`getuid`), `process` (unused now, kept for Wave 5 daemon `getpid`-style needs — trim opportunity if Wave 5 does not need it). | none (pure-Rust over `libc`); `cfg_aliases` is a build-time helper with no codegen | runtime: `bitflags`, `cfg-if`, `libc`; build: `cfg_aliases` | RUNTIME lane |
| `toml` | 1 | MIT OR Apache-2.0 | Parse shared oracle TOML fixtures in `test_support::fixture_loader` (FIXTURES lane) and parse `config.toml`/`machines.toml`/`icons.toml`/`lang/*.toml` in production `config` (CONFIG lane). Hard dep because CONFIG needs it in production; the `test-support` feature gates the test-support module, not the parser. FIXTURES originally proposed `toml = "0.8"` optional/feature-gated; unified to `1` (hard) so both lanes share one version. `toml_edit` rejected (preserves formatting we don't need for read-only config). Default features kept. | none (pure-Rust; no native code, no build.rs) | runtime: `serde_core`, `serde_spanned`, `toml_datetime`, `toml_parser` → `winnow`, `toml_writer`; plus `indexmap`/`equivalent`/`hashbrown` via `toml::Table`'s ordered map | FIXTURES lane (row updated by integration owner when CONFIG lane lands in the same wave) |
| `serde` | 1 | MIT OR Apache-2.0 | Derive-based `Deserialize` for the typed `Config` tree (CONFIG lane) and the `Mounts` enum (`list[str] \| str`). Each leaf struct uses `#[serde(default)]` at the container level so missing fields fall back to the struct's `Default` impl — mirrors Python's `_from_dict` (ignore unknown keys, fall back to dataclass defaults). `serde_derive` proc-macro was already vendored transitively via `toml`'s `serde_core`; this hard dep adds only the `serde` facade (re-exports `serde_core` + wires `serde_derive`). | none (pure-Rust; `serde_derive` proc-macro already vendored) | reuses `serde_core`, `serde_derive` already in the lock — **zero new transitive crates** | CONFIG lane |

## Phase 4

| Crate | Version | License | Purpose | Native/build | Transitive | Reviewed by |
|---|---|---|---|---|---|---|
| `miniz_oxide` | 0.8.9 | MIT OR Zlib OR Apache-2.0 | Pure-Rust DEFLATE/zlib used by `rust/src/render/chart.rs` to encode the graphs page's PNG images and decode them in focused round-trip tests. Chosen over the `png` crate because PiroStats already rasterizes pixels itself and needs only compression/decompression, and over `flate2`/system zlib to avoid native build/runtime dependencies and backend feature complexity. | none (pure-Rust; no native code, no build.rs) | runtime: `adler2` | CHART lane |

The crate declares `license = "GPL-2.0-or-later"` to match the project's
existing `LICENSE` / `NOTICE` / packaging metadata. Adding any dependency with
an incompatible license (e.g. GPLv3-only, AGPL, proprietary) is a blocker.

## Required fields per new entry

Mirror the policy checklist in `plans/ARCHITECTURE.md`:

- package identity + repository URL
- license (SPDX) and GPL-2.0-or-later compatibility note
- why stdlib / current code is insufficient
- default features kept or trimmed, with reasoning
- native code, build scripts, proc-macro usage
- transitive footprint summary (e.g. `cargo tree` result)
- Arch packaging impact (runtime library, header, optional dep)
- replacement cost / lockfile pin policy

## Rules

- `Cargo.lock` stays committed; no bulk upgrades during parity work.
- Optional native crates (NVML, HID) live behind cargo features so the default
  build stays portable.
- Any required `unsafe` lives inside the owning adapter module with a local
  `// SAFETY:` comment and focused tests; this file records the dependency that
  introduces it, not the in-module justification.
