use super::{FileController, LoadedPathResult, OpenBatchSummary, ScratchpadApp};
use crate::app::domain::{BufferState, TabManager, WorkspaceTab};
use crate::app::services::file_service::FileService;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::SettingsStore;
use crate::app::startup::StartupOptions;

fn test_app(root: &std::path::Path, tabs: Vec<WorkspaceTab>) -> ScratchpadApp {
    let mut app = ScratchpadApp::with_stores_and_startup(
        SessionStore::new(root.join("session")),
        SettingsStore::new(root.join("settings")),
        StartupOptions::default(),
    );
    app.set_session_persist_on_drop(false);
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
fn process_open_tab_result_adds_loaded_file_as_active_tab() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opened.txt");
    std::fs::write(&path, "opened").unwrap();
    let mut app = test_app(
        directory.path(),
        vec![WorkspaceTab::new(BufferState::new(
            "start.txt".to_owned(),
            String::new(),
            None,
        ))],
    );
    let mut summary = OpenBatchSummary::default();

    FileController::process_open_tab_result(
        &mut app,
        &mut summary,
        LoadedPathResult {
            path: path.clone(),
            disk_state: FileService::read_disk_state(&path).ok(),
            result: Ok(disk_buffer(&path, "opened")),
        },
    );

    assert_eq!(summary.opened_count, 1);
    assert_eq!(summary.duplicate_count, 0);
    assert_eq!(app.tab_manager.tabs.as_slice().len(), 2);
    assert_eq!(app.tab_manager.active_tab_index, 1);
    assert_eq!(
        app.tab_manager.tabs.as_slice()[1]
            .active_buffer()
            .path
            .as_deref(),
        Some(path.as_path())
    );
}

#[test]
fn open_selected_paths_async_deduplicates_pending_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("opened.txt");
    std::fs::write(&path, "opened").unwrap();
    let mut app = test_app(
        directory.path(),
        vec![WorkspaceTab::new(BufferState::new(
            "start.txt".to_owned(),
            String::new(),
            None,
        ))],
    );

    FileController::open_selected_paths_async(&mut app, vec![path.clone()]);
    FileController::open_selected_paths_async(&mut app, vec![path.clone()]);
    app.wait_for_background_io_idle();

    assert_eq!(open_path_count(&app, &path), 1);
    assert!(app.state.pending_open_file_paths.is_empty());
}

#[test]
fn large_file_stages_first_visible_window_before_background_hydration() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("large.txt");
    std::fs::write(&path, "first visible then fully hydrated").unwrap();
    let mut app = test_app(
        directory.path(),
        vec![WorkspaceTab::new(BufferState::new(
            "start.txt".to_owned(),
            String::new(),
            None,
        ))],
    );

    FileController::open_selected_paths_async(&mut app, vec![path]);

    let preview = app.tab_manager.active_tab().unwrap().active_buffer();
    assert_eq!(preview.text(), "first vi");
    assert!(preview.is_loading_preview);

    app.wait_for_background_io_idle();
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
    let mut app = test_app(
        directory.path(),
        vec![WorkspaceTab::new(BufferState::new(
            "start.txt".to_owned(),
            String::new(),
            None,
        ))],
    );

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

    app.wait_for_background_io_idle();
    assert!(app.state.pending_open_file_paths.is_empty());
    assert_eq!(app.tab_manager.cold_session_tabs().len(), paths.len() - 1);
}

#[test]
fn process_open_tab_result_reuses_existing_tab_for_duplicate_path() {
    let directory = tempfile::tempdir().unwrap();
    let first_path = directory.path().join("first.txt");
    let second_path = directory.path().join("second.txt");
    std::fs::write(&first_path, "first").unwrap();
    std::fs::write(&second_path, "second").unwrap();
    let mut app = test_app(
        directory.path(),
        vec![
            WorkspaceTab::new(disk_buffer(&first_path, "first")),
            WorkspaceTab::new(disk_buffer(&second_path, "second")),
        ],
    );
    let target_view = app.tab_manager.tabs.as_slice()[1].layout.active_view_id;
    let mut summary = OpenBatchSummary::default();

    FileController::process_open_tab_result(
        &mut app,
        &mut summary,
        LoadedPathResult {
            path: second_path.clone(),
            disk_state: FileService::read_disk_state(&second_path).ok(),
            result: Err("should not be read for duplicates".to_owned()),
        },
    );

    assert_eq!(summary.duplicate_count, 1);
    assert_eq!(app.tab_manager.tabs.as_slice().len(), 2);
    assert_eq!(app.tab_manager.active_tab_index, 1);
    assert_eq!(
        app.tab_manager.tabs.as_slice()[1].layout.active_view_id,
        target_view
    );
}

#[test]
fn process_open_tab_result_records_failed_load_without_adding_tab() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing.txt");
    let mut app = test_app(
        directory.path(),
        vec![WorkspaceTab::new(BufferState::new(
            "start.txt".to_owned(),
            String::new(),
            None,
        ))],
    );
    let mut summary = OpenBatchSummary::default();

    FileController::process_open_tab_result(
        &mut app,
        &mut summary,
        LoadedPathResult {
            path,
            disk_state: None,
            result: Err("not found".to_owned()),
        },
    );

    assert_eq!(summary.failure_count, 1);
    assert_eq!(app.tab_manager.tabs.as_slice().len(), 1);
}
