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
        let body = normalize_dbus_body(&request, &value)?;
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
        Value::Array(_) | Value::Object(_) => value.to_string(),
        Value::Null => String::new(),
    }
}

pub(crate) fn normalize_dbus_body(
    request: &DbusRequest,
    reply: &Value,
) -> Result<Vec<String>, BoundaryError> {
    match request.member.as_str() {
        "EnumerateDevices" => normalize_object_paths(request, reply),
        "GetManagedObjects" => normalize_managed_objects(request, reply),
        "GetAll" => normalize_properties(request, reply),
        "Get" => {
            let value = single_reply_value(request, reply, "v")?;
            let value = variant_value(request, value, "Get result")?;
            Ok(vec![scalar(value)])
        }
        _ => {
            let mut out = Vec::new();
            flatten_scalars(data(reply), &mut out);
            Ok(out)
        }
    }
}

fn normalize_object_paths(
    request: &DbusRequest,
    reply: &Value,
) -> Result<Vec<String>, BoundaryError> {
    let value = single_reply_value(request, reply, "ao")?;
    let Value::Array(paths) = value else {
        return Err(dbus_shape_error(
            request,
            "EnumerateDevices data must contain an object-path array",
        ));
    };
    paths
        .iter()
        .map(|path| {
            path.as_str().map(str::to_owned).ok_or_else(|| {
                dbus_shape_error(
                    request,
                    "EnumerateDevices data must contain only object-path strings",
                )
            })
        })
        .collect()
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

fn normalize_properties(
    request: &DbusRequest,
    reply: &Value,
) -> Result<Vec<String>, BoundaryError> {
    let mut out = Vec::new();
    let value = single_reply_value(request, reply, "a{sv}")?;
    let Value::Object(properties) = value else {
        return Err(dbus_shape_error(
            request,
            "GetAll data must contain a property object",
        ));
    };
    for (key, value) in properties {
        let value = variant_value(request, value, &format!("GetAll property `{key}`"))?;
        out.push(key.clone());
        out.push(scalar(value));
    }
    Ok(out)
}

fn normalize_managed_objects(
    request: &DbusRequest,
    reply: &Value,
) -> Result<Vec<String>, BoundaryError> {
    let mut out = Vec::new();
    let value = single_reply_value(request, reply, "a{oa{sa{sv}}}")?;
    let Value::Object(objects) = value else {
        return Err(dbus_shape_error(
            request,
            "GetManagedObjects data must contain an object map",
        ));
    };
    for (path, interfaces) in objects {
        let Value::Object(interfaces) = interfaces else {
            return Err(dbus_shape_error(
                request,
                &format!("managed object `{path}` must contain an interface map"),
            ));
        };
        out.push(path.clone());
        for (interface, properties) in interfaces {
            let Value::Object(properties) = properties else {
                return Err(dbus_shape_error(
                    request,
                    &format!(
                        "managed interface `{interface}` on `{path}` must contain a property map"
                    ),
                ));
            };
            out.push(interface.clone());
            for (property, value) in properties {
                let value = variant_value(
                    request,
                    value,
                    &format!("managed property `{interface}.{property}` on `{path}`"),
                )?;
                if interface == "org.freedesktop.UDisks2.Block" && property == "Drive" {
                    out.push(format!("Block.Drive={}", scalar(value)));
                }
            }
        }
        out.push(String::new());
    }
    Ok(out)
}

fn single_reply_value<'a>(
    request: &DbusRequest,
    reply: &'a Value,
    expected_signature: &str,
) -> Result<&'a Value, BoundaryError> {
    let signature = reply.get("type").and_then(Value::as_str).ok_or_else(|| {
        dbus_shape_error(
            request,
            "busctl reply must contain a string `type` signature",
        )
    })?;
    if signature != expected_signature {
        return Err(dbus_shape_error(
            request,
            &format!(
                "unexpected busctl reply signature `{signature}`; expected `{expected_signature}`"
            ),
        ));
    }
    let values = reply
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| dbus_shape_error(request, "busctl reply must contain a `data` array"))?;
    let [value] = values.as_slice() else {
        return Err(dbus_shape_error(
            request,
            &format!(
                "busctl reply signature `{expected_signature}` must contain exactly one value"
            ),
        ));
    };
    validate_typed_value(request, expected_signature, value, "busctl reply value")?;
    Ok(value)
}

