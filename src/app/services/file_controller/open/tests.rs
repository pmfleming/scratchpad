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
fn large_open_batch_hydrates_most_recent_tab_before_returning() {
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

    // The selected file is hydrated synchronously, while inactive metadata-only
    // shells are still being built on the path lane.
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
