use super::*;
use std::{
    fs,
    path::Path,
    thread,
    time::{Duration, Instant},
};

#[test]
fn notifications_rearm_and_follow_replacement_roots() {
    let root = tempfile::tempdir().unwrap();
    let first = root.path().join("first λ");
    let second = root.path().join("second 🦀");
    fs::create_dir(&first).unwrap();
    fs::create_dir(&second).unwrap();
    let mut service = FileWatchService::new();
    service.set_watched_directories(BTreeSet::from([first.clone()]));
    wait_for_change(&mut service, &first);
    wait_for_change(&mut service, &first);
    service.set_watched_directories(BTreeSet::from([second.clone()]));
    wait_for_change(&mut service, &second);
}

#[test]
fn notifications_reach_directories_beyond_the_windows_wait_limit() {
    let root = tempfile::tempdir().unwrap();
    let directories: BTreeSet<_> = (0..65)
        .map(|index| {
            let path = root.path().join(format!("directory-{index:02}"));
            fs::create_dir(&path).unwrap();
            path
        })
        .collect();
    let last = directories.last().unwrap().clone();
    let mut service = FileWatchService::new();
    service.set_watched_directories(directories);
    wait_for_change(&mut service, &last);
}

#[test]
fn stop_terminates_idle_and_active_watchers() {
    let root = tempfile::tempdir().unwrap();
    for directories in [vec![], vec![root.path().to_owned()]] {
        let (commands, incoming) = mpsc::channel();
        let (events, _received) = mpsc::channel();
        let (finished, completion) = mpsc::channel();
        let worker = thread::spawn(move || {
            windows_watch_loop(incoming, events);
            finished.send(()).unwrap();
        });
        commands
            .send(WatchCommand::SetDirectories(directories))
            .unwrap();
        commands.send(WatchCommand::Stop).unwrap();
        completion.recv_timeout(Duration::from_secs(5)).unwrap();
        worker.join().unwrap();
    }
}

fn wait_for_change(service: &mut FileWatchService, directory: &Path) {
    service.drain_events();
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut generation = 0;
    loop {
        // Repeat the stimulus until registration/rearming has completed; no
        // fixed sleep is assumed sufficient for the asynchronous watcher.
        fs::write(directory.join("probe.txt"), generation.to_string()).unwrap();
        if service.drain_events().iter().any(
            |event| matches!(event, FileWatchEvent::DirectoryChanged(path) if path == directory),
        ) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "no notification for {directory:?}"
        );
        generation += 1;
        thread::sleep(Duration::from_millis(50));
    }
}
