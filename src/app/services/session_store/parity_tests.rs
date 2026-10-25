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
