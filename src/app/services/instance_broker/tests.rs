use super::{BrokerResponse, ElectionResult, LaunchRequest, PrimaryInstance};
use crate::app::startup::StartupOptions;
use std::fs::OpenOptions;
use std::time::{Duration, Instant};

fn request(id: u128) -> LaunchRequest {
    LaunchRequest {
        invocation_id: id,
        sender_pid: std::process::id(),
        options: StartupOptions::default(),
        activate: true,
    }
}

#[test]
fn ownership_lock_prevents_a_second_primary() {
    let directory = tempfile::tempdir().unwrap();
    let primary = PrimaryInstance::elect(directory.path(), &request(1)).unwrap();
    assert!(matches!(primary, ElectionResult::Primary(_)));

    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(directory.path().join("instance.lock"))
        .unwrap();
    assert!(matches!(
        lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
}

#[test]
fn primary_listener_accepts_a_forwarded_request() {
    let directory = tempfile::tempdir().unwrap();
    let ElectionResult::Primary(mut primary) =
        PrimaryInstance::elect(directory.path(), &request(1)).unwrap()
    else {
        panic!("first election should become primary");
    };
    let context = eframe::egui::Context::default();
    let inbox = primary.start(&context).unwrap();

    let forwarded = std::thread::spawn({
        let root = directory.path().to_path_buf();
        move || PrimaryInstance::elect(&root, &request(2)).unwrap()
    })
    .join()
    .unwrap();
    assert!(matches!(forwarded, ElectionResult::Forwarded));

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Ok(received) = inbox.try_recv() {
            assert_eq!(received.invocation_id, 2);
            break;
        }
        assert!(Instant::now() < deadline, "forwarded request not delivered");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn clean_request_is_rejected_by_primary() {
    let request = LaunchRequest {
        options: StartupOptions::clean(),
        ..request(3)
    };
    assert!(matches!(
        request.validate_for_existing_primary(),
        Err(reason) if reason.contains("/clean")
    ));
}

#[test]
fn response_type_remains_explicit() {
    assert_ne!(BrokerResponse::Busy, BrokerResponse::Accepted);
}
