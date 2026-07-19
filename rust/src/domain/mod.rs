//! Frozen domain contracts for the Rust migration scaffold.

pub mod boundary;
pub mod form;
pub mod item;
pub mod metric;
pub mod registry;

pub use boundary::{
    BusKind, ClockSnapshot, CommandOutput, CommandStatus, DaemonStateSnapshot, DbusOutput,
    FilesystemRoots, HardwareSnapshot, ReadingsSnapshot,
};
pub use form::{Form, Shape, Surface, SurfaceSet};
pub use item::{ItemParseError, ItemRendering, ItemToken};
pub use metric::{Capability, Metric, MetricSpec};
pub use registry::{
    GRAPHS_PAGE_CAPABILITIES, NOTIFY_CAPABILITY_MAP, SEPARATOR_ITEMS, graphs_page_capabilities,
    list_items, misplaced_items, needed_capabilities, notification_capability_map, parse,
    placement_for, unknown_item_names,
};
