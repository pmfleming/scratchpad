#![forbid(unsafe_code)]

use rand::RngExt;
use rand::prelude::IndexedRandom;
use rand::seq::SliceRandom;
use scratchpad::ScratchpadApp;
use scratchpad::app::domain::{PaneBranch, PaneNode, SplitAxis, WorkspaceTab};
use scratchpad::app::{paths_match, services::session_store::SessionStore};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn path_match_is_case_insensitive_on_windows_paths() {
    assert!(paths_match(
        Path::new(r"C:\Temp\notes.txt"),
        Path::new(r"c:\temp\NOTES.txt")
    ));
}

#[test]
fn path_match_rejects_different_files() {
    assert!(!paths_match(
        Path::new(r"C:\Temp\notes.txt"),
        Path::new(r"C:\Temp\other.txt")
    ));
}

#[test]
fn reordering_tabs_preserves_active_tab_and_restore_order() {
    let session_root = std::env::temp_dir().join(format!(
        "scratchpad-tab-reorder-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let session_store = SessionStore::new(session_root.clone());
    let mut app = ScratchpadApp::with_session_store(session_store);

    app.tabs_mut()[0].buffer.name = "one.txt".to_owned();
    app.create_untitled_tab();
    app.tabs_mut()[1].buffer.name = "two.txt".to_owned();
    app.create_untitled_tab();
    app.tabs_mut()[2].buffer.name = "three.txt".to_owned();

    app.reorder_tab(0, 2);

    let ordered_names = app
        .tabs()
        .iter()
        .map(|tab| tab.buffer.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ordered_names, vec!["two.txt", "three.txt", "one.txt"]);
    assert_eq!(app.active_tab_index(), 1);
    assert_eq!(app.tabs()[app.active_tab_index()].buffer.name, "three.txt");

    app.session_store()
        .persist(
            app.tabs(),
            app.active_tab_index(),
            app.font_size(),
            app.word_wrap(),
            app.logging_enabled(),
        )
        .unwrap();

    let restored = app.session_store().load().unwrap().unwrap();
    let restored_names = restored
        .tabs
        .iter()
        .map(|tab| tab.buffer.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(restored_names, vec!["two.txt", "three.txt", "one.txt"]);
    assert_eq!(restored.active_tab_index, 1);
    assert_eq!(
        restored.tabs[restored.active_tab_index].buffer.name,
        "three.txt"
    );

    drop(app);
    fs::remove_dir_all(session_root).unwrap();
}

#[test]
fn opens_configurable_number_of_tabs_defaulting_to_1000() {
    let tab_count = std::env::var("SCRATCHPAD_TAB_STRESS_COUNT")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|count| *count > 0)
        .unwrap_or(1000);
    let session_root = std::env::temp_dir().join(format!(
        "scratchpad-tab-stress-test-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let session_store = SessionStore::new(session_root.clone());
    let mut app = ScratchpadApp::with_session_store(session_store);

    for _ in 1..tab_count {
        app.create_untitled_tab();
    }

    assert_eq!(app.tabs().len(), tab_count);

    let mut rng = rand::rng();
    let tabs_to_populate = (tab_count / 10).max(1);
    let mut indices: Vec<usize> = (0..tab_count).collect();
    indices.shuffle(&mut rng);

    for &index in &indices[..tabs_to_populate] {
        let line_count = rng.random_range(1..=1000);
        let mut content = String::new();
        for i in 0..line_count {
            content.push_str(&format!("This is random line {} in tab {}\n", i, index));
        }
        app.tabs_mut()[index].buffer.content = content;
        app.tabs_mut()[index].buffer.is_dirty = true;
    }

    for _ in 0..20 {
        let tab_index = rng.random_range(0..app.tabs().len());
        let axis = if rng.random_range(0..2) == 0 {
            SplitAxis::Vertical
        } else {
            SplitAxis::Horizontal
        };
        let new_view_first = rng.random_range(0..2) == 0;
        let split_ratio = rng.random_range(25..=75) as f32 / 100.0;

        let tab = &mut app.tabs_mut()[tab_index];
        tab.split_active_view_with_placement(axis, new_view_first, split_ratio)
            .expect("split should succeed during tab stress test");

        let split_paths = collect_split_paths(&tab.root_pane);
        assert!(
            !split_paths.is_empty(),
            "a split operation should create at least one resizable split path"
        );

        let resize_path = split_paths
            .choose(&mut rng)
            .expect("split paths should be available after splitting")
            .clone();
        let resize_ratio = rng.random_range(20..=80) as f32 / 100.0;
        assert!(tab.resize_split(resize_path, resize_ratio));

        if tab.views.len() > 1 && rng.random_range(0..2) == 0 {
            let close_index = rng.random_range(0..tab.views.len());
            let view_id = tab.views[close_index].id;
            assert!(tab.close_view(view_id));
        }

        assert_tab_layout_integrity(tab);
    }

    for tab in app.tabs() {
        assert_tab_layout_integrity(tab);
    }

    app.session_store()
        .persist(
            app.tabs(),
            app.active_tab_index(),
            app.font_size(),
            app.word_wrap(),
            app.logging_enabled(),
        )
        .unwrap();

    assert_eq!(app.active_tab_index(), tab_count - 1);

    let restored = app.session_store().load().unwrap().unwrap();
    assert_eq!(restored.tabs.len(), tab_count);
    assert_eq!(restored.active_tab_index, tab_count - 1);

    for &index in &indices[..tabs_to_populate] {
        assert!(!restored.tabs[index].buffer.content.is_empty());
    }

    for tab in &restored.tabs {
        assert_tab_layout_integrity(tab);
    }

    let mut close_indices: Vec<usize> = (0..tab_count).collect();
    close_indices.shuffle(&mut rng);

    for _ in close_indices {
        let current_count = app.tabs().len();
        let random_idx = rng.random_range(0..current_count);
        app.perform_close_tab_no_persist(random_idx);
    }

    assert_eq!(app.tabs().len(), 1);

    drop(app);
    fs::remove_dir_all(session_root).unwrap();
}

fn assert_tab_layout_integrity(tab: &WorkspaceTab) {
    let listed_view_ids = tab.views.iter().map(|view| view.id).collect::<HashSet<_>>();
    let mut pane_view_ids = HashSet::new();
    tab.root_pane.collect_view_ids(&mut pane_view_ids);

    assert_eq!(tab.root_pane.leaf_count(), tab.views.len());
    assert_eq!(pane_view_ids, listed_view_ids);
    assert!(listed_view_ids.contains(&tab.active_view_id));
}

fn collect_split_paths(root: &PaneNode) -> Vec<Vec<PaneBranch>> {
    let mut result = Vec::new();
    let mut current_path = Vec::new();
    collect_split_paths_recursive(root, &mut current_path, &mut result);
    result
}

fn collect_split_paths_recursive(
    node: &PaneNode,
    current_path: &mut Vec<PaneBranch>,
    result: &mut Vec<Vec<PaneBranch>>,
) {
    let PaneNode::Split { first, second, .. } = node else {
        return;
    };

    result.push(current_path.clone());

    current_path.push(PaneBranch::First);
    collect_split_paths_recursive(first, current_path, result);
    current_path.pop();

    current_path.push(PaneBranch::Second);
    collect_split_paths_recursive(second, current_path, result);
    current_path.pop();
}
