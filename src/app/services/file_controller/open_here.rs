use super::FileController;
use super::support::{DeferredBufferRefresh, LoadedFile};
use crate::app::app_state::{
    PendingBackgroundAction, PendingOpenHereAction, ScratchpadApp, StatusDomain,
};
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::diagnostics;
use crate::app::domain::{SplitAxis, ViewId, WorkspaceTab};
use crate::app::services::background_io::LoadedPathResult;
use std::path::{Path, PathBuf};

mod summary;

use summary::OpenHereBatchSummary;

enum OpenHerePathOutcome {
    Opened { artifact_warning: Option<String> },
    Migrated,
    AlreadyInCurrentTab,
    Queued,
    Failed,
}

enum ExistingOpenHerePath {
    AlreadyInCurrentTab { view_id: ViewId },
    NeedsMigration { source_index: usize },
}

impl FileController {
    pub fn open_external_paths_here_async(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::handle_external_paths(
            app,
            paths,
            "Background workspace-open requested for",
            Self::open_selected_paths_here_async,
        );
    }

    pub(super) fn open_selected_paths_here_background_blocking(
        app: &mut ScratchpadApp,
        paths: Vec<PathBuf>,
    ) {
        Self::open_selected_paths_here_async(app, paths);
        app.wait_for_background_io_idle();
    }

    pub(super) fn open_selected_paths_here_async(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::prepare_to_open_paths(app);
        let anchor_view_id = app
            .tab_manager
            .tabs
            .as_slice()
            .get(app.tab_manager.active_tab_index)
            .map(|tab| tab.layout.active_view_id());
        let mut pending_paths = Vec::new();
        let mut summary = OpenHereBatchSummary::default();
        let mut already_here_count = 0;
        let mut migrated_count = 0;
        let mut failure_count = 0;

        for path in paths {
            let outcome = Self::prepare_open_path_here_async(app, path, &mut pending_paths);
            match outcome {
                OpenHerePathOutcome::Migrated => migrated_count += 1,
                OpenHerePathOutcome::AlreadyInCurrentTab => already_here_count += 1,
                OpenHerePathOutcome::Failed => failure_count += 1,
                OpenHerePathOutcome::Opened { .. } | OpenHerePathOutcome::Queued => {}
            }
            summary = summary.record(outcome);
        }

        if pending_paths.is_empty() {
            if summary.opened_count > 0 || summary.migrated_count > 0 {
                Self::rebalance_open_here_layout(app);
            }
            Self::apply_open_here_summary(app, summary);
            return;
        }

        app.queue_background_path_loads(
            pending_paths,
            PendingBackgroundAction::OpenHere(PendingOpenHereAction {
                already_here_count,
                migrated_count,
                failure_count,
                anchor_view_id,
            }),
        );
    }

    fn prepare_open_path_here_async(
        app: &mut ScratchpadApp,
        path: PathBuf,
        pending_paths: &mut Vec<PathBuf>,
    ) -> OpenHerePathOutcome {
        if let Some(existing_path) = Self::find_existing_open_here_path(app, &path) {
            return Self::resolve_existing_open_here_path(app, path, existing_path);
        }

        if Self::reserve_pending_open_path(app, &path) {
            pending_paths.push(path);
            OpenHerePathOutcome::Queued
        } else {
            OpenHerePathOutcome::AlreadyInCurrentTab
        }
    }

    fn open_pending_files_here(
        app: &mut ScratchpadApp,
        anchor_view_id: Option<ViewId>,
        pending_files: Vec<LoadedFile>,
    ) -> Vec<OpenHerePathOutcome> {
        let Some((pending_workspace, log_entries, deferred_refreshes)) =
            Self::build_pending_open_here_workspace(app, pending_files)
        else {
            return Vec::new();
        };

        if !Self::attach_open_here_workspace(app, anchor_view_id, pending_workspace) {
            return Self::failed_open_here_outcomes(log_entries.len());
        }

        Self::queue_deferred_buffer_refreshes(app, deferred_refreshes);
        Self::log_open_here_success(app, log_entries)
    }

