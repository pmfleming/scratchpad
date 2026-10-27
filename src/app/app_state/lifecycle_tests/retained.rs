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
    use crate::app::{domain::BufferFreshness, services::file_service::FileService};
    for (missing, open_disk) in [(false, true), (false, false), (true, true)] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        std::fs::write(&path, "saved edit").unwrap();
        let mut app = test_app(root.path());
        let mut policy = app.state.app_settings.history.recovery.clone();
        policy.open_changed_disk_files = open_disk;
        crate::app::app_state::settings_controller::set_recovery_settings(&mut app, policy);
        let mut buffer = BufferState::new("note.txt".into(), "original".into(), Some(path.clone()));
        buffer
            .document_mut()
            .replace_char_ranges_with_undo(
                &[(0..8, "saved edit".into())],
                CursorRange::default(),
                CursorRange::default(),
            )
            .unwrap();
        buffer.sync_to_disk_state(Some(FileService::read_disk_state(&path).unwrap()));
        app.tab_manager.set_tabs(vec![WorkspaceTab::new(buffer)], 0);
        session_manager::persist_session_now(&mut app).unwrap();
        drop(app);
        if missing {
            std::fs::remove_file(&path).unwrap();
        } else {
            std::fs::write(&path, "external").unwrap();
        }
        let mut app = test_app(root.path());
        crate::app::app_state::test_support::wait_for_background_io(&mut app);
        assert!(!app.state.dialogs.any_modal_open());
        let buffers: Vec<_> = app
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(WorkspaceTab::buffers)
            .collect();
        let disk = buffers
            .iter()
            .find(|buffer| buffer.path.as_ref() == Some(&path));
        assert_eq!(disk.is_some(), !missing && open_disk);
        if let Some(disk) = disk {
            assert_eq!(disk.text(), "external");
            assert!(disk.document().history_entries().is_empty());
        }
        let recovered = buffers.iter().find(|buffer| buffer.path.is_none()).unwrap();
        assert_eq!(recovered.text(), "saved edit");
        assert_eq!(recovered.document().operation_undo_depth(), 1);
        assert_eq!(recovered.freshness, BufferFreshness::InSync);
        assert!(recovered.disk_state.is_none());
        assert!(recovered.disk_content_digest.is_none());
        assert!(!recovered.is_settings_file);
        assert_eq!(path.exists(), !missing);
        if !missing {
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "external");
        }
    }
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
