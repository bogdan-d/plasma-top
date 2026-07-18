//! Frozen domain contracts for the Rust migration scaffold.

pub mod boundary;
pub mod form;
pub mod item;
pub mod metric;

pub use boundary::{
    BusKind, ClockSnapshot, CommandOutput, CommandStatus, DaemonStateSnapshot, DbusOutput,
    FilesystemRoots, HardwareSnapshot, ReadingsSnapshot,
};
pub use form::{Form, Shape, Surface, SurfaceSet};
pub use item::{ItemParseError, ItemRendering, ItemToken};
pub use metric::{Capability, Metric, MetricSpec};
