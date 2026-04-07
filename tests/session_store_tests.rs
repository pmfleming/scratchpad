use scratchpad::app::domain::{BufferState, WorkspaceTab};
use scratchpad::app::domain::{PaneNode, SplitAxis};
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
    let buffer = BufferState::restored(
        "ansi.txt".to_owned(),
        "\u{1b}[31mred\u{1b}[0m".to_owned(),
        Some(PathBuf::from("ansi.txt")),
        false,
        "buffer-ansi".to_owned(),
        "UTF-8".to_owned(),
        false,
    );
    let mut tab = WorkspaceTab::new(buffer);
    tab.active_view_mut().unwrap().show_control_chars = true;
    let tabs = vec![tab];

    store.persist(&tabs, 0, 14.0, true).unwrap();
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
    let mut tab = WorkspaceTab::new(BufferState::restored(
        "split.txt".to_owned(),
        "alpha\nbeta".to_owned(),
        Some(PathBuf::from("split.txt")),
        false,
        "buffer-split".to_owned(),
        "UTF-8".to_owned(),
        false,
    ));
    tab.active_view_mut().unwrap().show_line_numbers = true;
    tab.split_active_view(SplitAxis::Vertical).unwrap();
    assert!(tab.resize_split(vec![], 0.63));
    tab.active_view_mut().unwrap().show_control_chars = false;
    let tabs = vec![tab];

    store.persist(&tabs, 0, 14.0, true).unwrap();
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
    let mut tab = WorkspaceTab::new(BufferState::restored(
        "split.txt".to_owned(),
        "alpha\nbeta".to_owned(),
        Some(PathBuf::from("split.txt")),
        false,
        "buffer-split".to_owned(),
        "UTF-8".to_owned(),
        false,
    ));
    tab.split_active_view(SplitAxis::Vertical).unwrap();
    let original_ids = tab.views.iter().map(|view| view.id).collect::<Vec<_>>();

    store.persist(&[tab], 0, 14.0, true).unwrap();
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
