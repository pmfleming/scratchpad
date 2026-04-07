use scratchpad::app::domain::{BufferState, WorkspaceTab};
use scratchpad::app::services::session_store::SessionStore;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

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
        WorkspaceTab::new(BufferState::restored(
            "notes.txt".to_owned(),
            "alpha".to_owned(),
            Some(PathBuf::from("notes.txt")),
            true,
            "buffer-a".to_owned(),
            "UTF-8".to_owned(),
            false,
        )),
        WorkspaceTab::new(BufferState::restored(
            "Untitled".to_owned(),
            "beta".to_owned(),
            None,
            false,
            "buffer-b".to_owned(),
            "UTF-8".to_owned(),
            false,
        )),
    ];

    store.persist(&tabs, 1, 18.0, false).unwrap();
    let restored = store.load().unwrap().unwrap();

    assert_eq!(restored.tabs.len(), 2);
    assert_eq!(restored.active_tab_index, 1);
    assert_eq!(restored.font_size, 18.0);
    assert!(!restored.word_wrap);
    assert_eq!(restored.tabs[0].buffer.content, "alpha");
    assert!(restored.tabs[0].buffer.is_dirty);
    assert_eq!(restored.tabs[0].buffer.encoding, "UTF-8");
    assert_eq!(restored.tabs[1].buffer.content, "beta");

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
        "jp.txt".to_owned(),
        "こんにちは".to_owned(),
        Some(PathBuf::from("jp.txt")),
        false,
        "buffer-jp".to_owned(),
        "Shift_JIS".to_owned(),
        true,
    ))];

    store.persist(&tabs, 0, 14.0, true).unwrap();
    let restored = store.load().unwrap().unwrap();

    assert_eq!(restored.tabs[0].buffer.encoding, "Shift_JIS");
    assert!(restored.tabs[0].buffer.has_bom);

    fs::remove_dir_all(root).unwrap();
}
