use super::*;

// ── read_battery_periph ──────────────────────────────────────────────────

#[test]
fn read_battery_periph_returns_reading_on_success() {
    let mut cache = BatteryPeripheralCache::default();
    let mut dbus = FakeDbus::new();
    let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        battery_props_reply(path, &[("Percentage", "75"), ("Model", "MX Master 3S")]),
    );

    let reading =
        read_battery_periph_once(&mut cache, &mut dbus, path, None, clock(0)).expect("present");

    assert_eq!(reading.name, "MX Master 3S");
    assert_eq!(reading.charge_percent, 75);
}

#[test]
fn read_battery_periph_none_when_percentage_zero_or_missing() {
    let mut cache = BatteryPeripheralCache::default();
    let mut dbus = FakeDbus::new();
    let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        battery_props_reply(path, &[("Percentage", "0"), ("Model", "MX Keys")]),
    );

    let reading = read_battery_periph_once(&mut cache, &mut dbus, path, None, clock(0));

    assert!(reading.is_none(), "0% → device disconnected");
    // The name was still cached while we had the props.
    assert_eq!(cache.name, "MX Keys");
}

#[test]
fn read_battery_periph_none_when_upower_unreachable() {
    let mut cache = BatteryPeripheralCache::default();
    let mut dbus = FakeDbus::new();
    let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";

    let reading = read_battery_periph_once(&mut cache, &mut dbus, path, None, clock(0));

    assert!(reading.is_none());
    assert_eq!(cache.failed_at, Some(Duration::ZERO));
}

#[test]
fn read_battery_periph_name_override_wins_over_cached_model() {
    let mut cache = BatteryPeripheralCache::default();
    let mut dbus = FakeDbus::new();
    let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        battery_props_reply(path, &[("Percentage", "60"), ("Model", "Internal Name")]),
    );

    let reading = read_battery_periph_once(&mut cache, &mut dbus, path, Some("Override"), clock(0))
        .expect("present");

    assert_eq!(reading.name, "Override");
}

#[test]
fn retained_peripheral_cache_builds_reading_without_io() {
    let mut cache = BatteryPeripheralCache::default();
    let mut dbus = FakeDbus::new();
    let path = "/org/freedesktop/UPower/devices/battery_hidpp_mouse";
    dbus.enqueue(
        SYSTEM,
        UPOWER_NAME,
        path,
        "org.freedesktop.DBus.Properties",
        "GetAll",
        battery_props_reply(path, &[("Percentage", "80"), ("Model", "Mouse")]),
    );

    let _ = read_battery_periph_once(&mut cache, &mut dbus, path, None, clock(0));
    assert_eq!(dbus.call_trace().len(), 1);

    let reading = battery_periph_from_cache(&cache, None);
    assert_eq!(reading.expect("cached").charge_percent, 80);
    assert_eq!(dbus.call_trace().len(), 1);
}

// ── read_battery_bolt ────────────────────────────────────────────────────

#[test]
fn read_battery_bolt_caches_name_and_level() {
    let mut cache = BatteryPeripheralCache::default();
    let mut bolt = FakeBolt::default();
    bolt.push_ok(
        1,
        true,
        Some(BoltBattery {
            name: String::from("MX Keys S"),
            level: 90,
        }),
    );

    let reading =
        read_battery_bolt_once(&mut cache, &mut bolt, 1, None, clock(0)).expect("present");

    assert_eq!(reading.name, "MX Keys S");
    assert_eq!(reading.charge_percent, 90);
    assert_eq!(bolt.calls(), &[(1, true)]);

    let reading_cached = battery_periph_from_cache(&cache, None).expect("cached");
    assert_eq!(reading_cached.charge_percent, 90);
    assert_eq!(bolt.calls().len(), 1);

    // Next attempt fetches level only because name stays cached.
    bolt.push_ok(
        1,
        false,
        Some(BoltBattery {
            name: String::new(),
            level: 80,
        }),
    );
    let reading2 =
        read_battery_bolt_once(&mut cache, &mut bolt, 1, None, clock(3601)).expect("refreshed");
    assert_eq!(reading2.charge_percent, 80);
    assert_eq!(reading2.name, "MX Keys S");
    assert_eq!(bolt.calls(), &[(1, true), (1, false)]);
}

