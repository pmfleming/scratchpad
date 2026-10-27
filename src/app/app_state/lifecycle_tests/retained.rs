use super::{BufferState, WorkspaceTab, test_app};
use crate::app::{
    app_state::workspace_controller,
    domain::{CharCursor, CursorRange},
    services::session_manager,
};

#[test]
fn ordinary_open_restores_archived_unsaved_text_and_undo_history() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.txt");
    std::fs::write(&path, "original").unwrap();
    let mut app = test_app(root.path());
    let mut buffer = BufferState::new("note.txt".into(), "original".into(), Some(path.clone()));
    buffer.sync_to_disk_state(Some(
        crate::app::services::file_service::FileService::read_disk_state(&path).unwrap(),
    ));
    buffer
        .document_mut()
        .replace_char_ranges_with_undo(
            &[(0..8, "unsaved".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    buffer.is_dirty = true;
    app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
    workspace_controller::perform_close_tab(&mut app, 0);
    crate::app::services::file_controller::FileController::open_paths(&mut app, vec![path.clone()]);
    let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(buffer.text(), "unsaved");
    assert!(buffer.is_dirty);
    assert_eq!(buffer.document().operation_undo_depth(), 1);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
    assert!(
        app.state
            .persistence
            .session_store
            .closed_tabs()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn startup_external_changes_keep_historical_undo_separate_without_a_prompt() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.txt");
    std::fs::write(&path, "original").unwrap();
    let mut app = test_app(root.path());
    let mut buffer = BufferState::new("note.txt".into(), "original".into(), Some(path.clone()));
    buffer.sync_to_disk_state(Some(
        crate::app::services::file_service::FileService::read_disk_state(&path).unwrap(),
    ));
    buffer
        .document_mut()
        .replace_char_ranges_with_undo(
            &[(0..8, "saved edit".into())],
            CursorRange::default(),
            CursorRange::default(),
        )
        .unwrap();
    std::fs::write(&path, "saved edit").unwrap();
    buffer.sync_to_disk_state(Some(
        crate::app::services::file_service::FileService::read_disk_state(&path).unwrap(),
    ));
    app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
    session_manager::persist_session_now(&mut app).unwrap();
    drop(app);
    std::fs::write(&path, "external").unwrap();
    let mut app = test_app(root.path());
    session_manager::restore_session_state(&mut app).unwrap();
    app.wait_for_background_io_idle();
    assert!(
        crate::app::app_state::workspace::restore_conflict::current_startup_restore_conflict(&app)
            .is_none()
    );
    let buffers: Vec<_> = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(WorkspaceTab::buffers)
        .collect();
    assert!(
        buffers
            .iter()
            .any(|buffer| buffer.path.as_ref() == Some(&path)
                && buffer.text() == "external"
                && buffer.document().history_entries().is_empty())
    );
    assert!(buffers.iter().any(|buffer| buffer.path.is_none()
        && buffer.text() == "saved edit"
        && buffer.document().operation_undo_depth() == 1));
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
}

#[test]
fn restoring_recovery_text_is_one_undoable_edit_and_never_writes_disk() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.txt");
    std::fs::write(&path, "on disk").unwrap();
    let mut app = test_app(root.path());
    let tab = WorkspaceTab::new(BufferState::new(
        "note.txt".into(),
        "current".into(),
        Some(path.clone()),
    ));
    let id = tab.active_buffer().id;
    app.tab_manager.set_tabs(vec![tab], 0);
    assert!(
        crate::app::app_state::workspace::mutation::restore_recovery_text(
            &mut app,
            id,
            "older state"
        )
    );
    let buffer = app
        .tab_manager
        .active_tab_mut()
        .unwrap()
        .active_buffer_mut();
    assert_eq!(buffer.text(), "older state");
    assert!(buffer.is_dirty);
    buffer.document_mut().undo_last_operation().unwrap();
    assert_eq!(buffer.text(), "current");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "on disk");
}

#[test]
fn retained_workspace_reopens_with_undo_and_recovers_versions_without_overwriting_files() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("note.txt");
    std::fs::write(&file, "before").unwrap();
    let mut app = test_app(root.path());
    let tab = WorkspaceTab::new(BufferState::new(
        "note.txt".into(),
        "before".into(),
        Some(file.clone()),
    ));
    let id = tab.active_buffer().temp_id.clone();
    app.tab_manager.set_tabs(vec![tab], 0);
    session_manager::persist_session_now(&mut app).unwrap();
    let buffer = &mut app.tab_manager.tabs.as_mut_slice()[0].buffers.buffer;
    buffer
        .document_mut()
        .replace_char_ranges_with_undo(
            &[(6..6, " after".into())],
            CursorRange::one(CharCursor::new(6)),
            CursorRange::one(CharCursor::new(12)),
        )
        .unwrap();
    buffer.is_dirty = true;
    workspace_controller::perform_close_tab(&mut app, 0);
    assert!(app.tab_manager.tabs.is_empty());
    assert!(app.state.persistence.pending_closed_tabs.is_empty());
    drop(app);
    std::fs::write(&file, "external change").unwrap();

    // Exercise the same orchestration as the menus, through a reopened real DB.
    let mut app = test_app(root.path());
    let store = app.state.persistence.session_store.clone();
    assert!(store.load().unwrap().is_none());
    assert_eq!(store.closed_tabs().unwrap().len(), 1);
    workspace_controller::reopen_last_closed_tab(&mut app);
    let index = app.tab_manager.active_tab_index;
    let reopened = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(reopened.temp_id, id);
    assert_eq!(reopened.text(), "before after");
    assert!(reopened.is_dirty);
    assert!(store.closed_tabs().unwrap().is_empty());
    let count = app.tab_manager.tabs.len();
    workspace_controller::reopen_last_closed_tab(&mut app);
    assert_eq!(
        app.tab_manager.tabs.len(),
        count,
        "reopen cannot duplicate a consumed archive"
    );

    let points = store.buffer_versions(&id).unwrap();
    assert_eq!(points.len(), 1);
    let text = store.restore_buffer_version(points[0].id).unwrap();
    workspace_controller::append_tab(
        &mut app,
        WorkspaceTab::new(BufferState::new("recovered".into(), text, None)),
    );
    let recovered = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(recovered.text(), "before");
    assert!(recovered.path.is_none());
    assert_ne!(recovered.temp_id, id);
    assert_eq!(app.tab_manager.tabs.len(), count + 1);
    assert!(store.restore_buffer_version(i64::MAX).is_err());
    assert_eq!(
        app.tab_manager.tabs.len(),
        count + 1,
        "an expired version cannot create a tab"
    );
    let original = app.tab_manager.tabs.as_mut_slice()[index].active_buffer_mut();
    assert_eq!(original.text(), "before after");
    original.document_mut().undo_last_operation().unwrap();
    assert_eq!(original.text(), "before");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "external change");
}
