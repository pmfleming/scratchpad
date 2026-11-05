use super::{FileController, LoadedPathResult, OpenBatchSummary, ScratchpadApp};
use crate::app::app_state::test_support::{self, wait_for_background_io};
use crate::app::domain::{BufferState, TabManager, WorkspaceTab};
use crate::app::services::file_service::FileService;

fn test_app(root: &std::path::Path, tabs: Vec<WorkspaceTab>) -> ScratchpadApp {
    let mut app = test_support::test_app(root);
    app.tab_manager = TabManager::for_test_tabs(tabs);
    crate::app::app_state::workspace::display_tabs::clear_tab_selection(&mut app);
    app
}

fn disk_buffer(path: &std::path::Path, text: &str) -> BufferState {
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut buffer = BufferState::new(name, text.to_owned(), Some(path.to_path_buf()));
    buffer.sync_to_disk_state(FileService::read_disk_state(path).ok());
    buffer
}

fn open_path_count(app: &ScratchpadApp, path: &std::path::Path) -> usize {
    app.tab_manager
        .tabs
        .as_slice()
        .iter()
        .flat_map(|tab| tab.buffers())
        .filter(|buffer| {
            buffer
                .path
                .as_deref()
                .is_some_and(|candidate| crate::app::paths_match(candidate, path))
        })
        .count()
}

#[test]
fn open_selected_paths_async_deduplicates_pending_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opened.txt");
    std::fs::write(&path, "opened").unwrap();
    let mut app = test_app(directory.path(), vec![WorkspaceTab::untitled()]);

    FileController::open_selected_paths_async(&mut app, vec![path.clone()]);
    FileController::open_selected_paths_async(&mut app, vec![path.clone()]);
    wait_for_background_io(&mut app);

    assert_eq!(open_path_count(&app, &path), 1);
    assert!(app.state.pending_open_file_paths.is_empty());
}

#[test]
fn large_file_stages_first_visible_window_before_background_hydration() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.txt");
    std::fs::write(&path, "first visible then fully hydrated").unwrap();
    let mut app = test_app(directory.path(), vec![WorkspaceTab::untitled()]);

    FileController::open_selected_paths_async(&mut app, vec![path]);

    let preview = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(preview.text(), "first vi");
    assert!(preview.is_loading_preview);

    wait_for_background_io(&mut app);
    let hydrated = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(hydrated.text(), "first visible then fully hydrated");
    assert!(!hydrated.is_loading_preview);
}

#[test]
fn cold_batch_reopening_both_paths_of_a_split_installs_each_document_once() {
    use crate::app::services::session_store::{CapturedClosedTab, SessionPersistRequest};
    let root = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..4)
        .map(|i| {
            let path = root.path().join(format!("cold_{i}.txt"));
            std::fs::write(&path, "one").unwrap();
            path
        })
        .collect();
    let mut app = test_app(root.path(), vec![WorkspaceTab::untitled()]);
    let load = |path: &std::path::Path| {
        FileService::build_buffer_from_file_content(
            path,
            FileService::read_file(path).unwrap(),
            Some(FileService::read_disk_state(path).unwrap()),
        )
    };
    let first = load(&paths[0]);
    let second = load(&paths[1]);
    let ids = [first.temp_id.clone(), second.temp_id.clone()];
    let mut split = WorkspaceTab::new(first);
    split
        .open_buffer_as_split(second, crate::app::domain::SplitAxis::Vertical, true, 0.5)
        .unwrap();
    let mut save = SessionPersistRequest::capture(app.tab_manager.tabs.as_slice(), 0, 16.0, false);
    save.closed_tabs
        .push(CapturedClosedTab::capture(&split, None));
    app.state
        .persistence
        .session_store
        .persist_request(save)
        .unwrap();
    FileController::open_paths(&mut app, paths);
    wait_for_background_io(&mut app);
    for id in ids {
        assert_eq!(
            app.tab_manager
                .tabs
                .as_slice()
                .iter()
                .flat_map(|tab| tab.buffers())
                .filter(|buffer| buffer.temp_id == id)
                .count(),
            1
        );
    }
    assert_eq!(
        app.state
            .persistence
            .pending_activity
            .iter()
            .filter(|event| event.kind
                == crate::app::services::session_store::activity::ActivityKind::Opened)
            .count(),
        4
    );
}

