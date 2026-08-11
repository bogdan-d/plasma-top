//! Typed deterministic system-bus fake.

use std::collections::{HashMap, VecDeque};

use crate::domain::boundary::{
    BoundaryError, BusKind, DbusFacade, DbusOutput, DbusRequest, UdisksSmartKind,
};

/// One exact typed D-Bus invocation.
pub type DbusCall = DbusRequest;

/// In-memory typed D-Bus fake keyed by exact requests.
#[derive(Debug, Default)]
pub struct FakeDbus {
    outputs: HashMap<DbusRequest, VecDeque<DbusOutput>>,
    call_trace: Vec<DbusCall>,
}

impl FakeDbus {
    /// Creates an empty fake.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueues a typed reply for an exact typed request.
    pub fn enqueue_request(&mut self, request: DbusRequest, output: DbusOutput) -> &mut Self {
        self.outputs.entry(request).or_default().push_back(output);
        self
    }

    /// Compatibility builder for sensor fixtures that name the expected call metadata.
    pub fn enqueue(
        &mut self,
        _bus: BusKind,
        _service: impl Into<String>,
        path: impl Into<String>,
        iface: impl Into<String>,
        _member: impl Into<String>,
        output: DbusOutput,
    ) -> &mut Self {
        let path = path.into();
        let iface = iface.into();
        let request = match &output {
            DbusOutput::UpowerDevices(_) => DbusRequest::UpowerEnumerate,
            DbusOutput::UpowerDeviceProperties(_) => {
                DbusRequest::UpowerDeviceProperties { object_path: path }
            }
            DbusOutput::UdisksManagedObjects(_) => DbusRequest::UdisksManagedObjects,
            DbusOutput::UdisksSmartUpdated => DbusRequest::UdisksSmartUpdate {
                object_path: path,
                kind: smart_kind(&iface),
                timeout: std::time::Duration::from_secs(15),
            },
            DbusOutput::UdisksNvmeCriticalWarnings(_) => DbusRequest::UdisksSmartProperty {
                object_path: path,
                kind: UdisksSmartKind::Nvme,
            },
            DbusOutput::UdisksAtaFailing(_) => DbusRequest::UdisksSmartProperty {
                object_path: path,
                kind: UdisksSmartKind::Ata,
            },
        };
        self.enqueue_request(request, output)
    }

    /// Peeks the first recorded call.
    #[must_use]
    pub fn next_call(&self) -> Option<&DbusCall> {
        self.call_trace.first()
    }

    /// Returns all calls in dispatch order.
    #[must_use]
    pub fn call_trace(&self) -> &[DbusCall] {
        &self.call_trace
    }
}

fn smart_kind(interface: &str) -> UdisksSmartKind {
    if interface.contains("NVMe") {
        UdisksSmartKind::Nvme
    } else {
        UdisksSmartKind::Ata
    }
}

impl DbusFacade for FakeDbus {
    fn call(&mut self, request: DbusRequest) -> Result<DbusOutput, BoundaryError> {
        self.call_trace.push(request.clone());
        self.outputs
            .get_mut(&request)
            .and_then(VecDeque::pop_front)
            .ok_or_else(|| {
                let (bus, service, path, interface, member) = request.metadata();
                BoundaryError::DbusCallNotQueued {
                    bus,
                    service: service.to_owned(),
                    path: path.to_owned(),
                    interface: interface.to_owned(),
                    member: member.to_owned(),
                }
            })
    }
}

#[cfg(test)]
mod tests;
