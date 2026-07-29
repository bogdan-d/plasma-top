//! Production command, D-Bus, notification, and clock adapters.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;
use wait_timeout::ChildExt;

use crate::domain::boundary::{
    BoundaryError, BusKind, ClockSnapshot, CommandOutput, CommandRunner, CommandStatus,
    DbusArgument, DbusFacade, DbusOutput, DbusRequest, NotificationError, NotificationFacade,
    NotificationPayload,
};

/// Runs production subprocesses without shell expansion.
#[derive(Debug, Default)]
pub struct ProductionCommandRunner;

impl CommandRunner for ProductionCommandRunner {
    fn run(
        &mut self,
        program: &Path,
        args: &[OsString],
        timeout: Duration,
    ) -> Result<CommandOutput, BoundaryError> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| command_error(program, args, error.to_string()))?;
        let status = child
            .wait_timeout(timeout)
            .map_err(|error| command_error(program, args, error.to_string()))?;
        if status.is_none() {
            let _ = child.kill();
            let _ = child.wait();
            return Err(command_error(
                program,
                args,
                format!("timed out after {:.3}s", timeout.as_secs_f64()),
            ));
        }
        let output = child
            .wait_with_output()
            .map_err(|error| command_error(program, args, error.to_string()))?;
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        let status = output.status.code().map_or_else(
            || CommandStatus::Signal(output.status.signal().unwrap_or(0)),
            CommandStatus::Exit,
        );
        Ok(CommandOutput {
            program: program.to_path_buf(),
            args: args.to_vec(),
            status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }
}

fn command_error(program: &Path, args: &[OsString], detail: String) -> BoundaryError {
    BoundaryError::CommandFailed {
        program: program.to_path_buf(),
        args: args.to_vec(),
        detail,
    }
}

/// Blocking production D-Bus facade implemented through systemd's `busctl`.
///
/// `busctl` is already part of the target Linux userspace and avoids a native
/// libdbus runtime dependency. Its JSON reply is normalized into the compact
/// body shapes consumed by the sensor modules.
#[derive(Debug)]
pub struct ProductionDbusFacade {
    runner: ProductionCommandRunner,
    busctl: PathBuf,
}

impl Default for ProductionDbusFacade {
    fn default() -> Self {
        Self {
            runner: ProductionCommandRunner,
            busctl: PathBuf::from("busctl"),
        }
    }
}

impl DbusFacade for ProductionDbusFacade {
    fn call(&mut self, request: DbusRequest) -> Result<DbusOutput, BoundaryError> {
        let mut args = vec![
            OsString::from(match request.bus {
                BusKind::Session => "--user",
                BusKind::System => "--system",
            }),
            OsString::from("--json=short"),
            OsString::from("call"),
            OsString::from(&request.service),
            OsString::from(&request.object_path),
            OsString::from(&request.interface),
            OsString::from(&request.member),
        ];
        append_dbus_arguments(&mut args, &request.arguments);
        let timeout = request.timeout.unwrap_or(Duration::from_secs(5));
        let result = self
            .runner
            .run(&self.busctl, &args, timeout)
            .map_err(|error| dbus_error(&request, error.to_string()))?;
        if result.status != CommandStatus::Exit(0) {
            return Err(dbus_error(
                &request,
                String::from_utf8_lossy(&result.stderr).trim().to_owned(),
            ));
        }
        let value: Value = serde_json::from_slice(&result.stdout)
            .map_err(|error| dbus_error(&request, format!("invalid busctl JSON: {error}")))?;
        let body = normalize_dbus_body(&request, &value);
        Ok(DbusOutput {
            bus: request.bus,
            service: request.service,
            object_path: request.object_path,
            interface: request.interface,
            member: request.member,
            body,
        })
    }
}

fn append_dbus_arguments(args: &mut Vec<OsString>, values: &[DbusArgument]) {
    if values.is_empty() {
        return;
    }
    let mut signature = String::new();
    for value in values {
        match value {
            DbusArgument::String(_) => signature.push('s'),
            DbusArgument::EmptyStringVariantDict => signature.push_str("a{sv}"),
        }
    }
    args.push(OsString::from(signature));
    for value in values {
        match value {
            DbusArgument::String(value) => args.push(OsString::from(value)),
            DbusArgument::EmptyStringVariantDict => args.push(OsString::from("0")),
        }
    }
}

