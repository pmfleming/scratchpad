use super::{TabManager, WorkspaceTab};
use crate::app::domain::{BufferId, BufferState, SplitAxis};

fn tab(name: &str) -> WorkspaceTab {
    WorkspaceTab::new(BufferState::new(name.to_owned(), String::new(), None))
}

fn disk_tab(path: &std::path::Path, text: &str) -> WorkspaceTab {
    WorkspaceTab::new(BufferState::new(
        path.file_name().unwrap().to_string_lossy().into_owned(),
        text.to_owned(),
        Some(path.to_path_buf()),
    ))
}

#[test]
fn path_owner_rechecks_hard_links_after_replacement() {
    let root = tempfile::tempdir().unwrap();
    let original = root.path().join("original");
    let alias = root.path().join("alias");
    std::fs::write(&original, "original").unwrap();
    std::fs::hard_link(&original, &alias).unwrap();
    let mut manager = TabManager::new();
    manager.set_tabs(vec![disk_tab(&original, "original")], 0);
    assert!(manager.find_tab_by_path(&alias).is_some());
    std::fs::remove_file(&original).unwrap();
    std::fs::write(&original, "replacement").unwrap();
    assert!(manager.find_tab_by_path(&alias).is_none());
    assert!(manager.find_tab_by_path(&original).is_some());
}

#[test]
fn restored_aliases_keep_independent_edits_as_recovered_copies() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    std::fs::write(&a, "disk").unwrap();
    std::fs::hard_link(&a, &b).unwrap();
    let mut manager = TabManager::new();
    manager.set_tabs(
        vec![disk_tab(&a, "first edits"), disk_tab(&b, "second edits")],
        0,
    );
    assert_eq!(manager.tabs.len(), 2);
    assert_eq!(manager.tabs[0].active_buffer().text(), "first edits");
    let recovered = manager.tabs[1].active_buffer();
    assert_eq!(recovered.text(), "second edits");
    assert!(recovered.path.is_none());
    assert!(recovered.is_dirty);
    assert_eq!(manager.find_tab_by_path(&b).unwrap().0, 0);
}

#[cfg(unix)]
#[test]
fn path_owner_does_not_trust_a_retargeted_symlink_cache() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    let b = root.path().join("b");
    let link = root.path().join("link");
    std::fs::write(&a, "a").unwrap();
    std::fs::write(&b, "b").unwrap();
    std::os::unix::fs::symlink(&a, &link).unwrap();
    let mut manager = TabManager::new();
    manager.set_tabs(vec![disk_tab(&link, "a")], 0);
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&b, &link).unwrap();
    assert!(manager.find_tab_by_path(&a).is_none());
    assert!(manager.find_tab_by_path(&b).is_some());
    manager.append_tab(disk_tab(&a, "a"));
    assert_eq!(manager.find_tab_by_path(&a).unwrap().0, 1);
}

fn assert_buffer_slots(manager: &TabManager, expected: &[Vec<BufferId>]) {
    for (index, ids) in expected.iter().enumerate() {
        for id in ids {
            assert_eq!(manager.tab_index_for_buffer(*id), Some(index));
        }
    }
    assert_eq!(
        manager.buffer_tab_index.len(),
        expected.iter().map(Vec::len).sum::<usize>()
    );
}

#[test]
fn buffer_tab_index_tracks_tab_mutations() {
    let first = tab("first.txt");
    let first_id = first.active_buffer().id;

    let mut second = tab("second.txt");
    let second_id = second.active_buffer().id;
    let split_buffer = BufferState::new("split.txt".to_owned(), String::new(), None);
    let split_id = split_buffer.id;
    second
        .open_buffer_as_split(split_buffer, SplitAxis::Vertical, true, 0.5)
        .unwrap();

    let mut manager = TabManager::new();
    manager.set_tabs(vec![first, second], 0);
    let mut expected = vec![vec![first_id], vec![second_id, split_id]];
    assert_buffer_slots(&manager, &expected);

    let restored = tab("restored.txt");
    expected.push(vec![restored.active_buffer().id]);
    manager.append_restored_tab(restored);
    assert_eq!(manager.active_tab_index, 0);
    assert_buffer_slots(&manager, &expected);

    let appended = tab("appended.txt");
    expected.push(vec![appended.active_buffer().id]);
    manager.append_tab(appended);
    assert_eq!(manager.active_tab_index, 3);
    assert_buffer_slots(&manager, &expected);

    let inserted = tab("inserted.txt");
    expected.insert(1, vec![inserted.active_buffer().id]);
    manager.insert_tab(1, inserted);
    assert_buffer_slots(&manager, &expected);

    assert!(manager.reorder_tab(2, 0));
    let moved = expected.remove(2);
    expected.insert(0, moved);
    assert_buffer_slots(&manager, &expected);

    manager.close_tab_internal(0);
    expected.remove(0);
    assert_buffer_slots(&manager, &expected);
}

#[test]
fn closing_last_tab_leaves_empty_workspace_and_indices() {
    let mut manager = TabManager::new();
    manager.close_tab_internal(0);

    assert!(manager.tabs.is_empty());
    assert!(manager.active_tab().is_none());
    assert_eq!(manager.active_tab_index, 0);
    assert!(manager.buffer_tab_index.is_empty());
    assert!(manager.path_tab_index.is_empty());
    assert!(manager.cold_session_tabs.is_empty());
    assert!(manager.session_dirty);

    manager.create_untitled_tab();
    assert_eq!(manager.tabs.len(), 1);
    assert!(manager.active_tab().is_some());
}

#[test]
fn path_index_tracks_open_file_owners() {
    let directory = tempfile::tempdir().unwrap();
    let first_path = directory.path().join("first.txt");
    let second_path = directory.path().join("second.txt");
    std::fs::write(&first_path, "first").unwrap();
    std::fs::write(&second_path, "second").unwrap();

    let first = disk_tab(&first_path, "first");
    let first_view = first.layout.active_view_id();
    let second = disk_tab(&second_path, "second");
    let second_view = second.layout.active_view_id();
    let mut manager = TabManager::new();
    manager.set_tabs(vec![first, second], 0);

    assert_eq!(manager.find_tab_by_path(&first_path), Some((0, first_view)));
    assert_eq!(
        manager.find_tab_by_path(&second_path),
        Some((1, second_view))
    );

    assert!(manager.reorder_tab(0, 1));
    assert_eq!(manager.find_tab_by_path(&first_path), Some((1, first_view)));
    assert_eq!(
        manager.find_tab_by_path(&second_path),
        Some((0, second_view))
    );
    assert!(manager.reorder_tab(1, 0));

    manager.close_tab_internal(0);

    assert_eq!(manager.find_tab_by_path(&first_path), None);
    assert_eq!(
        manager.find_tab_by_path(&second_path),
        Some((0, second_view))
    );
}

#[test]
fn set_tabs_dedupes_duplicate_restored_paths() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("shared.txt");
    std::fs::write(&path, "shared").unwrap();
    let first = disk_tab(&path, "first");
    let first_view = first.layout.active_view_id();
    let second = disk_tab(&path, "second");

    let mut manager = TabManager::new();
    manager.set_tabs(vec![first, second], 0);

    assert_eq!(manager.tabs.as_slice().len(), 1);
    assert_eq!(manager.find_tab_by_path(&path), Some((0, first_view)));
    assert_eq!(
        manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(|tab| tab.buffers())
            .filter(|buffer| buffer.path.as_deref() == Some(path.as_path()))
            .count(),
        1
    );
}
