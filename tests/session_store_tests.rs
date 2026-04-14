#![forbid(unsafe_code)]

use scratchpad::app::domain::{BufferFreshness, BufferState, RestoredBufferState, WorkspaceTab};
use scratchpad::app::domain::{PaneNode, SplitAxis};
use scratchpad::app::services::session_store::SessionStore;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_session_root(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "scratchpad-{label}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn persists_and_restores_open_tabs() {
    let root = std::env::temp_dir().join(format!(
        "scratchpad-session-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = SessionStore::new(root.clone());
    let tabs = vec![
        WorkspaceTab::new(BufferState::restored(RestoredBufferState {
            id: 11,
            name: "notes.txt".to_owned(),
            content: "alpha".to_owned(),
            path: Some(PathBuf::from("notes.txt")),
            is_dirty: true,
            temp_id: "buffer-a".to_owned(),
            encoding: "UTF-8".to_owned(),
            has_bom: false,
            disk_state: None,
            freshness: BufferFreshness::InSync,
        })),
        WorkspaceTab::new(BufferState::restored(RestoredBufferState {
            id: 12,
            name: "Untitled".to_owned(),
            content: "beta".to_owned(),
            path: None,
            is_dirty: false,
            temp_id: "buffer-b".to_owned(),
            encoding: "UTF-8".to_owned(),
            has_bom: false,
            disk_state: None,
            freshness: BufferFreshness::InSync,
        })),
    ];

    store.persist(&tabs, 1, 18.0, false, true).unwrap();
    let restored = store.load().unwrap().unwrap();

    assert_eq!(restored.tabs.len(), 2);
    assert_eq!(restored.active_tab_index, 1);
    assert_eq!(restored.legacy_settings.font_size, 18.0);
    assert!(!restored.legacy_settings.word_wrap);
    assert!(restored.legacy_settings.logging_enabled);
    assert_eq!(restored.tabs[0].buffer.text(), "alpha");
    assert!(restored.tabs[0].buffer.is_dirty);
    assert_eq!(restored.tabs[0].buffer.encoding, "UTF-8");
    assert_eq!(restored.tabs[1].buffer.text(), "beta");
    assert_eq!(restored.tabs[0].views.len(), 1);
    assert!(matches!(restored.tabs[0].root_pane, PaneNode::Leaf { .. }));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn persists_encoding_metadata_for_restored_tabs() {
    let root = std::env::temp_dir().join(format!(
        "scratchpad-session-encoding-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = SessionStore::new(root.clone());
    let tabs = vec![WorkspaceTab::new(BufferState::restored(
        RestoredBufferState {
            id: 21,
            name: "jp.txt".to_owned(),
            content: "こんにちは".to_owned(),
            path: Some(PathBuf::from("jp.txt")),
            is_dirty: false,
            temp_id: "buffer-jp".to_owned(),
            encoding: "Shift_JIS".to_owned(),
            has_bom: true,
            disk_state: None,
            freshness: BufferFreshness::InSync,
        },
    ))];

    store.persist(&tabs, 0, 14.0, true, true).unwrap();
    let restored = store.load().unwrap().unwrap();

    assert_eq!(restored.tabs[0].buffer.encoding, "Shift_JIS");
    assert!(restored.tabs[0].buffer.has_bom);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn persists_control_character_inspection_mode() {
    let root = std::env::temp_dir().join(format!(
        "scratchpad-session-control-char-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = SessionStore::new(root.clone());
    let buffer = BufferState::restored(RestoredBufferState {
        id: 31,
        name: "ansi.txt".to_owned(),
        content: "\u{1b}[31mred\u{1b}[0m".to_owned(),
        path: Some(PathBuf::from("ansi.txt")),
        is_dirty: false,
        temp_id: "buffer-ansi".to_owned(),
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_state: None,
        freshness: BufferFreshness::InSync,
    });
    let mut tab = WorkspaceTab::new(buffer);
    tab.active_view_mut().unwrap().show_control_chars = true;
    let tabs = vec![tab];

    store.persist(&tabs, 0, 14.0, true, true).unwrap();
    let restored = store.load().unwrap().unwrap();

    assert!(restored.tabs[0].active_view().unwrap().show_control_chars);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn persists_split_views_and_active_view() {
    let root = std::env::temp_dir().join(format!(
        "scratchpad-session-split-view-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = SessionStore::new(root.clone());
    let mut tab = WorkspaceTab::new(BufferState::restored(RestoredBufferState {
        id: 41,
        name: "split.txt".to_owned(),
        content: "alpha\nbeta".to_owned(),
        path: Some(PathBuf::from("split.txt")),
        is_dirty: false,
        temp_id: "buffer-split".to_owned(),
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_state: None,
        freshness: BufferFreshness::InSync,
    }));
    tab.active_view_mut().unwrap().show_line_numbers = true;
    tab.split_active_view(SplitAxis::Vertical).unwrap();
    assert!(tab.resize_split(vec![], 0.63));
    tab.active_view_mut().unwrap().show_control_chars = false;
    let tabs = vec![tab];

    store.persist(&tabs, 0, 14.0, true, true).unwrap();
    let restored = store.load().unwrap().unwrap();

    assert_eq!(restored.tabs[0].views.len(), 2);
    assert!(matches!(restored.tabs[0].root_pane, PaneNode::Split { .. }));
    assert_eq!(
        restored.tabs[0].active_view_id,
        restored.tabs[0].views[1].id
    );
    match &restored.tabs[0].root_pane {
        PaneNode::Split { ratio, .. } => assert_eq!(*ratio, 0.63),
        PaneNode::Leaf { .. } => panic!("expected split root"),
    }

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restored_tabs_allocate_new_unique_view_ids() {
    let root = std::env::temp_dir().join(format!(
        "scratchpad-session-view-id-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = SessionStore::new(root.clone());
    let mut tab = WorkspaceTab::new(BufferState::restored(RestoredBufferState {
        id: 51,
        name: "split.txt".to_owned(),
        content: "alpha\nbeta".to_owned(),
        path: Some(PathBuf::from("split.txt")),
        is_dirty: false,
        temp_id: "buffer-split".to_owned(),
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_state: None,
        freshness: BufferFreshness::InSync,
    }));
    tab.split_active_view(SplitAxis::Vertical).unwrap();
    let original_ids = tab.views.iter().map(|view| view.id).collect::<Vec<_>>();

    store.persist(&[tab], 0, 14.0, true, true).unwrap();
    let mut restored = store.load().unwrap().unwrap();
    let restored_tab = &mut restored.tabs[0];
    let new_view_id = restored_tab
        .split_active_view(SplitAxis::Horizontal)
        .expect("split after restore should succeed");

    assert!(!original_ids.contains(&new_view_id));
    assert_eq!(
        restored_tab
            .views
            .iter()
            .map(|view| view.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        restored_tab.views.len()
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn persists_and_restores_combined_workspace_tabs() {
    let root = std::env::temp_dir().join(format!(
        "scratchpad-session-combine-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let store = SessionStore::new(root.clone());
    let mut target = WorkspaceTab::new(BufferState::restored(RestoredBufferState {
        id: 61,
        name: "left.txt".to_owned(),
        content: "left".to_owned(),
        path: Some(PathBuf::from("left.txt")),
        is_dirty: false,
        temp_id: "buffer-left".to_owned(),
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_state: None,
        freshness: BufferFreshness::InSync,
    }));
    let source = WorkspaceTab::new(BufferState::restored(RestoredBufferState {
        id: 62,
        name: "right.txt".to_owned(),
        content: "right".to_owned(),
        path: Some(PathBuf::from("right.txt")),
        is_dirty: true,
        temp_id: "buffer-right".to_owned(),
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_state: None,
        freshness: BufferFreshness::InSync,
    }));
    let source_view_id = source.active_view_id;
    target
        .combine_with_tab(source, SplitAxis::Vertical, false, 0.5)
        .expect("combine should succeed");

    store.persist(&[target], 0, 14.0, true, true).unwrap();
    let restored = store.load().unwrap().unwrap();
    let restored_tab = &restored.tabs[0];

    assert_eq!(restored_tab.views.len(), 2);
    assert_eq!(restored_tab.active_view_id, source_view_id);
    assert!(restored_tab.buffer_for_view(source_view_id).is_some());
    assert_eq!(restored_tab.buffers().count(), 2);
    assert_eq!(restored_tab.active_buffer().name, "right.txt");
    assert!(restored_tab.active_buffer().is_dirty);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restored_clean_buffer_reloads_newer_disk_content() {
    let root = unique_session_root("session-restore-reload");
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("notes.txt");
    fs::write(&path, "old session text\n").expect("write temp file");

    let store = SessionStore::new(root.clone());
    let mut tab = WorkspaceTab::new(BufferState::restored(RestoredBufferState {
        id: 71,
        name: "notes.txt".to_owned(),
        content: "old session text\n".to_owned(),
        path: Some(path.clone()),
        is_dirty: false,
        temp_id: "buffer-reload".to_owned(),
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_state: None,
        freshness: BufferFreshness::InSync,
    }));
    tab.buffer.sync_to_disk_state(
        scratchpad::app::services::file_service::FileService::read_disk_state(&path).ok(),
    );

    store.persist(&[tab], 0, 14.0, true, true).unwrap();
    fs::write(&path, "new disk text\n").expect("overwrite temp file");

    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.tabs[0].buffer.text(), "new disk text\n");
    assert_eq!(restored.tabs[0].buffer.freshness, BufferFreshness::InSync);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restored_dirty_buffer_keeps_session_text_and_marks_conflict() {
    let root = unique_session_root("session-restore-conflict");
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("draft.txt");
    fs::write(&path, "base text\n").expect("write temp file");

    let store = SessionStore::new(root.clone());
    let mut tab = WorkspaceTab::new(BufferState::restored(RestoredBufferState {
        id: 72,
        name: "draft.txt".to_owned(),
        content: "local unsaved text\n".to_owned(),
        path: Some(path.clone()),
        is_dirty: true,
        temp_id: "buffer-conflict".to_owned(),
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_state: None,
        freshness: BufferFreshness::InSync,
    }));
    tab.buffer.sync_to_disk_state(
        scratchpad::app::services::file_service::FileService::read_disk_state(&path).ok(),
    );

    store.persist(&[tab], 0, 14.0, true, true).unwrap();
    fs::write(&path, "newer disk text\n").expect("overwrite temp file");

    let restored = store.load().unwrap().unwrap();
    assert_eq!(restored.tabs[0].buffer.text(), "local unsaved text\n");
    assert_eq!(
        restored.tabs[0].buffer.freshness,
        BufferFreshness::ConflictOnDisk
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn restored_missing_path_marks_buffer_missing_on_disk() {
    let root = unique_session_root("session-restore-missing");
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let path = temp_dir.path().join("missing.txt");
    fs::write(&path, "temporary text\n").expect("write temp file");

    let store = SessionStore::new(root.clone());
    let mut tab = WorkspaceTab::new(BufferState::restored(RestoredBufferState {
        id: 73,
        name: "missing.txt".to_owned(),
        content: "temporary text\n".to_owned(),
        path: Some(path.clone()),
        is_dirty: true,
        temp_id: "buffer-missing".to_owned(),
        encoding: "UTF-8".to_owned(),
        has_bom: false,
        disk_state: None,
        freshness: BufferFreshness::InSync,
    }));
    tab.buffer.sync_to_disk_state(
        scratchpad::app::services::file_service::FileService::read_disk_state(&path).ok(),
    );

    store.persist(&[tab], 0, 14.0, true, true).unwrap();
    fs::remove_file(&path).expect("remove temp file");

    let restored = store.load().unwrap().unwrap();
    assert_eq!(
        restored.tabs[0].buffer.freshness,
        BufferFreshness::MissingOnDisk
    );
    assert_eq!(restored.tabs[0].buffer.text(), "temporary text\n");

    fs::remove_dir_all(root).unwrap();
}
