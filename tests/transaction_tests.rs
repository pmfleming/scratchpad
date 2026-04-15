#![forbid(unsafe_code)]

use scratchpad::ScratchpadApp;
use scratchpad::app::domain::SplitAxis;
use scratchpad::app::services::file_controller::FileController;
use scratchpad::app::services::session_store::SessionStore;
use std::fs;

fn test_app() -> ScratchpadApp {
    let session_root = tempfile::tempdir().expect("create session dir");
    let session_store = SessionStore::new(session_root.path().to_path_buf());
    ScratchpadApp::with_session_store(session_store)
}

#[test]
fn new_tab_transaction_can_be_undone_from_log() {
    let mut app = test_app();

    app.new_tab();

    assert_eq!(app.tabs().len(), 2);
    let entry_id = app
        .latest_transaction_entry_id()
        .expect("transaction entry");

    assert!(app.undo_transaction_entry(entry_id));
    assert_eq!(app.tabs().len(), 1);
    assert_eq!(app.transaction_log_len(), 0);
}

#[test]
fn split_view_transaction_can_be_undone_from_log() {
    let mut app = test_app();

    app.split_active_view_with_placement(SplitAxis::Vertical, false, 0.5);

    assert_eq!(app.tabs()[0].views.len(), 2);
    let entry_id = app
        .latest_transaction_entry_id()
        .expect("transaction entry");

    assert!(app.undo_transaction_entry(entry_id));
    assert_eq!(app.tabs()[0].views.len(), 1);
}

#[test]
fn open_file_transaction_can_be_undone_from_log() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("transaction-log.txt");
    fs::write(&path, "alpha\nbeta\n").expect("write temp file");

    let mut app = test_app();
    FileController::open_external_paths(&mut app, vec![path.clone()]);

    assert_eq!(app.tabs().len(), 2);
    assert_eq!(
        app.tabs()[app.active_tab_index()]
            .active_buffer()
            .path
            .as_deref(),
        Some(path.as_path())
    );
    let entry_id = app
        .latest_transaction_entry_id()
        .expect("transaction entry");

    assert!(app.undo_transaction_entry(entry_id));
    assert_eq!(app.tabs().len(), 1);
    assert_eq!(app.tabs()[0].active_buffer().path, None);
}

#[test]
fn undo_from_transaction_log_keeps_log_window_open() {
    let mut app = test_app();
    app.open_transaction_log();
    app.new_tab();

    let entry_id = app
        .latest_transaction_entry_id()
        .expect("transaction entry");

    assert!(app.undo_transaction_entry(entry_id));
    assert!(app.transaction_log_open());
}