    fn open_loaded_files_here(
        app: &mut ScratchpadApp,
        anchor_view_id: Option<ViewId>,
        loaded_paths: Vec<LoadedPathResult>,
    ) -> Vec<OpenHerePathOutcome> {
        let mut pending_files = Vec::new();
        let mut outcomes = Vec::new();

        for loaded in loaded_paths {
            Self::release_pending_open_path(app, &loaded.path);
            if let Some(existing_path) = Self::find_existing_open_here_path(app, &loaded.path) {
                outcomes.push(Self::resolve_existing_open_here_path(
                    app,
                    loaded.path,
                    existing_path,
                ));
                continue;
            }

            match loaded.result {
                Ok(buffer) => {
                    let mut loaded_file = LoadedFile::from_buffer(buffer);
                    Self::mark_settings_buffer(app, &mut loaded_file.buffer);
                    pending_files.push(loaded_file);
                }
                Err(error) => {
                    diagnostics::record_io_error(
                        "open_here",
                        Some(&loaded.path),
                        "file_controller::open_here",
                        &error,
                    );
                    outcomes.push(OpenHerePathOutcome::Failed);
                }
            }
        }

        outcomes.extend(Self::open_pending_files_here(
            app,
            anchor_view_id,
            pending_files,
        ));
        outcomes
    }

    fn rebalance_open_here_layout(app: &mut ScratchpadApp) {
        let reflow_axis = app.state.workspace_reflow_axis;
        let rebalanced = if let Some(tab) = app.tab_manager.active_tab_mut() {
            tab.rebalance_views_equally_for_axis(reflow_axis)
        } else {
            false
        };

        if !rebalanced {
            return;
        }

        app.tab_manager.mark_session_dirty();
        let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
    }

    fn find_existing_open_here_path(
        app: &ScratchpadApp,
        path: &Path,
    ) -> Option<ExistingOpenHerePath> {
        let target_index = app.tab_manager.active_tab_index;
        app.tab_manager
            .find_tab_by_path(path)
            .map(|(existing_tab_index, view_id)| {
                if existing_tab_index == target_index {
                    ExistingOpenHerePath::AlreadyInCurrentTab { view_id }
                } else {
                    ExistingOpenHerePath::NeedsMigration {
                        source_index: existing_tab_index,
                    }
                }
            })
    }

    fn resolve_existing_open_here_path(
        app: &mut ScratchpadApp,
        path: PathBuf,
        existing_path: ExistingOpenHerePath,
    ) -> OpenHerePathOutcome {
        match existing_path {
            ExistingOpenHerePath::AlreadyInCurrentTab { view_id } => {
                crate::app::commands::handle_command(
                    app,
                    AppCommand::Workspace(WorkspaceCommand::ActivateView { view_id }),
                );
                if crate::app::app_state::settings_state::is_settings_file_path(app, &path) {
                    crate::app::app_state::settings_state::mark_active_buffer_as_settings_file(app);
                }
                OpenHerePathOutcome::AlreadyInCurrentTab
            }
            ExistingOpenHerePath::NeedsMigration { source_index } => {
                Self::migrate_open_here_path(app, path, source_index)
            }
        }
    }

