use super::*;

pub(super) fn execute_peripheral(
    job: &JobId,
    state: &mut crate::sensors::power::PowerState,
    cfg: &Config,
    ctx: &mut CollectCtx<'_, '_>,
    readings: &mut DisplaySnapshot,
    notifications: &mut DisplaySnapshot,
    timings: &mut Option<&mut Timings>,
) -> CompletionKind {
    let SourceIdentity::Peripheral { role, source } = &job.source else {
        return CompletionKind::ConfirmedAbsent;
    };
    let (cache, name, key) = match role {
        PeripheralRole::Mouse => (
            &mut state.battery_mouse_cache,
            cfg.battery.mouse_name.as_deref(),
            "battery_mouse",
        ),
        PeripheralRole::Keyboard => (
            &mut state.battery_kbd_cache,
            cfg.battery.kbd_name.as_deref(),
            "battery_kbd",
        ),
    };
    let result = match source {
        PeripheralSource::Upower(id) => {
            attempt_upower_peripheral(cache, ctx.dbus, id, name, (ctx.clock)(), key, timings)
        }
        PeripheralSource::Bolt(index) => {
            let Some(bolt) = ctx.bolt.as_deref_mut() else {
                return CompletionKind::ConfirmedAbsent;
            };
            attempt_bolt_peripheral(cache, bolt, *index, name, (ctx.clock)(), key, timings)
        }
    };
    let completion = completion(result.reading.status);
    let value = result.reading.sample.map(|sample| sample.value);
    if result.reading.status == AttemptStatus::Captured {
        match role {
            PeripheralRole::Mouse => notifications.battery_mouse.clone_from(&value),
            PeripheralRole::Keyboard => notifications.battery_kbd.clone_from(&value),
        }
    }
    match role {
        PeripheralRole::Mouse => readings.battery_mouse = value,
        PeripheralRole::Keyboard => readings.battery_kbd = value,
    }
    completion
}