fn dbus_error(request: &DbusRequest, detail: String) -> BoundaryError {
    BoundaryError::DbusCallFailed {
        bus: request.bus,
        service: request.service.clone(),
        path: request.object_path.clone(),
        interface: request.interface.clone(),
        member: request.member.clone(),
        detail,
    }
}

fn data(value: &Value) -> &Value {
    value.get("data").unwrap_or(value)
}

fn scalar(value: &Value) -> String {
    let value = data(value);
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => number.to_string(),
        Value::Array(values) if values.len() == 1 => scalar(&values[0]),
        _ => String::new(),
    }
}

fn normalize_dbus_body(request: &DbusRequest, reply: &Value) -> Vec<String> {
    let value = data(reply);
    if request.member == "GetManagedObjects" {
        return normalize_managed_objects(value);
    }
    if request.member == "GetAll" {
        return normalize_properties(value);
    }
    if request.member == "Get" {
        return vec![scalar(value)];
    }
    let mut out = Vec::new();
    flatten_scalars(value, &mut out);
    out
}

fn flatten_scalars(value: &Value, out: &mut Vec<String>) {
    match data(value) {
        Value::Array(values) => values.iter().for_each(|value| flatten_scalars(value, out)),
        Value::Object(values) => values
            .values()
            .for_each(|value| flatten_scalars(value, out)),
        value => out.push(scalar(value)),
    }
}

fn normalize_properties(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let value = match data(value) {
        Value::Array(values) if values.len() == 1 => data(&values[0]),
        value => value,
    };
    if let Value::Object(properties) = value {
        for (key, value) in properties {
            out.push(key.clone());
            out.push(scalar(value));
        }
    }
    out
}

fn normalize_managed_objects(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let value = match data(value) {
        Value::Array(values) if values.len() == 1 => data(&values[0]),
        value => value,
    };
    let Value::Object(objects) = value else {
        return out;
    };
    for (path, interfaces) in objects {
        out.push(path.clone());
        if let Value::Object(interfaces) = data(interfaces) {
            for (interface, properties) in interfaces {
                out.push(interface.clone());
                if interface == "org.freedesktop.UDisks2.Block"
                    && let Value::Object(properties) = data(properties)
                    && let Some(drive) = properties.get("Drive")
                {
                    out.push(format!("Block.Drive={}", scalar(drive)));
                }
            }
        }
        out.push(String::new());
    }
    out
}

/// Desktop notification facade using the standard `notify-send` client.
#[derive(Debug, Default)]
pub struct ProductionNotificationFacade {
    runner: ProductionCommandRunner,
}

impl NotificationFacade for ProductionNotificationFacade {
    fn send(&mut self, payload: &NotificationPayload) -> Result<(), NotificationError> {
        let args = [
            OsString::from("-u"),
            OsString::from("critical"),
            OsString::from("-t"),
            OsString::from("0"),
            OsString::from("-i"),
            OsString::from(&payload.icon),
            OsString::from(&payload.title),
            OsString::from(&payload.body),
        ];
        let output = self
            .runner
            .run(Path::new("notify-send"), &args, Duration::from_secs(5))
            .map_err(|error| NotificationError {
                detail: error.to_string(),
            })?;
        if output.status == CommandStatus::Exit(0) {
            Ok(())
        } else {
            Err(NotificationError {
                detail: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            })
        }
    }
}

/// Process-lifetime monotonic clock paired with wall time.
#[derive(Debug)]
pub struct ProductionClock {
    origin: Instant,
}

impl Default for ProductionClock {
    fn default() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl ProductionClock {
    /// Samples monotonic and wall clocks once.
    #[must_use]
    pub fn snapshot(&self) -> ClockSnapshot {
        ClockSnapshot {
            monotonic: self.origin.elapsed(),
            wall: SystemTime::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_json_normalizes_to_interleaved_pairs() {
        let value = serde_json::json!({"data": [{
            "Percentage": {"type": "d", "data": 52.5},
            "State": {"type": "u", "data": 2}
        }]});
        assert_eq!(
            normalize_properties(&value),
            vec!["Percentage", "52.5", "State", "2"]
        );
    }

    #[test]
    fn managed_object_json_preserves_drive_relation() {
        let value = serde_json::json!({"data": [{
            "/block": {"data": {
                "org.freedesktop.UDisks2.Block": {"data": {
                    "Drive": {"type": "o", "data": "/drive"}
                }}
            }}
        }]});
        assert_eq!(
            normalize_managed_objects(&value),
            vec![
                "/block",
                "org.freedesktop.UDisks2.Block",
                "Block.Drive=/drive",
                ""
            ]
        );
    }
}
