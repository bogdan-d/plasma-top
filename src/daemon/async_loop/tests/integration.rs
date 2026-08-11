use std::sync::atomic::AtomicBool;

use super::*;

fn realtime_runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("runtime")
}

#[test]
fn production_async_loop_publishes_while_real_owner_task_is_blocked() {
    let (root, paths) = test_paths("production-blocked-owner");
    fs::create_dir_all(&paths.state).expect("state root");
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
        runtime_root: Some(paths.runtime.clone()),
        proc_root,
        sys_root,
        ..FilesystemRoots::default()
    };
    let entered = Arc::new(AtomicBool::new(false));
    let release = Arc::new(Semaphore::new(0));
    let mut io = crate::adapters::ProductionIo::start_without_signals().expect("production I/O");
    let services = DaemonServices {
        commands: io.commands(),
        dbus: io.dbus(),
        notifications: io.notifications(),
        stopped: io.stopped(),
        events: io.take_event_receiver().expect("I/O events"),
        clock: ProductionClock::default(),
        shutdown: io.shutdown_receiver(),
        blocked_owner: Some(worker::TestOwnerBlock {
            owner: OwnerId::Disk,
            entered: Arc::clone(&entered),
            release: Arc::clone(&release),
        }),
    };
    let panel = paths.panel.clone();
    let stopped = Arc::clone(&services.stopped);
    let first_paint_started = Instant::now();

    let shutdown_started = realtime_runtime().block_on(async move {
        let daemon =
            tokio::spawn(async move { run(Some(&config), &roots, &paths, services).await });
        tokio::time::timeout(Duration::from_secs(1), async {
            while !panel.exists() || !entered.load(Ordering::Acquire) {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("panel publication while owner blocked");
        assert!(fs::read_to_string(&panel).is_ok_and(|html| html.contains("class=\"panel")));
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
