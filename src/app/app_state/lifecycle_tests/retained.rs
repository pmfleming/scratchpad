use super::{BufferState, WorkspaceTab, test_app};
use crate::app::{
    app_state::workspace_controller,
    domain::{CharCursor, CursorRange},
    services::session_manager,
};

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
    workspace_controller::recover_buffer_version(&mut app, index, points[0].id);
    let recovered = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(recovered.text(), "before");
    assert!(recovered.path.is_none());
    assert_ne!(recovered.temp_id, id);
    assert_eq!(app.tab_manager.tabs.len(), count + 1);
    workspace_controller::recover_buffer_version(&mut app, index, i64::MAX);
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
