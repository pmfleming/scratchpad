use scratchpad::app::services::instance_broker::{ElectionResult, LaunchRequest, PrimaryInstance};
use scratchpad::app::startup::StartupOptions;
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
fn concurrent_elections_choose_one_primary_and_forward_the_rest() {
    let directory = tempfile::tempdir().unwrap();
    let ElectionResult::Primary(primary) =
        PrimaryInstance::elect(directory.path(), &request(1)).unwrap()
    else {
        panic!("first election should become primary");
    };
    let inbox = primary.start(&eframe::egui::Context::default()).unwrap();

    let handles = (2..12)
        .map(|id| {
            let root = directory.path().to_path_buf();
            std::thread::spawn(move || PrimaryInstance::elect(&root, &request(id)).unwrap())
        })
        .collect::<Vec<_>>();
    for handle in handles {
        assert!(matches!(handle.join().unwrap(), ElectionResult::Forwarded));
    }

    let deadline = Instant::now() + Duration::from_secs(2);
    let mut received = 0;
    while received < 10 && Instant::now() < deadline {
        if inbox.try_recv().is_ok() {
            received += 1;
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    assert_eq!(received, 10);
}

#[test]
fn lock_is_released_when_primary_inbox_drops() {
    let directory = tempfile::tempdir().unwrap();
    let ElectionResult::Primary(primary) =
        PrimaryInstance::elect(directory.path(), &request(20)).unwrap()
    else {
        panic!("first election should become primary");
    };
    let inbox = primary.start(&eframe::egui::Context::default()).unwrap();
    drop(inbox);

    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match PrimaryInstance::elect(directory.path(), &request(21)) {
            Ok(ElectionResult::Primary(_)) => break,
            _ if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => panic!("ownership was not released before the deadline"),
        }
    }
}
