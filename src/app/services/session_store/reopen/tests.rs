use super::{CanonicalPathKey, FileService, SessionStore};
use crate::app::{
    domain::{BufferState, WorkspaceTab},
    services::session_store::{CapturedClosedTab, SessionPersistRequest},
};
use std::path::PathBuf;

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
fn reopened_split_keeps_identities_and_retains_changed_files_previous_states() {
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
        let tab = reopened;
        assert_eq!(tab.layout.views().len(), 2);
        assert!(
            tab.buffers()
                .all(|buffer| original_ids.contains(&buffer.temp_id))
        );
        let clean = tab
            .buffers()
            .find(|buffer| buffer.path.as_ref() == Some(&path))
            .unwrap();
        assert_eq!(clean.text(), "external clean");
        assert!(!clean.is_dirty);
        assert_eq!(clean.document().operation_undo_depth(), 2);
        let mut document = clean.document().clone();
        document.undo_last_operation().unwrap();
        assert_eq!(document.extract_text(), "original clean");
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
            assert_eq!(other.document().operation_undo_depth(), 2);
        }
        store.persist(&[tab], 0, 14.0, false).unwrap();
        assert!(store.closed_tabs().unwrap().is_empty());
    }
}

#[test]
fn previously_observed_conflict_is_not_loaded_again_on_reopen() {
    let (root, store, path, token) = archived(true);
    let live = store.reopen_archive(&token).unwrap();
    let id = live.active_buffer().temp_id.clone();
    store.persist(&[live], 0, 14.0, false).unwrap();
    std::fs::write(&path, "conflict version").unwrap();
    let restored = store.load().unwrap().unwrap();
    let mut app = crate::app::app_state::test_support::test_app(root.path());
    app.wait_for_background_io_idle();
    app.state.persistence.session_store = store.clone();
    app.tab_manager.set_tabs(restored.tabs, 0);
    app.wait_for_background_io_idle();
    let restored = app.tab_manager.tabs.as_slice();
    let buffer = restored[0].active_buffer();
    assert_eq!(buffer.text(), "unsaved");
    assert_eq!(
        buffer.freshness,
        crate::app::domain::BufferFreshness::ConflictOnDisk
    );
    assert!(app.state.persistence.pending_activity.iter().any(
        |e| e.kind == crate::app::services::session_store::activity::ActivityKind::DiskConflict
    ));
    let closed = CapturedClosedTab::capture(&restored[0], None);
    let token = closed.token.clone();
    let mut save = SessionPersistRequest::capture(&[], 0, 14.0, false);
    save.closed_tabs.push(closed);
    store.persist_request(save).unwrap();
    let reopened = store.reopen_archive(&token).unwrap();
    assert_eq!(reopened.active_buffer().temp_id, id);
    assert_eq!(reopened.active_buffer().text(), "unsaved");
    assert!(reopened.active_buffer().is_dirty);
    // Returning to an older disk version is still an external edit relative to
    // the most recently observed version, even when it matches the saved baseline.
    std::fs::write(&path, "original").unwrap();
    let reopened = store.reopen_archive(&token).unwrap();
    assert_eq!(reopened.active_buffer().text(), "original");
    assert_eq!(
        reopened
            .active_buffer()
            .document()
            .history_entries()
            .last()
            .unwrap()
            .source,
        crate::app::domain::buffer::PieceSource::External
    );
}

#[test]
fn path_claimed_during_reopen_keeps_the_original_tree_without_the_staged_external_edit() {
    use crate::app::{
        app_state::{
            test_support::{test_app, wait_for_background_io},
            workspace::lifecycle::install_archive,
        },
        domain::{BufferState, WorkspaceTab},
        services::{file_service::FileService, session_manager},
    };
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("note.txt");
    std::fs::write(&path, "saved").unwrap();
    let mut app = test_app(root.path());
    wait_for_background_io(&mut app);
    let mut original =
        FileService::buffer_from_observed(&path, FileService::read_observed_file(&path).unwrap());
    original
        .document_mut()
        .replace_exact_with_history("unsaved");
    original.is_dirty = true;
    let id = original.temp_id.clone();
    let history = original.document().history_entries().len();
    app.tab_manager
        .set_tabs(vec![WorkspaceTab::new(original)], 0);
    session_manager::persist_session_now(&mut app).unwrap();
    let store = app.state.persistence.session_store.clone();
    app.tab_manager.set_tabs(vec![], 0);
    session_manager::persist_session_now(&mut app).unwrap();
    std::fs::write(&path, "external").unwrap();
    let reopened = store
        .reopen_target(super::RetainedTarget::Document(id.clone()), &[])
        .unwrap();
    let owner = WorkspaceTab::new(BufferState::new(
        "owner".into(),
        "owner".into(),
        Some(path.clone()),
    ));
    app.tab_manager.set_tabs(vec![owner], 0);
    assert!(install_archive(&mut app, reopened));
    let b = app
        .tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|t| t.buffers())
        .find(|b| b.temp_id == id)
        .unwrap();
    assert_eq!(b.text(), "unsaved");
    assert!(b.path.is_none());
    assert_eq!(b.document().history_entries().len(), history);
    assert_eq!(std::fs::read_to_string(path).unwrap(), "external");
}

