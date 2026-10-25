use scratchpad::app::domain::{BufferState, WorkspaceTab};
use scratchpad::app::services::session_store::SessionStore;
use std::process::Command;

#[test]
#[ignore = "subprocess helper invoked by acknowledged_commit_survives_abrupt_exit"]
fn abrupt_writer() {
    let root = std::env::var_os("SCRATCHPAD_CRASH_TEST_ROOT").expect("isolated test root");
    let store = SessionStore::new(root.into());
    let tab = WorkspaceTab::new(BufferState::new(
        "unsaved".into(),
        "committed\0λ\r\n".into(),
        None,
    ));
    store.persist(&[tab], 0, 19.0, false).unwrap();
    // No worker join, connection drop, or automatic checkpoint on exit.
    std::process::exit(0);
}

#[test]
fn healthy_shutdown_drains_and_checkpoints_the_wal() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().to_owned());
    let tab = WorkspaceTab::new(BufferState::new(
        "saved session".into(),
        "still unsaved document".into(),
        None,
    ));
    store.persist(&[tab], 0, 16.0, false).unwrap();
    drop(store);
    let wal = directory.path().join("session.db-wal");
    assert!(!wal.exists() || wal.metadata().unwrap().len() == 0);
    let restored = SessionStore::new(directory.path().to_owned())
        .load()
        .unwrap()
        .unwrap();
    assert_eq!(
        restored.tabs[0].active_buffer().text(),
        "still unsaved document"
    );
}

#[test]
fn acknowledged_commit_survives_abrupt_exit() {
    let directory = tempfile::tempdir().unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "abrupt_writer"])
        .env("SCRATCHPAD_CRASH_TEST_ROOT", directory.path())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(directory.path().join("session.db").is_file());
    assert!(!directory.path().join("session.json").exists());
    let store = SessionStore::new(directory.path().to_owned());
    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.tabs[0].active_buffer().text(), "committed\0λ\r\n");
    assert_eq!(restored.legacy_settings.editor.font_size, 19.0);
}
