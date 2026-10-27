use super::*;
use crate::app::{
    domain::{BufferState, WorkspaceTab},
    services::session_store::{CapturedClosedTab, SessionPersistRequest},
};

fn archived(dirty: bool) -> (tempfile::TempDir, SessionStore, PathBuf, String) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("file.txt");
    std::fs::write(&path, "original").unwrap();
    let mut buffer = BufferState::new("file.txt".into(), "original".into(), Some(path.clone()));
    buffer.sync_to_disk_state(Some(FileService::read_disk_state(&path).unwrap()));
    if dirty {
        buffer
            .document_mut()
            .replace_char_ranges_with_undo(
                &[(0..8, "unsaved".into())],
                crate::app::domain::CursorRange::default(),
                crate::app::domain::CursorRange::default(),
            )
            .unwrap();
        buffer.is_dirty = true;
    }
    let closed = CapturedClosedTab::capture(&WorkspaceTab::new(buffer), None);
    let token = closed.token.clone();
    let store = SessionStore::new(root.path().join("session"));
    let mut request = SessionPersistRequest::capture(&[], 0, 14.0, false);
    request.closed_tabs.push(closed);
    store.persist_request(request).unwrap();
    (root, store, path, token)
}

#[test]
fn unchanged_disk_restores_unsaved_text_and_undo() {
    let (_root, store, path, token) = archived(true);
    let reopened = store.reopen_archive(&token).unwrap();
    assert!(reopened.disk_paths.is_empty());
    let mut tab = reopened.tab.unwrap();
    assert_eq!(tab.active_buffer().text(), "unsaved");
    assert_eq!(tab.active_buffer().path.as_ref(), Some(&path));
    tab.active_buffer_mut()
        .document_mut()
        .undo_last_operation()
        .unwrap();
    assert_eq!(tab.active_buffer().text(), "original");
    assert_eq!(std::fs::read_to_string(path).unwrap(), "original");
}

#[test]
fn external_change_preserves_both_without_attaching_old_undo() {
    let (_root, store, path, token) = archived(true);
    // Same length: metadata-only comparison would not be sufficient.
    std::fs::write(&path, "external").unwrap();
    let reopened = store.reopen_archive(&token).unwrap();
    assert_eq!(reopened.disk_paths, vec![path.clone()]);
    let tab = reopened.tab.unwrap();
    assert_eq!(tab.active_buffer().text(), "unsaved");
    assert!(tab.active_buffer().path.is_none());
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
    assert_eq!(
        store.closed_tabs().unwrap().len(),
        1,
        "read does not consume archive"
    );
}

#[test]
fn changed_clean_buffer_in_a_mixed_split_uses_disk_and_forks_the_entire_archive() {
    use crate::app::domain::{CursorRange, SplitAxis};
    for missing_other in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("clean.txt");
        let other_path = root.path().join("other.txt");
        std::fs::write(&path, "original clean").unwrap();
        std::fs::write(&other_path, "original other").unwrap();
        let load = |path: &std::path::Path| {
            FileService::build_buffer_from_file_content(
                path,
                FileService::read_file(path).unwrap(),
                Some(FileService::read_disk_state(path).unwrap()),
            )
        };
        let mut tab = WorkspaceTab::new(load(&path));
        let mut other = load(&other_path);
        if !missing_other {
            other
                .document_mut()
                .replace_char_ranges_with_undo(
                    &[(0..0, "unsaved ".into())],
                    CursorRange::default(),
                    CursorRange::default(),
                )
                .unwrap();
            other.is_dirty = true;
        }
        tab.open_buffer_as_split(other, SplitAxis::Vertical, true, 0.5)
            .unwrap();
        let original_ids: Vec<_> = tab.buffers().map(|buffer| buffer.temp_id.clone()).collect();
        let closed = CapturedClosedTab::capture(&tab, None);
        let token = closed.token.clone();
        let store = SessionStore::new(root.path().join("session"));
        let mut request = SessionPersistRequest::capture(&[], 0, 14.0, false);
        request.closed_tabs.push(closed);
        store.persist_request(request).unwrap();
        std::fs::write(&path, "external clean").unwrap();
        if missing_other {
            std::fs::remove_file(&other_path).unwrap();
        }
        let reopened = store.reopen_archive(&token).unwrap();
        assert!(
            reopened.disk_paths.is_empty(),
            "disk file is already in the restored split"
        );
        let tab = reopened.tab.unwrap();
        assert_eq!(tab.layout.views().len(), 2);
        assert!(
            tab.buffers()
                .all(|buffer| !original_ids.contains(&buffer.temp_id))
        );
        let clean = tab
            .buffers()
            .find(|buffer| buffer.path.as_ref() == Some(&path))
            .unwrap();
        assert_eq!(clean.text(), "external clean");
        assert!(!clean.is_dirty);
        assert_eq!(clean.document().operation_undo_depth(), 0);
        let other = tab
            .buffers()
            .find(|buffer| buffer.name.starts_with("other.txt"))
            .unwrap();
        if missing_other {
            assert_eq!(other.text(), "original other");
            assert!(other.path.is_none());
            assert!(other.is_dirty);
        } else {
            assert_eq!(other.text(), "unsaved original other");
            assert_eq!(other.path.as_ref(), Some(&other_path));
            assert_eq!(other.document().operation_undo_depth(), 1);
        }
        store.persist(&[tab], 0, 14.0, false).unwrap();
        assert_eq!(store.closed_tabs().unwrap()[0].token, token);
        let archived = store.restore_closed_tab(&token).unwrap();
        assert_eq!(
            archived
                .buffers()
                .find(|buffer| buffer.name == "clean.txt")
                .unwrap()
                .text(),
            "original clean"
        );
    }
}

#[test]
fn clean_external_change_keeps_archive_and_missing_file_recovers_untitled() {
    let (_root, store, path, token) = archived(false);
    std::fs::write(&path, "external").unwrap();
    let reopened = store.reopen_archive(&token).unwrap();
    assert!(reopened.tab.is_none());
    assert_eq!(reopened.disk_paths, vec![path.clone()]);
    assert_eq!(store.closed_tabs().unwrap().len(), 1);
    std::fs::remove_file(path).unwrap();
    let tab = store.reopen_archive(&token).unwrap().tab.unwrap();
    assert!(tab.active_buffer().path.is_none());
    assert_eq!(tab.active_buffer().text(), "original");
}