#[test]
fn retained_reopen_preserves_history_through_disk_changes_claims_failures_and_retry() {
    use crate::app::domain::buffer::PieceSource;
    for dirty in [false, true] {
        let (_root, store, path, token) = archived(dirty);
        let retained = if dirty { "unsaved" } else { "original" };
        let mut tab = store.reopen_archive(&token).unwrap();
        let id = tab.active_buffer().temp_id.clone();
        assert_eq!(tab.active_buffer().text(), retained);
        assert_eq!(tab.active_buffer().path.as_ref(), Some(&path));
        if dirty {
            tab.active_buffer_mut()
                .document_mut()
                .undo_last_operation()
                .unwrap();
            assert_eq!(tab.active_buffer().text(), "original");
        }

        // Same length: modification metadata alone cannot detect the change.
        std::fs::write(&path, "external").unwrap();
        let mut tab = store.reopen_archive(&token).unwrap();
        assert_eq!(tab.active_buffer().temp_id, id);
        assert_eq!(tab.active_buffer().path.as_ref(), Some(&path));
        assert_eq!(tab.active_buffer().text(), "external");
        assert_eq!(
            tab.active_buffer()
                .document()
                .history_entries()
                .last()
                .unwrap()
                .source,
            PieceSource::External
        );
        tab.active_buffer_mut()
            .document_mut()
            .undo_last_operation()
            .unwrap();
        assert_eq!(tab.active_buffer().text(), retained);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "external");
        assert_eq!(
            store.closed_tabs().unwrap().len(),
            1,
            "reads retain the archive"
        );

        let tab = store
            .reopen_archive_with_claimed(&token, &[CanonicalPathKey::from_path(&path)])
            .unwrap();
        assert!(tab.active_buffer().path.is_none());
        assert_eq!(tab.active_buffer().text(), retained);
        assert!(
            !tab.active_buffer()
                .document()
                .history_entries()
                .iter()
                .any(|entry| entry.source == PieceSource::External)
        );

        let (shell, cold) = store.cold_tab_for_path(&path).unwrap().unwrap();
        assert_eq!(shell.active_buffer().temp_id, id);
        assert_eq!(shell.active_buffer().text(), "");
        std::fs::write(&path, "new disk version").unwrap();
        let (mut tab, _) = store.restore_cold_session_tab(cold).unwrap();
        assert_eq!(tab.active_buffer().temp_id, id);
        assert_eq!(tab.active_buffer().text(), "new disk version");
        tab.active_buffer_mut()
            .document_mut()
            .undo_last_operation()
            .unwrap();
        assert_eq!(tab.active_buffer().text(), retained);

        // A directory gives a non-NotFound error even under a privileged user.
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert!(store.reopen_archive(&token).is_err());
        assert!(store.reopen_path_with_claimed(&path, &[]).is_err());
        let (_, cold) = store.cold_tab_for_path(&path).unwrap().unwrap();
        assert!(store.restore_cold_session_tab(cold).is_err());
        assert_eq!(store.closed_tabs().unwrap().len(), 1);
        std::fs::remove_dir(&path).unwrap();
        std::fs::write(&path, "original").unwrap();
        let tab = store.reopen_path_with_claimed(&path, &[]).unwrap().unwrap();
        assert_eq!(tab.active_buffer().temp_id, id);
        assert_eq!(tab.active_buffer().path.as_ref(), Some(&path));
        assert_eq!(tab.active_buffer().text(), retained);

        std::fs::remove_file(&path).unwrap();
        let tab = store.reopen_archive(&token).unwrap();
        assert!(tab.active_buffer().path.is_none());
        assert_eq!(tab.active_buffer().text(), retained);
        assert!(tab.active_buffer().is_dirty);
    }
}