#[test]
fn closing_a_loading_preview_cancels_its_open_without_recording_lifecycle_activity() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("large.txt");
    std::fs::write(&path, "first visible then fully hydrated").unwrap();
    let mut app = test_app(root.path(), vec![WorkspaceTab::untitled()]);
    FileController::open_selected_paths_async(&mut app, vec![path.clone()]);
    assert!(
        app.tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .is_loading_preview
    );
    let index = app.tab_manager.active_tab_index;
    crate::app::app_state::workspace_controller::perform_close_tab_no_persist(&mut app, index);
    crate::app::app_state::test_support::wait_for_background_io(&mut app);
    wait_for_background_io(&mut app);
    assert!(app.tab_manager.find_tab_by_path(&path).is_none());
    assert!(app.state.persistence.pending_activity.is_empty());
    assert!(
        app.state
            .persistence
            .session_store
            .timeline_page(None, vec![], String::new())
            .unwrap()
            .rows
            .is_empty()
    );
}

#[test]
fn large_open_batch_hydrates_most_recent_tab_in_background() {
    let directory = tempfile::tempdir().unwrap();
    let paths = (0..super::LAZY_OPEN_BATCH_THRESHOLD)
        .map(|index| {
            let path = directory.path().join(format!("lazy_{index}.txt"));
            std::fs::write(&path, format!("loaded {index}")).unwrap();
            path
        })
        .collect::<Vec<_>>();
    let mut app = test_app(directory.path(), vec![WorkspaceTab::untitled()]);

    FileController::open_selected_paths_async(&mut app, paths.clone());

    // The callback queues I/O; selected text arrives when the background load completes.
    wait_for_background_io(&mut app);
    let active_index = app.tab_manager.active_tab_index;
    assert_eq!(
        app.tab_manager.tabs.as_slice()[active_index]
            .active_buffer()
            .text(),
        format!("loaded {}", paths.len() - 1)
    );

    wait_for_background_io(&mut app);
    assert!(app.state.pending_open_file_paths.is_empty());
    assert_eq!(app.tab_manager.cold_session_tabs().len(), paths.len() - 1);
}

#[test]
fn open_results_preserve_identity_release_reservations_and_handle_failures() {
    for (mode, succeeds, counts, tab_count, expected_text) in [
        ("new", true, (1, 0, 0), 2, Some("loaded")),
        ("new", false, (0, 0, 1), 1, None),
        ("existing", false, (0, 1, 0), 2, Some("original")),
        ("preview", true, (1, 0, 0), 2, Some("loaded")),
        ("preview", false, (0, 0, 1), 1, None),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("opened.txt");
        std::fs::write(&path, "loaded").unwrap();
        let mut app = test_app(directory.path(), vec![WorkspaceTab::untitled()]);
        let mut buffer = disk_buffer(&path, "original");
        buffer.is_loading_preview = mode == "preview";
        let original_id = buffer.id;
        if mode != "new" {
            let mut tab = WorkspaceTab::new(buffer);
            tab.split_active_view(crate::app::domain::SplitAxis::Vertical)
                .unwrap();
            let view_id = tab.layout.active_view_id;
            let (buffer, view) = tab.buffer_and_view_mut(view_id).unwrap();
            view.set_cursor_range_anchored(buffer, crate::app::domain::CursorRange::default());
            view.latest_display_snapshot_revision = Some(buffer.document_revision());
            app.tab_manager
                .set_tabs(vec![WorkspaceTab::untitled(), tab], 0);
        }
        app.state
            .pending_open_file_paths
            .insert(crate::app::CanonicalPathKey::from_path(&path));
        let mut loaded = disk_buffer(&path, "loaded");
        loaded.refresh_text_metadata_after_operation(None);
        let mut summary = OpenBatchSummary::default();
        FileController::process_open_tab_result(
            &mut app,
            &mut summary,
            LoadedPathResult {
                missing: false,
                retained_tab: None,
                disk_state: FileService::read_disk_state(&path).ok(),
                result: if succeeds {
                    Ok(loaded)
                } else {
                    Err("load failed".into())
                },
                path: path.clone(),
            },
        );
        assert_eq!(
            (
                summary.opened_count,
                summary.duplicate_count,
                summary.failure_count
            ),
            counts,
            "{mode}"
        );
        assert!(app.state.pending_open_file_paths.is_empty());
        assert_eq!(app.tab_manager.tabs.as_slice().len(), tab_count);
        wait_for_background_io(&mut app);
        if let Some(expected) = expected_text {
            assert_eq!(app.tab_manager.active_tab_index, 1);
            assert_eq!(open_path_count(&app, &path), 1);
            let tab = app.tab_manager.active_tab_mut().unwrap();
            assert_eq!(tab.active_buffer().text(), expected);
            assert!(!tab.active_buffer().is_loading_preview);
            assert!(!tab.active_buffer().text_metadata_refresh_needed());
            if mode != "new" {
                assert_eq!(tab.active_buffer().id, original_id);
                assert_eq!(tab.layout.views.len(), 2);
            }
            if mode == "preview" {
                for view in &mut tab.layout.views {
                    assert!(view.latest_display_snapshot_revision.is_none());
                    assert!(view.take_runtime_anchors_for_release().is_empty());
                }
            }
        }
    }
}

