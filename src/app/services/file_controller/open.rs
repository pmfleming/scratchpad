use super::FileController;
use super::support::LoadedFile;
use crate::app::app_state::{
    PendingBackgroundAction, PendingOpenTabsAction, ScratchpadApp, StatusDomain,
    workspace::accessors as workspace_accessors,
};
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::diagnostics;
use crate::app::domain::WorkspaceTab;
use crate::app::services::background_io::LoadedPathResult;
use crate::app::utils::summarize_open_results;
use std::path::{Path, PathBuf};

pub(crate) enum OpenPathOutcome {
    Opened { artifact_warning: Option<String> },
    AlreadyOpen,
    Failed,
}

#[derive(Default)]
pub(crate) struct OpenBatchSummary {
    pub(crate) opened_count: usize,
    pub(crate) duplicate_count: usize,
    pub(crate) failure_count: usize,
    pub(crate) artifact_count: usize,
    pub(crate) last_artifact_warning: Option<String>,
}

impl OpenBatchSummary {
    pub(crate) fn record_outcome(&mut self, outcome: OpenPathOutcome) {
        match outcome {
            OpenPathOutcome::Opened { artifact_warning } => {
                self.opened_count += 1;
                if let Some(warning) = artifact_warning {
                    self.artifact_count += 1;
                    self.last_artifact_warning = Some(warning);
                }
            }
            OpenPathOutcome::AlreadyOpen => {
                self.duplicate_count += 1;
            }
            OpenPathOutcome::Failed => {
                self.failure_count += 1;
            }
        }
    }

    fn log_message(&self) -> String {
        format!(
            "Open file batch completed: opened={}, duplicates={}, failed={}, artifacts={}",
            self.opened_count, self.duplicate_count, self.failure_count, self.artifact_count
        )
    }
}

impl FileController {
    pub fn open_file(app: &mut ScratchpadApp) {
        Self::handle_open_dialog(app, "Open file dialog", Self::open_selected_paths_async);
    }

    pub fn open_file_here(app: &mut ScratchpadApp) {
        Self::handle_open_dialog(
            app,
            "Open Here dialog",
            Self::open_selected_paths_here_async,
        );
    }

    pub fn open_paths(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::handle_external_paths(
            app,
            paths,
            "Open requested for",
            Self::open_selected_paths_background_blocking,
        );
    }

    pub fn open_paths_async(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::handle_external_paths(
            app,
            paths,
            "Background open requested for",
            Self::open_selected_paths_async,
        );
    }

    pub fn open_external_paths(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::handle_external_paths(
            app,
            paths,
            "Startup open requested for",
            Self::open_selected_paths_background_blocking,
        );
    }

    pub fn open_external_paths_async(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::handle_external_paths(
            app,
            paths,
            "Background open requested for",
            Self::open_selected_paths_async,
        );
    }

    pub fn open_external_paths_here(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::handle_external_paths(
            app,
            paths,
            "Startup workspace-open requested for",
            Self::open_selected_paths_here_background_blocking,
        );
    }

    pub fn open_external_paths_into_tab(
        app: &mut ScratchpadApp,
        target_index: usize,
        paths: Vec<PathBuf>,
    ) {
        if paths.is_empty() {
            return;
        }

        if target_index >= app.tab_manager.tabs.as_slice().len() {
            app.state.status.set_error_status_with_detail(
                StatusDomain::Session,
                "Could not add startup files to that tab.",
                format!(
                    "Startup /addto:index:{} target does not exist.",
                    target_index + 1
                ),
            );
            return;
        }

        app.handle_command(AppCommand::Workspace(WorkspaceCommand::ActivateTab {
            index: target_index,
        }));
        Self::open_external_paths_here(app, paths);
    }

    pub fn open_external_paths_into_tab_async(
        app: &mut ScratchpadApp,
        target_index: usize,
        paths: Vec<PathBuf>,
    ) {
        if paths.is_empty() {
            return;
        }

        if target_index >= app.tab_manager.tabs.as_slice().len() {
            app.state.status.set_error_status_with_detail(
                StatusDomain::Session,
                "Could not add startup files to that tab.",
                format!(
                    "Startup /addto:index:{} target does not exist.",
                    target_index + 1
                ),
            );
            return;
        }

        app.handle_command(AppCommand::Workspace(WorkspaceCommand::ActivateTab {
            index: target_index,
        }));
        Self::open_external_paths_here_async(app, paths);
    }

    fn open_selected_paths_background_blocking(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::open_selected_paths_async(app, paths);
        app.wait_for_background_io_idle();
    }

