//! Frozen domain contracts for the Rust migration scaffold.

pub mod boundary;
pub mod form;
pub mod item;
pub mod metric;
pub mod readings;
pub mod registry;
pub mod state;

pub use boundary::{
    BoundaryError, BusKind, ClockSnapshot, CommandOutput, CommandRunner, CommandStatus,
    DbusArgument, DbusFacade, DbusOutput, DbusRequest, FilesystemRoots,
};
pub use form::{Form, Shape, Surface, SurfaceSet};
pub use item::{ItemParseError, ItemRendering, ItemToken};
pub use metric::{Capability, Metric, MetricSpec};
pub use readings::{
    BatteryPeripheralReading, BatteryState, BatterySystemReading, DiskSmartInterface,
    DiskUsageReading, HardwareSnapshot, LoadAverage, ReadingsSnapshot, SmartDisk,
    TopProcessDetails, TopProcessSummary,
};
pub use registry::{
    GRAPHS_PAGE_CAPABILITIES, NOTIFY_CAPABILITY_MAP, SEPARATOR_ITEMS, graphs_page_capabilities,
    list_items, misplaced_items, needed_capabilities, notification_capability_map, parse,
    placement_for, unknown_item_names,
};
pub use state::{
    BatteryPeripheralCache, BatterySystemCache, CounterRateState, DaemonStateSnapshot, GpuCache,
    NetworkInfoCache, TimedValue,
};
