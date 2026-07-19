//! Linux sensor collection building blocks.
//!
//! Wave 3 lands the sensor families incrementally. Each submodule owns one
//! hardware domain and exposes deterministic, fixture-friendly readers that
//! take explicit proc/sys roots and clock snapshots.

pub mod cpu;
pub mod disk;
pub mod gpu_intel;
pub mod gpu_nvidia;
pub mod hid;
pub mod hwmon;
pub mod memory;
pub mod network;
pub mod power;
pub mod process;
