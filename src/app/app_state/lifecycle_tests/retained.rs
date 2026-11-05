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
    assert_eq!(buffer.document().operation_undo_depth(), 2);
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
fn startup_external_changes_keep_one_document_and_dirty_conflicts_keep_the_path() {
    use crate::app::{
        domain::{BufferFreshness, buffer::PieceSource},
        services::file_service::FileService,
    };
    for (missing, dirty) in [(false, false), (false, true), (true, true)] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("note.txt");
        std::fs::write(&path, "saved edit").unwrap();
        let mut app = test_app(root.path());
        let mut buffer = BufferState::new("note.txt".into(), "original".into(), Some(path.clone()));
        buffer
            .document_mut()
            .replace_exact_with_history("saved edit");
        buffer.sync_to_disk_state(Some(FileService::read_disk_state(&path).unwrap()));
        if dirty {
            buffer
                .document_mut()
                .replace_exact_with_history("unsaved local");
            buffer.is_dirty = true;
        }
        let id = buffer.temp_id.clone();
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
        assert_eq!(app.tab_manager.tabs.len(), 1);
        let buffer = app.tab_manager.active_tab().unwrap().active_buffer();
        assert_eq!(buffer.temp_id, id);
        if missing {
            assert!(buffer.path.is_none());
            assert_eq!(buffer.text(), "unsaved local");
        } else if dirty {
            assert_eq!(buffer.path.as_ref(), Some(&path));
            assert_eq!(buffer.text(), "unsaved local");
            assert_eq!(buffer.freshness, BufferFreshness::ConflictOnDisk);
        } else {
            assert_eq!(buffer.path.as_ref(), Some(&path));
            assert_eq!(buffer.text(), "external");
            assert_eq!(
                buffer.document().history_entries().last().unwrap().source,
                PieceSource::External
            );
            let mut document = buffer.document().clone();
            document.undo_last_operation().unwrap();
            assert_eq!(document.extract_text(), "saved edit");
        }
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
    let closed = store.closed_tabs().unwrap();
    assert_eq!(closed.len(), 1);
    let token = &closed[0].token;
    workspace_controller::reopen_archived_tab(&mut app, token);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    let index = app.tab_manager.active_tab_index;
    let reopened = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(reopened.temp_id, id);
    assert_eq!(reopened.text(), "external change");
    assert!(!reopened.is_dirty);
    assert!(store.closed_tabs().unwrap().is_empty());
    let count = app.tab_manager.tabs.len();
    workspace_controller::reopen_archived_tab(&mut app, token);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    assert_eq!(
        app.tab_manager.tabs.len(),
        count,
        "reopen cannot duplicate a consumed archive"
    );

    let original = app.tab_manager.tabs.as_mut_slice()[index].active_buffer_mut();
    assert_eq!(original.text(), "external change");
    original.document_mut().undo_last_operation().unwrap();
    assert_eq!(original.text(), "before after");
    original.document_mut().undo_last_operation().unwrap();
    assert_eq!(original.text(), "before");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "external change");
}