#[test]
fn read_battery_bolt_none_without_sample_records_attempt() {
    let mut cache = BatteryPeripheralCache::default();
    let mut bolt = FakeBolt::default();
    bolt.push_ok(2, true, None);

    let reading = read_battery_bolt_once(&mut cache, &mut bolt, 2, None, clock(0));
    assert!(reading.is_none());

    let reading2 = battery_periph_from_cache(&cache, None);
    assert!(reading2.is_none());
    assert_eq!(bolt.calls().len(), 1);
}

#[test]
fn read_battery_bolt_unsupported_clears_stale_charge() {
    let mut cache = BatteryPeripheralCache {
        name: "Keyboard".to_owned(),
        charge_percent: Some(80),
        sampled_at: Some(Duration::ZERO),
        ..BatteryPeripheralCache::default()
    };
    let mut bolt = FakeBolt::default();
    bolt.push_ok(2, false, None);

    let refreshed = read_battery_bolt_once(&mut cache, &mut bolt, 2, None, clock(3601));

    assert!(refreshed.is_none());
    assert_eq!(cache.charge_percent, None);
    assert_eq!(cache.sampled_at, None);
    assert_eq!(cache.attempted_at, Some(Duration::from_secs(3601)));
    assert_eq!(bolt.calls().len(), 1);
}

#[test]
fn read_battery_bolt_returns_none_on_hid_failure_without_replacing_capture_timestamp() {
    let mut cache = BatteryPeripheralCache::default();
    let mut bolt = FakeBolt::default();
    bolt.push_err(3, true);

    let reading = read_battery_bolt_once(&mut cache, &mut bolt, 3, None, clock(0));
    assert!(reading.is_none());

    // Capture time is unchanged, while the attempt time still permits an immediate retry.
    bolt.push_ok(
        3,
        true,
        Some(BoltBattery {
            name: String::from("Recovered"),
            level: 50,
        }),
    );
    let reading2 =
        read_battery_bolt_once(&mut cache, &mut bolt, 3, None, clock(1)).expect("retry ok");
    assert_eq!(reading2.charge_percent, 50);
    assert_eq!(reading2.name, "Recovered");
    assert_eq!(cache.failed_at, None);
}

#[test]
fn read_battery_bolt_name_override_suppresses_name_fetch() {
    let mut cache = BatteryPeripheralCache::default();
    let mut bolt = FakeBolt::default();
    // want_name should be false because name_override is provided.
    bolt.push_ok(
        1,
        false,
        Some(BoltBattery {
            name: String::new(),
            level: 70,
        }),
    );

    let reading =
        read_battery_bolt_once(&mut cache, &mut bolt, 1, Some("Custom"), clock(0)).expect("ok");

    assert_eq!(reading.name, "Custom");
    assert_eq!(reading.charge_percent, 70);
    assert_eq!(bolt.calls(), &[(1, false)]);
}

#[test]
fn cached_peripheral_result_keeps_latest_failure_timestamp() {
    let cache = BatteryPeripheralCache {
        name: String::from("Mouse"),
        charge_percent: Some(80),
        sampled_at: Some(Duration::from_secs(1)),
        attempted_at: Some(Duration::from_secs(2)),
        failed_at: Some(Duration::from_secs(2)),
        ..BatteryPeripheralCache::default()
    };

    let result = crate::sensors::cached_peripheral(&cache, None);

    assert_eq!(result.reading.status, crate::sensors::AttemptStatus::Cached);
    assert_eq!(result.reading.failed_at, Some(Duration::from_secs(2)));
    assert_eq!(
        result.reading.sample.map(|sample| sample.captured_at),
        Some(Duration::from_secs(1))
    );
}
