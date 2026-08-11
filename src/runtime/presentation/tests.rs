#![allow(clippy::expect_used)]

use super::*;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "plasma-top-presentation-{name}-{}",
        std::process::id()
    ))
}

#[test]
fn instance_id_is_strictly_positive_decimal() {
    assert_eq!(
        "42".parse::<InstanceId>().map(|id| id.to_string()),
        Ok(String::from("42"))
    );
    for invalid in ["", "0", "-1", "+1", "1.0", " 1", "01x"] {
        assert_eq!(invalid.parse::<InstanceId>(), Err(InvalidInstanceId));
    }
}

#[test]
fn scan_aggregates_instances_and_removes_stale_leases() {
    let directory = fixture("aggregate");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create fixture");
    let now = SystemTime::now();
    let live = "1".parse().expect("valid instance");
    let live_two = "3".parse().expect("valid instance");
    let stale = "2".parse().expect("valid instance");
    std::fs::write(lease_path(&directory, live), []).expect("write live lease");
    std::fs::write(lease_path(&directory, live_two), []).expect("write second live lease");
    std::fs::write(lease_path(&directory, stale), []).expect("write stale lease");
    let stale_path = lease_path(&directory, stale);
    let stale_time = now
        .checked_sub(LEASE_LIFETIME + Duration::from_secs(1))
        .expect("time");
    let file = File::options()
        .write(true)
        .open(&stale_path)
        .expect("open stale");
    file.set_modified(stale_time).expect("age stale lease");

    let state = scan(&directory, now).expect("scan leases");

    assert!(state.presented);
    assert!(
        state
            .next_expiry_in
            .is_some_and(|remaining| remaining <= LEASE_LIFETIME)
    );
    assert!(!stale_path.exists());
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn future_mtime_and_backward_clock_keep_expiry_bounded() {
    let directory = fixture("future-clock");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create fixture");
    let id: InstanceId = "9".parse().expect("valid instance");
    let path = lease_path(&directory, id);
    std::fs::write(&path, []).expect("write lease");
    let now = SystemTime::now();
    let future = now.checked_add(Duration::from_secs(3_600)).expect("future");
    File::options()
        .write(true)
        .open(&path)
        .expect("open lease")
        .set_modified(future)
        .expect("set future mtime");

    let state = scan(&directory, now).expect("scan before future mtime");

    assert_eq!(state.next_expiry_in, Some(LEASE_LIFETIME));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn concurrent_protocol_mutation_never_loses_refreshed_lease() {
    let directory = fixture("concurrent");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create fixture");
    let id: InstanceId = "11".parse().expect("valid instance");
    let path = lease_path(&directory, id);
    std::fs::write(&path, []).expect("write stale lease");
    let stale = SystemTime::now()
        .checked_sub(LEASE_LIFETIME + Duration::from_secs(1))
        .expect("stale time");
    File::options()
        .write(true)
        .open(&path)
        .expect("open stale lease")
        .set_modified(stale)
        .expect("age lease");
    let scan_dir = directory.clone();
    let present_dir = directory.clone();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let scan_barrier = std::sync::Arc::clone(&barrier);
    let scan_thread = std::thread::spawn(move || {
        scan_barrier.wait();
        for _ in 0..100 {
            scan(&scan_dir, SystemTime::now()).expect("concurrent scan");
        }
    });
    let present_thread = std::thread::spawn(move || {
        barrier.wait();
        for _ in 0..100 {
            dismiss_at(&present_dir, id).expect("concurrent dismiss");
            present_at(&present_dir, id).expect("concurrent present");
        }
    });
    scan_thread.join().expect("scan thread");
    present_thread.join().expect("present thread");

    assert!(
        scan(&directory, SystemTime::now())
            .expect("final scan")
            .presented
    );
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn dismiss_is_idempotent() {
    let directory = fixture("dismiss");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("create fixture");
    let id: InstanceId = "7".parse().expect("valid instance");
    present_at(&directory, id).expect("present");
    dismiss_at(&directory, id).expect("first dismiss");
    dismiss_at(&directory, id).expect("second dismiss");
    let _ = std::fs::remove_dir_all(directory);
}