    pub(super) fn open_selected_paths_async(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::prepare_to_open_paths(app);
        let mut duplicate_count = 0;
        let mut pending_paths = Vec::new();

        for path in paths {
            if Self::activate_existing_path(app, &path).is_some() {
                duplicate_count += 1;
            } else if Self::reserve_pending_open_path(app, &path) {
                pending_paths.push(path);
            } else {
                duplicate_count += 1;
            }
        }

        if pending_paths.is_empty() {
            Self::apply_open_summary(
                app,
                OpenBatchSummary {
                    duplicate_count,
                    ..OpenBatchSummary::default()
                },
            );
            return;
        }

        app.queue_background_path_loads_streaming(
            pending_paths,
            PendingBackgroundAction::OpenTabs(PendingOpenTabsAction {
                accumulator: OpenBatchSummary {
                    duplicate_count,
                    ..OpenBatchSummary::default()
                },
            }),
        );
    }

    fn activate_existing_path(app: &mut ScratchpadApp, path: &Path) -> Option<String> {
        if let Some((index, view_id)) = app.tab_manager.find_tab_by_path(path) {
            app.handle_command(AppCommand::Workspace(WorkspaceCommand::ActivateTab {
                index,
            }));
            app.handle_command(AppCommand::Workspace(WorkspaceCommand::ActivateView {
                view_id,
            }));
            if crate::app::app_state::settings_state::is_settings_file_path(app, path) {
                crate::app::app_state::settings_state::mark_active_buffer_as_settings_file(app);
            }
            Some(
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string()),
            )
        } else {
            None
        }
    }

    fn apply_open_summary(app: &mut ScratchpadApp, summary: OpenBatchSummary) {
        Self::apply_open_status(
            app,
            summarize_open_results(
                summary.opened_count,
                summary.duplicate_count,
                summary.failure_count,
                summary.artifact_count,
                summary.last_artifact_warning.clone(),
            ),
            summary.failure_count > 0 || summary.artifact_count > 0,
            summary.log_message(),
        );
    }

    pub(crate) fn apply_async_open_tabs_result(
        app: &mut ScratchpadApp,
        action: PendingOpenTabsAction,
        results: Vec<LoadedPathResult>,
    ) {
        let mut summary = action.accumulator;
        for loaded in results {
            Self::process_open_tab_result(app, &mut summary, loaded);
        }

        Self::finalize_open_tabs(
            app,
            PendingOpenTabsAction {
                accumulator: summary,
            },
        );
    }

    /// Streaming entry point: consume one `LoadedPathResult` from a partial
    /// `PathsLoaded` message. Borrows the accumulator on the action so the
    /// caller can keep the action in `pending_background_actions` for further
    /// partials.
    pub(crate) fn process_open_tab_result(
        app: &mut ScratchpadApp,
        summary: &mut OpenBatchSummary,
        loaded: LoadedPathResult,
    ) {
        Self::release_pending_open_path(app, &loaded.path);
        if Self::activate_existing_path(app, &loaded.path).is_some() {
            summary.record_outcome(OpenPathOutcome::AlreadyOpen);
            return;
        }

        match loaded.result {
            Ok(buffer) => {
                let deferred_refresh = Self::deferred_buffer_refresh(&buffer);
                let LoadedFile {
                    artifact_warning,
                    mut buffer,
                    ..
                } = LoadedFile::from_buffer(buffer);
                Self::mark_settings_buffer(app, &mut buffer);
                crate::app::app_state::workspace_controller::insert_new_tab_from_settings(
                    app,
                    WorkspaceTab::new(buffer),
                );
                crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(
                    app,
                );
                Self::queue_deferred_buffer_refreshes(app, deferred_refresh);
                crate::app::app_state::search_runtime::mark_search_dirty(app);
                workspace_accessors::request_focus_for_active_view(app);
                summary.record_outcome(OpenPathOutcome::Opened { artifact_warning });
            }
            Err(error) => {
                diagnostics::record_io_error(
                    "open_file",
                    Some(&loaded.path),
                    "file_controller::open",
                    &error,
                );
                summary.record_outcome(OpenPathOutcome::Failed);
            }
        }
    }

    /// Finalize a streaming open, persist session,
    /// emit summary status. Called after the last `PathsLoaded` partial
    /// (`is_partial: false`) is processed.
    pub(crate) fn finalize_open_tabs(app: &mut ScratchpadApp, action: PendingOpenTabsAction) {
        let summary = action.accumulator;
        if summary.opened_count > 0 {
            let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
        }

        Self::apply_open_summary(app, summary);
    }
}

#[cfg(test)]
mod tests {
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
        app.tab_manager = TabManager {
            tabs,
            active_tab_index: 0,
            pending_action: None,
            session_dirty: false,
            pending_scroll_to_active: false,
            buffer_tab_index: Default::default(),
            cold_session_tabs: Default::default(),
        };
        app.tab_manager.rebuild_buffer_tab_index();
        crate::app::app_state::workspace::display_tabs::clear_tab_selection(app);
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
}
