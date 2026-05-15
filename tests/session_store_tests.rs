use scratchpad::app::domain::{
    BufferFreshness, BufferState, DiskFileState, SplitAxis, WorkspaceTab,
};
use scratchpad::app::services::session_store::SessionStore;

#[test]
fn persists_and_restores_open_tabs() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().to_path_buf());
    let tabs = vec![
        WorkspaceTab::new(BufferState::new(
            "one.txt".to_owned(),
            "one".to_owned(),
            None,
        )),
        WorkspaceTab::new(BufferState::new(
            "two.txt".to_owned(),
            "two".to_owned(),
            None,
        )),
    ];

    store.persist(&tabs, 1, 17.0, false).unwrap();
    let restored = store.load().unwrap().unwrap();

    assert_eq!(restored.active_tab_index, 1);
    assert_eq!(restored.tabs.len(), 2);
    assert_eq!(restored.tabs[0].buffers.buffer.text(), "one");
    assert_eq!(restored.tabs[1].buffers.buffer.text(), "two");
    assert_eq!(restored.legacy_settings.editor.font_size, 17.0);
    assert!(!restored.legacy_settings.editor.word_wrap);
}

#[test]
fn persists_split_views_and_active_view() {
    let directory = tempfile::tempdir().unwrap();
    let store = SessionStore::new(directory.path().to_path_buf());
    let mut tab = WorkspaceTab::new(BufferState::new(
        "split.txt".to_owned(),
        "content".to_owned(),
        None,
    ));
    let first_view = tab.layout.active_view_id;
    let second_view = tab.split_active_view(SplitAxis::Vertical).unwrap();
    assert!(tab.activate_view(second_view));

    store.persist(&[tab], 0, 14.0, true).unwrap();
    let restored = store.load().unwrap().unwrap();
    let restored_tab = &restored.tabs[0];

    assert_eq!(restored_tab.layout.views.len(), 2);
    assert_ne!(restored_tab.layout.active_view_id, first_view);
    assert!(
        restored_tab
            .layout
            .view(restored_tab.layout.active_view_id)
            .is_some()
    );
}

#[test]
fn restored_clean_buffer_reloads_newer_disk_content() {
    let directory = tempfile::tempdir().unwrap();
    let file_path = directory.path().join("tracked.txt");
    std::fs::write(&file_path, "session").unwrap();
    let store = SessionStore::new(directory.path().join("session"));
    let mut buffer = BufferState::new(
        "tracked.txt".to_owned(),
        "session".to_owned(),
        Some(file_path.clone()),
    );
    buffer.sync_to_disk_state(Some(DiskFileState {
        modified_millis: Some(1),
        len: 7,
    }));

    store
        .persist(&[WorkspaceTab::new(buffer)], 0, 14.0, true)
        .unwrap();
    std::fs::write(&file_path, "disk").unwrap();
    let restored = store.load().unwrap().unwrap();

    assert_eq!(restored.tabs[0].buffers.buffer.text(), "disk");
    assert_eq!(
        restored.tabs[0].buffers.buffer.freshness,
        BufferFreshness::InSync
    );
}

#[test]
fn restored_dirty_buffer_keeps_session_text_and_marks_conflict() {
    let directory = tempfile::tempdir().unwrap();
    let file_path = directory.path().join("tracked.txt");
    std::fs::write(&file_path, "session").unwrap();
    let store = SessionStore::new(directory.path().join("session"));
    let mut buffer = BufferState::new(
        "tracked.txt".to_owned(),
        "session".to_owned(),
        Some(file_path.clone()),
    );
    buffer.is_dirty = true;
    buffer.sync_to_disk_state(Some(DiskFileState {
        modified_millis: Some(1),
        len: 7,
    }));

    store
        .persist(&[WorkspaceTab::new(buffer)], 0, 14.0, true)
        .unwrap();
    std::fs::write(&file_path, "disk").unwrap();
    let restored = store.load().unwrap().unwrap();

    assert_eq!(restored.tabs[0].buffers.buffer.text(), "session");
    assert_eq!(
        restored.tabs[0].buffers.buffer.freshness,
        BufferFreshness::ConflictOnDisk
    );
}