    fn migrate_open_here_path(
        app: &mut ScratchpadApp,
        path: PathBuf,
        source_index: usize,
    ) -> OpenHerePathOutcome {
        let target_index = app.tab_manager.active_tab_index;
        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::CombineTabIntoTab {
                source_index,
                target_index,
            }),
        );

        if let Some((current_index, current_view_id)) = app.tab_manager.find_tab_by_path(&path)
            && current_index == app.tab_manager.active_tab_index
        {
            crate::app::commands::handle_command(
                app,
                AppCommand::Workspace(WorkspaceCommand::ActivateView {
                    view_id: current_view_id,
                }),
            );
            if crate::app::app_state::settings_state::is_settings_file_path(app, &path) {
                crate::app::app_state::settings_state::mark_active_buffer_as_settings_file(app);
            }
            return OpenHerePathOutcome::Migrated;
        }
        OpenHerePathOutcome::Failed
    }

    fn build_pending_open_here_workspace(
        app: &mut ScratchpadApp,
        pending_files: Vec<LoadedFile>,
    ) -> Option<(
        WorkspaceTab,
        Vec<Option<String>>,
        Vec<DeferredBufferRefresh>,
    )> {
        let mut pending_iter = pending_files.into_iter();
        let first_file = pending_iter.next()?;
        let (buffer, log_entry) = first_file.into_parts();
        let mut deferred_refreshes = Vec::new();
        if let Some(refresh) = Self::deferred_buffer_refresh(&buffer) {
            deferred_refreshes.push(refresh);
        }
        let mut pending_workspace = WorkspaceTab::new(buffer);
        let mut log_entries = vec![log_entry];

        for pending_file in pending_iter {
            if !Self::append_pending_file_to_workspace(
                app,
                &mut pending_workspace,
                &mut log_entries,
                &mut deferred_refreshes,
                pending_file,
            ) {
                return None;
            }
        }

        Some((pending_workspace, log_entries, deferred_refreshes))
    }

    fn append_pending_file_to_workspace(
        app: &mut ScratchpadApp,
        pending_workspace: &mut WorkspaceTab,
        log_entries: &mut Vec<Option<String>>,
        deferred_refreshes: &mut Vec<DeferredBufferRefresh>,
        pending_file: LoadedFile,
    ) -> bool {
        let (buffer, artifact_warning) = pending_file.into_parts();
        if let Some(refresh) = Self::deferred_buffer_refresh(&buffer) {
            deferred_refreshes.push(refresh);
        }
        log_entries.push(artifact_warning);

        if pending_workspace
            .open_buffer_with_balanced_layout(buffer)
            .is_some()
        {
            true
        } else {
            app.state.status.set_error_status_in_domain(
                StatusDomain::Layout,
                "Could not add those files to this tab layout.",
            );
            false
        }
    }

    fn attach_open_here_workspace(
        app: &mut ScratchpadApp,
        anchor_view_id: Option<ViewId>,
        pending_workspace: WorkspaceTab,
    ) -> bool {
        let opened = if let Some(tab) = app.tab_manager.active_tab_mut() {
            if let Some(anchor_view_id) = anchor_view_id {
                let _ = tab.activate_view(anchor_view_id);
            }
            tab.combine_with_tab(pending_workspace, SplitAxis::Vertical, false, 0.5)
                .is_some()
        } else {
            false
        };

        if opened {
            app.tab_manager.rebuild_buffer_tab_index();
            true
        } else {
            app.state.status.set_error_status_in_domain(
                StatusDomain::Layout,
                "Could not add those files to this tab layout.",
            );
            false
        }
    }

    fn log_open_here_success(
        _app: &mut ScratchpadApp,
        log_entries: Vec<Option<String>>,
    ) -> Vec<OpenHerePathOutcome> {
        log_entries
            .into_iter()
            .map(|artifact_warning| OpenHerePathOutcome::Opened { artifact_warning })
            .collect()
    }

    fn failed_open_here_outcomes(file_count: usize) -> Vec<OpenHerePathOutcome> {
        (0..file_count)
            .map(|_| OpenHerePathOutcome::Failed)
            .collect()
    }

    fn apply_open_here_summary(app: &mut ScratchpadApp, summary: OpenHereBatchSummary) {
        Self::apply_open_status(
            app,
            summary.status_message(),
            summary.failure_count > 0 || summary.artifact_count > 0,
            summary.log_message(),
        );
    }

    pub(crate) fn apply_async_open_here_result(
        app: &mut ScratchpadApp,
        action: PendingOpenHereAction,
        results: Vec<LoadedPathResult>,
    ) {
        let mut summary = OpenHereBatchSummary::default();
        for _ in 0..action.already_here_count {
            summary = summary.record(OpenHerePathOutcome::AlreadyInCurrentTab);
        }
        for _ in 0..action.migrated_count {
            summary = summary.record(OpenHerePathOutcome::Migrated);
        }
        for _ in 0..action.failure_count {
            summary = summary.record(OpenHerePathOutcome::Failed);
        }
        summary = Self::open_loaded_files_here(app, action.anchor_view_id, results)
            .into_iter()
            .fold(summary, |summary, outcome| summary.record(outcome));

        if summary.opened_count > 0 || summary.migrated_count > 0 {
            Self::rebalance_open_here_layout(app);
        }

        Self::apply_open_here_summary(app, summary);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        FileController, LoadedPathResult, OpenHerePathOutcome, ScratchpadApp, SplitAxis,
        WorkspaceTab,
    };
    use crate::app::domain::{BufferState, TabManager};
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

    #[test]
    fn open_loaded_files_here_adds_each_loaded_file_to_active_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let one = directory.path().join("one.txt");
        let two = directory.path().join("two.txt");
        std::fs::write(&one, "one").unwrap();
        std::fs::write(&two, "two").unwrap();
        let mut app = test_app(
            directory.path(),
            vec![WorkspaceTab::new(BufferState::new(
                "base.txt".to_owned(),
                "base".to_owned(),
                None,
            ))],
        );
        let anchor_view_id = app.tab_manager.tabs.as_slice()[0].layout.active_view_id();

        let outcomes = FileController::open_loaded_files_here(
            &mut app,
            Some(anchor_view_id),
            vec![
                LoadedPathResult {
                    path: one.clone(),
                    disk_state: FileService::read_disk_state(&one).ok(),
                    result: Ok(disk_buffer(&one, "one")),
                },
                LoadedPathResult {
                    path: two.clone(),
                    disk_state: FileService::read_disk_state(&two).ok(),
                    result: Ok(disk_buffer(&two, "two")),
                },
            ],
        );

        assert_eq!(outcomes.len(), 2);
        assert!(matches!(outcomes[0], OpenHerePathOutcome::Opened { .. }));
        assert!(matches!(outcomes[1], OpenHerePathOutcome::Opened { .. }));
        assert_eq!(app.tab_manager.tabs.as_slice().len(), 1);
        assert_eq!(app.tab_manager.tabs.as_slice()[0].buffers().count(), 3);
        assert!(app.tab_manager.find_tab_by_path(&one).is_some());
        assert!(app.tab_manager.find_tab_by_path(&two).is_some());
    }

    #[test]
    fn open_selected_paths_here_reuses_path_already_in_current_workspace() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        let mut tab = WorkspaceTab::new(disk_buffer(&first, "first"));
        let second_view = tab
            .open_buffer_as_split(
                disk_buffer(&second, "second"),
                SplitAxis::Vertical,
                true,
                0.5,
            )
            .unwrap();
        let mut app = test_app(directory.path(), vec![tab]);

        FileController::open_selected_paths_here_async(&mut app, vec![second.clone()]);

        assert_eq!(app.tab_manager.tabs.as_slice().len(), 1);
        assert_eq!(app.tab_manager.tabs.as_slice()[0].buffers().count(), 2);
        assert_eq!(
            app.tab_manager.tabs.as_slice()[0].layout.active_view_id(),
            second_view
        );
    }

    #[test]
    fn open_selected_paths_here_migrates_existing_path_from_another_tab() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.txt");
        let second = directory.path().join("second.txt");
        std::fs::write(&first, "first").unwrap();
        std::fs::write(&second, "second").unwrap();
        let mut app = test_app(
            directory.path(),
            vec![
                WorkspaceTab::new(disk_buffer(&first, "first")),
                WorkspaceTab::new(disk_buffer(&second, "second")),
            ],
        );

        FileController::open_selected_paths_here_async(&mut app, vec![second.clone()]);

        assert_eq!(app.tab_manager.tabs.as_slice().len(), 1);
        assert_eq!(app.tab_manager.tabs.as_slice()[0].buffers().count(), 2);
        assert_eq!(
            app.tab_manager
                .find_tab_by_path(&second)
                .map(|(index, _)| index),
            Some(0)
        );
    }
}
