use super::{
    BufferFreshness, BufferState, DiskFileState, FileService, RestoreSummary, SessionBuffer,
    SessionStore,
};
use crate::app::domain::WorkspaceTab;
use crate::app::services::session_store::ops::BUFFER_FILE_EXTENSION;
use std::fs;

#[test]
fn stale_dirty_flag_clears_when_session_text_matches_disk() {
    let (store, buffer, _path) = restore_fixture("same", "same", true, None);

    let restored = store.restore_buffer_content(&buffer);

    assert!(!restored.is_dirty);
    assert_eq!(restored.freshness, BufferFreshness::InSync);
}

#[test]
fn dirty_session_text_stays_dirty_when_disk_has_not_changed() {
    let disk_text = "disk";
    let (store, mut buffer, _path) = restore_fixture("session", disk_text, true, None);
    buffer.disk_len = Some(disk_text.len() as u64);
    buffer.disk_modified_millis = restored_disk_state(&store, &buffer).modified_millis;

    let restored = store.restore_buffer_content(&buffer);

    assert!(restored.is_dirty);
    assert_eq!(restored.freshness, BufferFreshness::InSync);
}

#[test]
fn dirty_session_text_conflicts_when_disk_changed() {
    let (store, buffer, _path) = restore_fixture("session", "disk", true, None);

    let restored = store.restore_buffer_content(&buffer);

    assert!(restored.is_dirty);
    assert_eq!(restored.freshness, BufferFreshness::ConflictOnDisk);
}

#[test]
fn restore_status_pluralizes_counts() {
    let summary = RestoreSummary {
        conflicted_buffers: 1,
        missing_buffers: 2,
        ..RestoreSummary::default()
    };

    assert_eq!(
        summary.into_status().map(|status| status.message),
        Some("Session restore found 1 disk conflict and 2 missing files.".to_owned())
    );

    let summary = RestoreSummary {
        reloaded_clean_buffers: 1,
        ..RestoreSummary::default()
    };

    assert_eq!(
        summary.into_status().map(|status| status.message),
        Some("Reloaded 1 clean file from disk during session restore.".to_owned())
    );
}

#[test]
fn load_streaming_delivers_tabs_before_final_metadata() {
    let store = two_tab_store("first", "second");

    let mut started = false;
    let mut streamed_tabs = Vec::new();
    let restored = store
        .load_streaming(
            |active_tab_index, _, _| {
                started = active_tab_index == 1;
                true
            },
            |tab_index, tab, cold_session_tab| {
                streamed_tabs.push((
                    tab_index,
                    tab.active_buffer().name.clone(),
                    cold_session_tab.is_some(),
                ));
                true
            },
        )
        .unwrap()
        .unwrap();

    assert!(started);
    assert_eq!(streamed_tabs.len(), 2);
    assert_eq!(streamed_tabs[0], (1, "second.txt".to_owned(), false));
    assert_eq!(streamed_tabs[1], (0, "first.txt".to_owned(), true));
    assert_eq!(restored.active_tab_index, 1);
    assert!(restored.tabs.is_empty());
}

#[test]
fn load_startup_visible_restores_only_active_tab() {
    let store = two_tab_store("first", "second");

    let restored = store.load_startup_visible().unwrap().unwrap();

    assert_eq!(restored.tabs.len(), 1);
    assert_eq!(restored.active_tab_index, 0);
    assert_eq!(restored.tabs[0].active_buffer().name, "second.txt");
}

#[test]
fn cold_streamed_tabs_persist_original_payloads() {
    let store = two_tab_store("first original", "second original");

    let mut streamed_tabs = Vec::new();
    let mut cold_tabs = std::collections::HashMap::new();
    store
        .load_streaming(
            |_, _, _| true,
            |tab_index, tab, cold_session_tab| {
                if let Some(cold_session_tab) = cold_session_tab {
                    cold_tabs.insert(tab_index, cold_session_tab);
                }
                streamed_tabs.push((tab_index, tab));
                true
            },
        )
        .unwrap()
        .unwrap();
    streamed_tabs.sort_by_key(|(tab_index, _)| *tab_index);
    let streamed_tabs = streamed_tabs
        .into_iter()
        .map(|(_, tab)| tab)
        .collect::<Vec<_>>();

    let request =
        crate::app::services::session_store::SessionPersistRequest::capture_with_cold_tabs(
            &streamed_tabs,
            &cold_tabs,
            1,
            crate::app::services::session_store::SessionActiveSurface::Workspace,
            16.0,
            false,
        );
    store.persist_request(request).unwrap();

    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.tabs.len(), 2);
    assert_eq!(restored.tabs[0].active_buffer().name, "first.txt");
    assert_eq!(restored.tabs[0].active_buffer().text(), "first original");
    assert_eq!(restored.tabs[1].active_buffer().name, "second.txt");
    assert_eq!(restored.tabs[1].active_buffer().text(), "second original");
}

fn two_tab_store(first_text: &str, second_text: &str) -> SessionStore {
    let temp_dir = tempfile::tempdir().expect("create temp session root");
    let root = temp_dir.keep();
    let store = SessionStore::new(root);
    let first = WorkspaceTab::new(BufferState::new(
        "first.txt".to_owned(),
        first_text.to_owned(),
        None,
    ));
    let second = WorkspaceTab::new(BufferState::new(
        "second.txt".to_owned(),
        second_text.to_owned(),
        None,
    ));
    store.persist(&[first, second], 1, 16.0, false).unwrap();
    store
}

fn restore_fixture(
    session_text: &str,
    disk_text: &str,
    is_dirty: bool,
    session_disk_len: Option<u64>,
) -> (SessionStore, SessionBuffer, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().expect("create temp session root");
    let root = temp_dir.keep();
    fs::create_dir_all(&root).expect("create session root");
    let path = root.join("note.txt");
    fs::write(&path, disk_text).expect("write disk file");

    let temp_id = "buffer-test".to_owned();
    fs::write(
        root.join(format!("{temp_id}.{BUFFER_FILE_EXTENSION}")),
        session_text,
    )
    .expect("write session snapshot");

    let buffer = SessionBuffer {
        id: 1,
        name: "note.txt".to_owned(),
        path: Some(path.clone()),
        is_dirty,
        is_settings_file: false,
        show_control_chars: false,
        right_to_left_reading_order: false,
        temp_id,
        format: None,
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_modified_millis: None,
        disk_len: session_disk_len,
        text_history: Vec::new(),
    };

    (SessionStore::new(root), buffer, path)
}

fn restored_disk_state(store: &SessionStore, buffer: &SessionBuffer) -> DiskFileState {
    FileService::read_disk_state(buffer.path.as_ref().expect("fixture path")).unwrap_or_else(|_| {
        panic!(
            "read disk state from {}",
            store.root().join("note.txt").display()
        )
    })
}