fn variant_value<'a>(
    request: &DbusRequest,
    value: &'a Value,
    context: &str,
) -> Result<&'a Value, BoundaryError> {
    let Value::Object(variant) = value else {
        return Err(dbus_shape_error(
            request,
            &format!("{context} must be a typed variant object"),
        ));
    };
    let Some(signature) = variant.get("type").and_then(Value::as_str) else {
        return Err(dbus_shape_error(
            request,
            &format!("{context} must contain a string `type` signature"),
        ));
    };
    if signature.is_empty() {
        return Err(dbus_shape_error(
            request,
            &format!("{context} must contain a nonempty string `type` signature"),
        ));
    }
    let value = variant
        .get("data")
        .ok_or_else(|| dbus_shape_error(request, &format!("{context} must contain `data`")))?;
    validate_typed_value(request, signature, value, context)?;
    Ok(value)
}

fn validate_typed_value(
    request: &DbusRequest,
    signature: &str,
    value: &Value,
    context: &str,
) -> Result<(), BoundaryError> {
    let consumed = validate_dbus_type(request, signature, value, context)?;
    if consumed != signature.len() {
        return Err(dbus_shape_error(
            request,
            &format!("{context} has unsupported compound signature `{signature}`"),
        ));
    }
    Ok(())
}

fn validate_dbus_type(
    request: &DbusRequest,
    signature: &str,
    value: &Value,
    context: &str,
) -> Result<usize, BoundaryError> {
    let Some(kind) = signature.as_bytes().first().copied() else {
        return Err(dbus_shape_error(
            request,
            &format!("{context} has an empty value signature"),
        ));
    };
    match kind {
        b's' | b'o' | b'g' if value.is_string() => Ok(1),
        b'b' if value.is_boolean() => Ok(1),
        b'y' if value
            .as_u64()
            .is_some_and(|number| u8::try_from(number).is_ok()) =>
        {
            Ok(1)
        }
        b'n' if value
            .as_i64()
            .is_some_and(|number| i16::try_from(number).is_ok()) =>
        {
            Ok(1)
        }
        b'q' if value
            .as_u64()
            .is_some_and(|number| u16::try_from(number).is_ok()) =>
        {
            Ok(1)
        }
        b'i' if value
            .as_i64()
            .is_some_and(|number| i32::try_from(number).is_ok()) =>
        {
            Ok(1)
        }
        b'u' | b'h'
            if value
                .as_u64()
                .is_some_and(|number| u32::try_from(number).is_ok()) =>
        {
            Ok(1)
        }
        b'x' if value.as_i64().is_some() => Ok(1),
        b't' if value.as_u64().is_some() => Ok(1),
        b'd' if value.is_number() => Ok(1),
        b'v' => {
            variant_value(request, value, context)?;
            Ok(1)
        }
        b'a' => validate_dbus_array(request, signature, value, context),
        b'(' => validate_dbus_struct(request, signature, value, context),
        b'{' | b')' | b'}' => Err(dbus_shape_error(
            request,
            &format!(
                "{context} has misplaced signature delimiter `{}`",
                char::from(kind)
            ),
        )),
        _ => Err(dbus_shape_error(
            request,
            &format!("{context} does not match D-Bus signature `{signature}`"),
        )),
    }
}

fn validate_dbus_array(
    request: &DbusRequest,
    signature: &str,
    value: &Value,
    context: &str,
) -> Result<usize, BoundaryError> {
    let element_signature = signature.get(1..).unwrap_or_default();
    if element_signature.starts_with('{') {
        return validate_dbus_dictionary(request, signature, value, context);
    }
    let element_len = dbus_type_len(request, element_signature, context)?;
    let Value::Array(values) = value else {
        return Err(dbus_shape_error(
            request,
            &format!("{context} must be an array for signature `{signature}`"),
        ));
    };
    for (index, value) in values.iter().enumerate() {
        validate_typed_value(
            request,
            &element_signature[..element_len],
            value,
            &format!("{context} element {index}"),
        )?;
    }
    Ok(1 + element_len)
}