#[test]
fn persisting_large_file_preview_keeps_retained_identity_and_history() {
    use crate::app::services::session_manager;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("large.txt");
    let text = "first visible then fully hydrated";
    std::fs::write(&path, text).unwrap();
    let mut app = test_app(
        root.path(),
        vec![WorkspaceTab::new(disk_buffer(&path, text))],
    );
    let id = app
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .temp_id
        .clone();
    let store = app.state.persistence.session_store.clone();
    session_manager::persist_session_now(&mut app).unwrap();
    app.tab_manager.set_tabs(vec![], 0);
    session_manager::persist_session_now(&mut app).unwrap();
    FileController::open_selected_paths_async(&mut app, vec![path.clone()]);
    let preview_id = app
        .tab_manager
        .active_tab()
        .unwrap()
        .active_buffer()
        .temp_id
        .clone();
    session_manager::persist_session_now(&mut app).unwrap();
    let retained = store.reopen_path_with_claimed(&path, &[]).unwrap().unwrap();
    assert_eq!(retained.active_buffer().temp_id, id);
    assert_eq!(retained.active_buffer().name, "large.txt");
    assert!(
        !retained
            .active_buffer()
            .document()
            .history_entries()
            .is_empty()
    );
    assert!(
        store
            .reopen_target(
                crate::app::services::session_store::RetainedTarget::Document(preview_id),
                &[]
            )
            .is_err()
    );
    wait_for_background_io(&mut app);
    assert_eq!(
        app.tab_manager
            .active_tab()
            .unwrap()
            .active_buffer()
            .temp_id,
        id
    );
}

#[test]
fn unavailable_store_opens_single_files_and_large_batches_from_disk() {
    for restore_failed in [false, true] {
        for count in [1, super::LAZY_OPEN_BATCH_THRESHOLD] {
            let root = tempfile::tempdir().unwrap();
            std::fs::write(root.path().join("session"), "unavailable").unwrap();
            let paths: Vec<_> = (0..count)
                .map(|i| {
                    let path = root.path().join(format!("file_{i}.txt"));
                    std::fs::write(&path, format!("body {i}")).unwrap();
                    path
                })
                .collect();
            let mut app = test_app(root.path(), vec![WorkspaceTab::untitled()]);
            assert!(app.state.persistence.session_store.writes_blocked());
            app.state.persistence.session_restore_failed = restore_failed;
            FileController::open_selected_paths_async(&mut app, paths.clone());
            wait_for_background_io(&mut app);
            assert!(app.tab_manager.cold_session_tabs().is_empty());
            for (i, path) in paths.iter().enumerate() {
                let (index, view) = app.tab_manager.find_tab_by_path(path).unwrap();
                assert_eq!(
                    app.tab_manager.tabs.as_slice()[index]
                        .buffer_for_view(view)
                        .unwrap()
                        .text(),
                    format!("body {i}")
                );
            }
            assert!(app.state.pending_open_file_paths.is_empty());
        }
    }
}
