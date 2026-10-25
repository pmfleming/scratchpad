use super::{SessionActiveSurface, SessionPersistRequest, SessionStore};
use crate::app::domain::{BufferState, SplitAxis, WorkspaceTab};
use crate::app::services::file_service::FileService;
use crate::app::ui::editor_content::native_editor::{CharCursor, CursorRange};

#[test]
fn combined_tabs_preserve_format_flags_views_and_active_surface() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().to_owned());
    let mut buffer = BufferState::new("encoded".into(), "λ\r\na\u{1}\0🦀".into(), None);
    buffer.format.encoding_name = "UTF-16LE".into();
    buffer.format.has_bom = true;
    buffer.show_control_chars = true;
    buffer.right_to_left_reading_order = true;
    let first_id = buffer.id;
    let mut tab = WorkspaceTab::new(buffer);
    let other = WorkspaceTab::new(BufferState::new("other".into(), "second".into(), None));
    let active = tab
        .combine_with_tab(other, SplitAxis::Vertical, false, 0.4)
        .unwrap();
    let request = SessionPersistRequest::capture_with_cold_tabs(
        &[tab],
        &std::collections::HashMap::new(),
        0,
        SessionActiveSurface::Settings,
        18.0,
        true,
    );
    store.persist_request(request).unwrap();
    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.active_surface, SessionActiveSurface::Settings);
    let tab = &restored.tabs[0];
    assert_eq!(tab.layout.active_view_id(), active);
    assert_eq!(tab.layout.views.len(), 2);
    let buffer = tab.buffer_by_id(first_id).unwrap();
    assert_eq!(buffer.text(), "λ\r\na\u{1}\0🦀");
    assert_eq!(buffer.format.encoding_name, "UTF-16LE");
    assert!(
        buffer.format.has_bom && buffer.show_control_chars && buffer.right_to_left_reading_order
    );
}

fn persisted_dirty_file(text: &str) -> (tempfile::TempDir, WorkspaceTab, SessionStore) {
    let directory = tempfile::tempdir().unwrap();
    let mut tab = WorkspaceTab::new(BufferState::new(
        "saved.txt".into(),
        text.into(),
        Some(directory.path().join("saved.txt")),
    ));
    tab.buffers.buffer.is_dirty = true;
    let store = SessionStore::new(directory.path().join("session"));
    store
        .persist(std::slice::from_ref(&tab), 0, 16.0, false)
        .unwrap();
    (directory, tab, store)
}

#[test]
fn saved_clean_buffer_discards_obsolete_recovery_text() {
    let (directory, mut tab, store) = persisted_dirty_file("old dirty text");
    let path = directory.path().join("saved.txt");
    std::fs::write(&path, "current saved text").unwrap();
    tab.buffers.buffer.is_dirty = false;
    tab.buffers.buffer.replace_text("current saved text".into());
    tab.buffers
        .buffer
        .sync_to_disk_state(Some(FileService::read_disk_state(&path).unwrap()));
    store
        .persist(std::slice::from_ref(&tab), 0, 16.0, false)
        .unwrap();
    assert!(
        store
            .read_session(super::database::ReadScope::All)
            .unwrap()
            .unwrap()
            .contents
            .is_empty()
    );
    assert_eq!(
        store.load().unwrap().unwrap().tabs[0].buffers.buffer.text(),
        "current saved text"
    );
}

#[test]
fn clean_cold_backup_is_preserved_but_available_disk_is_authoritative() {
    let (directory, mut tab, store) = persisted_dirty_file("backup text");
    let path = directory.path().join("saved.txt");
    std::fs::write(&path, "new disk text").unwrap();
    tab.buffers.buffer.is_dirty = false;
    tab.buffers
        .buffer
        .sync_to_disk_state(Some(FileService::read_disk_state(&path).unwrap()));
    let cold = std::collections::HashMap::from([(0, super::cold_tab_from_workspace_tab(&tab))]);
    let request = SessionPersistRequest::capture_with_cold_tabs(
        &[tab],
        &cold,
        0,
        SessionActiveSurface::Workspace,
        16.0,
        false,
    );
    store.persist_request(request).unwrap();
    assert_eq!(
        store
            .read_session(super::database::ReadScope::All)
            .unwrap()
            .unwrap()
            .contents
            .len(),
        1
    );
    assert_eq!(
        store.load().unwrap().unwrap().tabs[0].buffers.buffer.text(),
        "new disk text"
    );
    std::fs::remove_file(path).unwrap();
    assert_eq!(
        store.load().unwrap().unwrap().tabs[0].buffers.buffer.text(),
        "backup text"
    );
}

#[test]
fn clean_disk_buffer_restores_exported_history_without_redundant_content() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("saved.txt");
    let mut buffer = BufferState::new("saved.txt".into(), "before".into(), Some(path.clone()));
    let cursor = |index| CursorRange::one(CharCursor::new(index));
    buffer
        .document_mut()
        .replace_char_ranges_with_undo(&[(6..6, " after".into())], cursor(6), cursor(12))
        .unwrap();
    assert_eq!(buffer.document().operation_undo_depth(), 1);
    std::fs::write(&path, buffer.text()).unwrap();
    buffer.sync_to_disk_state(Some(FileService::read_disk_state(&path).unwrap()));
    let store = SessionStore::new(directory.path().join("session"));
    store
        .persist(&[WorkspaceTab::new(buffer)], 0, 16.0, false)
        .unwrap();
    let mut restored = store.load().unwrap().unwrap();
    let document = restored.tabs[0].buffers.buffer.document_mut();
    assert_eq!(document.operation_undo_depth(), 1);
    assert!(document.undo_last_operation().is_some());
    assert_eq!(document.extract_text(), "before");
}