fn validate_dbus_dictionary(
    request: &DbusRequest,
    signature: &str,
    value: &Value,
    context: &str,
) -> Result<usize, BoundaryError> {
    let inner = signature.get(2..).unwrap_or_default();
    let key_len = dbus_type_len(request, inner, context)?;
    let value_signature = inner.get(key_len..).unwrap_or_default();
    let value_len = dbus_type_len(request, value_signature, context)?;
    if value_signature.as_bytes().get(value_len) != Some(&b'}') {
        return Err(dbus_shape_error(
            request,
            &format!("{context} has malformed dictionary signature `{signature}`"),
        ));
    }
    let Value::Object(entries) = value else {
        return Err(dbus_shape_error(
            request,
            &format!("{context} must be an object for signature `{signature}`"),
        ));
    };
    for (key, value) in entries {
        validate_typed_value(
            request,
            &inner[..key_len],
            &Value::String(key.clone()),
            &format!("{context} key `{key}`"),
        )?;
        validate_typed_value(
            request,
            &value_signature[..value_len],
            value,
            &format!("{context} value for `{key}`"),
        )?;
    }
    Ok(3 + key_len + value_len)
}

fn validate_dbus_struct(
    request: &DbusRequest,
    signature: &str,
    value: &Value,
    context: &str,
) -> Result<usize, BoundaryError> {
    let Value::Array(fields) = value else {
        return Err(dbus_shape_error(
            request,
            &format!("{context} must be an array for struct signature `{signature}`"),
        ));
    };
    let mut offset = 1;
    for (index, field) in fields.iter().enumerate() {
        let remaining = signature.get(offset..).unwrap_or_default();
        if remaining.starts_with(')') {
            return Err(dbus_shape_error(
                request,
                &format!("{context} has too many struct fields"),
            ));
        }
        let field_len = validate_dbus_type(
            request,
            remaining,
            field,
            &format!("{context} field {index}"),
        )?;
        offset += field_len;
    }
    if signature.as_bytes().get(offset) != Some(&b')') {
        return Err(dbus_shape_error(
            request,
            &format!("{context} has too few struct fields"),
        ));
    }
    Ok(offset + 1)
}

fn dbus_type_len(
    request: &DbusRequest,
    signature: &str,
    context: &str,
) -> Result<usize, BoundaryError> {
    match signature.as_bytes().first().copied() {
        Some(b'a') => {
            Ok(1 + dbus_type_len(request, signature.get(1..).unwrap_or_default(), context)?)
        }
        Some(b'(') => delimited_type_len(request, signature, b'(', b')', context),
        Some(b'{') => delimited_type_len(request, signature, b'{', b'}', context),
        Some(
            b's' | b'o' | b'g' | b'b' | b'y' | b'n' | b'q' | b'i' | b'u' | b'h' | b'x' | b't'
            | b'd' | b'v',
        ) => Ok(1),
        _ => Err(dbus_shape_error(
            request,
            &format!("{context} has malformed D-Bus signature `{signature}`"),
        )),
    }
}

fn delimited_type_len(
    request: &DbusRequest,
    signature: &str,
    open: u8,
    close: u8,
    context: &str,
) -> Result<usize, BoundaryError> {
    let mut offset = 1;
    while let Some(kind) = signature.as_bytes().get(offset).copied() {
        if kind == close {
            return Ok(offset + 1);
        }
        if kind == open {
            return Err(dbus_shape_error(
                request,
                &format!("{context} has malformed D-Bus signature `{signature}`"),
            ));
        }
        offset += dbus_type_len(
            request,
            signature.get(offset..).unwrap_or_default(),
            context,
        )?;
    }
    Err(dbus_shape_error(
        request,
        &format!("{context} has unterminated D-Bus signature `{signature}`"),
    ))
}

fn dbus_shape_error(request: &DbusRequest, detail: &str) -> BoundaryError {
    dbus_error(request, format!("malformed busctl reply: {detail}"))
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
mod tests;
