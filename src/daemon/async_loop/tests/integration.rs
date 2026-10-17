use std::sync::atomic::AtomicBool;

use super::*;

fn realtime_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("runtime")
}

#[test]
fn production_async_loop_publishes_while_real_owner_task_is_blocked() {
    let (root, paths) = test_paths("production-blocked-owner");
    fs::create_dir_all(&root).expect("test root");
    let proc_root = root.join("proc");
    let sys_root = root.join("sys");
    fs::create_dir_all(&proc_root).expect("proc root");
    fs::create_dir_all(&sys_root).expect("sys root");
    let config = root.join("config.toml");
    fs::write(
        &config,
        "[display]\npoll_interval = 0.25\n[panel]\norder = [\"disk\"]\n[panel.disk]\nitems = [\"disk_usage\"]\n[tooltip]\norder = []\n[pages]\norder = []\n",
    )
    .expect("config fixture");
    let roots = FilesystemRoots {
        proc_root,
        sys_root,
        ..FilesystemRoots::default()
    };
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(Semaphore::new(0));
    let mut io = crate::adapters::ProductionIo::start_without_signals().expect("production I/O");
    let sink = RecordingSink::default();
    let (applet_sender, applet_events) = mpsc::channel(1);
    let services = DaemonServices {
        commands: io.commands(),
        dbus: io.dbus(),
        notifications: io.notifications(),
        stopped: io.stopped(),
        events: io.take_event_receiver().expect("I/O events"),
        clock: ProductionClock::default(),
        shutdown: io.shutdown_receiver(),
        applet_events,
        sink: Box::new(sink.clone()),
        blocked_owner: Some(worker::TestOwnerBlock {
            owner: OwnerId::Disk,
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        }),
    };
    let stopped = Arc::clone(&services.stopped);
    let first_paint_started = Instant::now();

    let shutdown_started = realtime_runtime().block_on(async move {
        let daemon = tokio::spawn(async move {
            let _applet_sender = applet_sender;
            run(Some(&config), &roots, &paths, services, None).await
        });
        tokio::time::timeout(Duration::from_secs(1), async {
            while sink.last_panel().is_none() || !entered.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("panel publication while owner blocked");
        assert!(
            sink.last_panel()
                .is_some_and(|html| html.contains("class=\"panel"))
        );
        let first_paint_elapsed = first_paint_started.elapsed();
        assert!(first_paint_elapsed >= Duration::from_millis(190));
        assert!(first_paint_elapsed <= Duration::from_millis(250));
        let shutdown_started = Instant::now();
        stopped.store(true, Ordering::Release);
        tokio::time::timeout(Duration::from_millis(500), daemon)
            .await
            .expect("bounded daemon shutdown")
            .expect("daemon task")
            .expect("daemon result");
        shutdown_started
    });
    io.shutdown();
    assert!(shutdown_started.elapsed() <= Duration::from_millis(500));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn production_async_loop_discovers_samples_and_publishes_amd_fields() {
    use std::os::unix::fs::symlink;
    let (root, paths) = test_paths("production-amd");
    let proc_root = root.join("proc");
    let sys_root = root.join("sys");
    let device = sys_root.join("devices/pci0000:00/0000:c3:00.0");
    let driver = sys_root.join("bus/pci/drivers/amdgpu");
    let card = sys_root.join("class/drm/card17");
    let hwmon = device.join("hwmon/hwmon42");
    for path in [&root, &proc_root, &driver, &card, &hwmon] {
        fs::create_dir_all(path).expect("fixture directory");
    }
    symlink(&device, card.join("device")).expect("DRM device");
    symlink(&driver, device.join("driver")).expect("AMDGPU driver");
    for (name, value) in [
        ("vendor", "0x1002"),
        ("class", "0x030000"),
        ("gpu_busy_percent", "71"),
        ("vcn_busy_percent", "23"),
        ("mem_info_vram_used", "1073741824"),
        ("mem_info_vram_total", "4294967296"),
        ("hwmon/hwmon42/name", "amdgpu"),
        ("hwmon/hwmon42/temp7_label", "edge"),
        ("hwmon/hwmon42/temp7_input", "63000"),
        ("hwmon/hwmon42/freq3_label", "sclk"),
        ("hwmon/hwmon42/freq3_input", "2700000000"),
        ("hwmon/hwmon42/power1_average", "42000000"),
        ("hwmon/hwmon42/fan1_input", "1800"),
    ] {
        fs::write(device.join(name), value).expect("sysfs fixture");
    }
    let config = root.join("config.toml");
    fs::write(&config, r#"
[display]
poll_interval = 0.25
[panel]
order = ["amd"]
[panel.amd]
items = ["gpu_amd_usage", "gpu_amd_codec_usage", "gpu_amd_mem_usage", "gpu_amd_freq", "gpu_amd_temp", "gpu_amd_power", "gpu_amd_fan_speed"]
[tooltip]
order = []
[pages]
order = []
[notifications]
cpu_temp = false
gpu_nvidia_temp = false
gpu_amd_temp = false
disk_usage = false
disk_smart = false
hd_temp = false
battery_sys = false
battery_mouse = false
battery_kbd = false
load_avg = false
server_check = false
"#).expect("AMD config");
    let roots = FilesystemRoots {
        proc_root,
        sys_root,
        ..FilesystemRoots::default()
    };
    let mut io = crate::adapters::ProductionIo::start_without_signals().expect("production I/O");
    let sink = RecordingSink::default();
    let (applet_sender, applet_events) = mpsc::channel(1);
    let services = DaemonServices {
        commands: io.commands(),
        dbus: io.dbus(),
        notifications: io.notifications(),
        stopped: io.stopped(),
        events: io.take_event_receiver().expect("I/O events"),
        clock: ProductionClock::default(),
        shutdown: io.shutdown_receiver(),
        applet_events,
        sink: Box::new(sink.clone()),
        blocked_owner: None,
    };
    let stopped = Arc::clone(&services.stopped);
    realtime_runtime().block_on(async move {
        let daemon = tokio::spawn(async move {
            let _applet_sender = applet_sender;
            run(Some(&config), &roots, &paths, services, None).await
        });
        let published = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let html = sink.last_panel().unwrap_or_default();
                if ["71", "23", "25", "2700", "63", "42", "1800"]
                    .iter()
                    .all(|value| html.contains(value))
                {
                    break html;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await;
        stopped.store(true, Ordering::Release);
        tokio::time::timeout(Duration::from_secs(1), daemon)
            .await
            .expect("bounded shutdown")
            .expect("daemon task")
            .expect("daemon result");
        let html = published.expect("all seven AMD fields published");
        assert!(html.contains("class=\"panel"));
    });
    io.shutdown();
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rapid_page_steps_wake_the_idle_loop_and_apply_in_order() {
    use crate::domain::applet::{AppletRequest, PageStep};
    let (root, paths) = test_paths("production-rapid-steps");
    let proc_root = root.join("proc");
    let sys_root = root.join("sys");
    for path in [&root, &proc_root, &sys_root] {
        fs::create_dir_all(path).expect("fixture directory");
    }
    let config = root.join("config.toml");
    fs::write(
        &config,
        "[display]\npoll_interval = 30\n[panel]\norder = []\n[tooltip]\norder = []\n[pages]\norder = [\"cpu_cores\", \"graphs\"]\n",
    )
    .expect("config fixture");
    let roots = FilesystemRoots {
        proc_root,
        sys_root,
        ..FilesystemRoots::default()
    };
    let sink = RecordingSink::default();
    let (applet_sender, applet_events) = mpsc::channel(4);
    let mut io = crate::adapters::ProductionIo::start_without_signals().expect("production I/O");
    let services = DaemonServices {
        commands: io.commands(),
        dbus: io.dbus(),
        notifications: io.notifications(),
        stopped: io.stopped(),
        events: io.take_event_receiver().expect("I/O events"),
        clock: ProductionClock::default(),
        shutdown: io.shutdown_receiver(),
        applet_events,
        sink: Box::new(sink.clone()),
        blocked_owner: None,
    };
    let stopped = Arc::clone(&services.stopped);
    let selections = || {
        sink.frames()
            .into_iter()
            .filter_map(|frame| match frame {
                Frame::Selection(page, count) => Some((page, count)),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    realtime_runtime().block_on(async {
        let daemon =
            tokio::spawn(async move { run(Some(&config), &roots, &paths, services, None).await });
        // Let first paint pass so the loop is idle with nothing due for a long time.
        tokio::time::sleep(Duration::from_millis(400)).await;
        let sent = Instant::now();
        for step in [
            PageStep::Next,
            PageStep::Next,
            PageStep::Next,
            PageStep::Prev,
            PageStep::Next,
            PageStep::Prev,
        ] {
            applet_sender
                .send(AppletEvent::Request(AppletRequest::StepPage(step)))
                .await
                .expect("deliver step");
        }
        tokio::time::timeout(Duration::from_millis(200), async {
            while selections().len() < 7 {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("every step applied promptly");
        assert!(sent.elapsed() < Duration::from_millis(200));
        assert_eq!(
            selections(),
            vec![(0, 3), (1, 3), (2, 3), (0, 3), (2, 3), (0, 3), (2, 3)]
        );
        stopped.store(true, Ordering::Release);
        tokio::time::timeout(Duration::from_secs(1), daemon)
            .await
            .expect("bounded shutdown")
            .expect("daemon task")
            .expect("daemon result");
    });
    io.shutdown();
    let _ = fs::remove_dir_all(root);
}
