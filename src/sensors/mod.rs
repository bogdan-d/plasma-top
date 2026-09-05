//! Linux sensor collection building blocks.
//!
//! Each submodule owns one hardware domain and exposes deterministic, fixture-friendly readers that take explicit proc/sys roots and clock snapshots. The pure scheduler selects work; the production executor runs emitted jobs serially through cadence-free one-attempt functions.
//!
//! ## State ownership
//!
//! Domain-specific state structs ([`cpu::CpuState`], [`memory::MemoryState`], [`network::NetworkState`], [`disk::DiskState`], [`process::ProcessState`], [`gpu_intel::IntelGpuState`], [`power::PowerState`], [`gpu_nvidia::NvidiaState`], [`gpu_history::GpuHistoryState`], and [`external::ExternalState`]) own mutable metric-sample and attempt state. Daemon and diagnostic composition store these owners separately and construct [`OwnerRefs`] only for one serial job. Source reconciliation and invalidation remain encapsulated by the matching owner.

pub mod cpu;
pub mod disk;
pub mod external;
pub mod gpu_amd;
pub mod gpu_history;
pub mod gpu_intel;
pub mod gpu_nvidia;
pub mod hid;
pub mod hwmon;
pub mod memory;
pub mod network;
pub mod power;
pub mod process;

mod attempts;
mod catalog;
#[cfg(test)]
#[path = "tests/legacy_collect.rs"]
mod collect;
mod coordinator;
mod discovery;
mod scheduled;

pub use attempts::*;
pub(crate) use catalog::*;
#[cfg(test)]
pub use collect::*;
pub use coordinator::*;
pub use discovery::*;
pub(crate) use scheduled::*;

use std::time::Duration;

/// `ip`/`iw` subprocess timeout — mirrors `src/sensors.py`'s `timeout=3`.
pub const NETWORK_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

#[cfg(all(test, feature = "test-support"))]
mod tests;
