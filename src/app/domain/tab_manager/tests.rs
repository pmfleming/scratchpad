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

fn assert_buffer_slots(manager: &TabManager, expected: &[(BufferId, Option<usize>)]) {
    for (buffer_id, tab_index) in expected {
        assert_eq!(manager.tab_index_for_buffer(*buffer_id), *tab_index);
    }
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
    assert_buffer_slots(
        &manager,
        &[
            (first_id, Some(0)),
            (second_id, Some(1)),
            (split_id, Some(1)),
        ],
    );

    let restored = tab("restored.txt");
    let restored_id = restored.active_buffer().id;
    manager.append_restored_tab(restored);
    assert_eq!(manager.active_tab_index, 0);
    assert_buffer_slots(
        &manager,
        &[
            (first_id, Some(0)),
            (second_id, Some(1)),
            (split_id, Some(1)),
            (restored_id, Some(2)),
        ],
    );

    let appended = tab("appended.txt");
    let appended_id = appended.active_buffer().id;
    manager.append_tab(appended);
    assert_eq!(manager.active_tab_index, 3);
    assert_buffer_slots(
        &manager,
        &[
            (first_id, Some(0)),
            (second_id, Some(1)),
            (split_id, Some(1)),
            (restored_id, Some(2)),
            (appended_id, Some(3)),
        ],
    );

    let inserted = tab("inserted.txt");
    let inserted_id = inserted.active_buffer().id;
    manager.insert_tab(1, inserted);
    assert_buffer_slots(
        &manager,
        &[
            (first_id, Some(0)),
            (inserted_id, Some(1)),
            (second_id, Some(2)),
            (split_id, Some(2)),
            (restored_id, Some(3)),
            (appended_id, Some(4)),
        ],
    );

    assert!(manager.reorder_tab(2, 0));
    assert_buffer_slots(
        &manager,
        &[
            (second_id, Some(0)),
            (split_id, Some(0)),
            (first_id, Some(1)),
            (inserted_id, Some(2)),
            (restored_id, Some(3)),
            (appended_id, Some(4)),
        ],
    );

    manager.close_tab_internal(0);
    assert_buffer_slots(
        &manager,
        &[
            (second_id, None),
            (split_id, None),
            (first_id, Some(0)),
            (inserted_id, Some(1)),
            (restored_id, Some(2)),
            (appended_id, Some(3)),
        ],
    );
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
