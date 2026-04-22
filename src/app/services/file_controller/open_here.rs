use super::FileController;
use super::support::LoadedFile;
use crate::app::app_state::{PendingBackgroundAction, PendingOpenHereAction, ScratchpadApp};
use crate::app::commands::AppCommand;
use crate::app::domain::{SplitAxis, ViewId, WorkspaceTab};
use crate::app::services::background_io::LoadedPathResult;
use crate::app::services::file_service::FileService;
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

    pub(super) fn open_selected_paths_here(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::prepare_to_open_paths(app);
        let snapshot = app.capture_transaction_snapshot();
        let affected_items = Self::affected_item_labels(&paths);
        let anchor_view_id = app
            .tabs()
            .get(app.active_tab_index())
            .map(|tab| tab.active_view_id);
        let mut pending_files = Vec::new();
        let summary = paths
            .into_iter()
            .fold(OpenHereBatchSummary::default(), |summary, path| {
                summary.record(Self::prepare_open_path_here(app, path, &mut pending_files))
            });

        let summary = Self::open_pending_files_here(app, anchor_view_id, pending_files)
            .into_iter()
            .fold(summary, |summary, outcome| summary.record(outcome));

        if summary.opened_count > 0 || summary.migrated_count > 0 {
            Self::rebalance_open_here_layout(app);
            let action_count = summary.opened_count + summary.migrated_count;
            let title = if action_count == 1 {
                "Open file here"
            } else {
                "Open files here"
            };
            app.record_transaction(title, affected_items, None, snapshot);
        }

        Self::apply_open_here_summary(app, summary);
    }

    pub(super) fn open_selected_paths_here_async(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::prepare_to_open_paths(app);
        let transaction_snapshot = app.capture_transaction_snapshot();
        let affected_items = Self::affected_item_labels(&paths);
        let anchor_view_id = app
            .tabs()
            .get(app.active_tab_index())
            .map(|tab| tab.active_view_id);
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
                let action_count = summary.opened_count + summary.migrated_count;
                let title = if action_count == 1 {
                    "Open file here"
                } else {
                    "Open files here"
                };
                app.record_transaction(title, affected_items, None, transaction_snapshot);
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
                affected_items,
                transaction_snapshot,
                anchor_view_id,
            }),
        );
    }

    fn prepare_open_path_here(
        app: &mut ScratchpadApp,
        path: PathBuf,
        pending_files: &mut Vec<LoadedFile>,
    ) -> OpenHerePathOutcome {
        if let Some(existing_path) = Self::find_existing_open_here_path(app, &path) {
            return Self::resolve_existing_open_here_path(app, path, existing_path);
        }

        Self::queue_open_here_path(app, path, pending_files)
    }

    fn prepare_open_path_here_async(
        app: &mut ScratchpadApp,
        path: PathBuf,
        pending_paths: &mut Vec<PathBuf>,
    ) -> OpenHerePathOutcome {
        if let Some(existing_path) = Self::find_existing_open_here_path(app, &path) {
            return Self::resolve_existing_open_here_path(app, path, existing_path);
        }

        pending_paths.push(path);
        OpenHerePathOutcome::Queued
    }

    fn open_pending_files_here(
        app: &mut ScratchpadApp,
        anchor_view_id: Option<ViewId>,
        pending_files: Vec<LoadedFile>,
    ) -> Vec<OpenHerePathOutcome> {
        let Some((pending_workspace, log_entries)) =
            Self::build_pending_open_here_workspace(app, pending_files)
        else {
            return Vec::new();
        };

        if !Self::attach_open_here_workspace(app, anchor_view_id, pending_workspace) {
            return Self::failed_open_here_outcomes(log_entries.len());
        }

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
            if let Some(existing_path) = Self::find_existing_open_here_path(app, &loaded.path) {
                outcomes.push(Self::resolve_existing_open_here_path(
                    app,
                    loaded.path,
                    existing_path,
                ));
                continue;
            }

            match loaded.result {
                Ok(file_content) => {
                    let mut loaded_file = LoadedFile::from_file_content(loaded.path, file_content);
                    Self::mark_settings_buffer(app, &mut loaded_file.buffer);
                    pending_files.push(loaded_file);
                }
                Err(_) => outcomes.push(OpenHerePathOutcome::Failed),
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
        let reflow_axis = app.workspace_reflow_axis;
        let rebalanced = if let Some(tab) = app.active_tab_mut() {
            tab.rebalance_views_equally_for_axis(reflow_axis)
        } else {
            false
        };

        if !rebalanced {
            return;
        }

        app.mark_session_dirty();
        let _ = app.persist_session_now();
    }

    fn find_existing_open_here_path(
        app: &ScratchpadApp,
        path: &Path,
    ) -> Option<ExistingOpenHerePath> {
        let target_index = app.active_tab_index();
        app.find_tab_by_path(path)
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
                app.handle_command(AppCommand::ActivateView { view_id });
                if app.is_settings_file_path(&path) {
                    app.mark_active_buffer_as_settings_file();
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
        let target_index = app.active_tab_index();
        app.handle_command(AppCommand::CombineTabIntoTab {
            source_index,
            target_index,
        });

        if let Some((current_index, current_view_id)) = app.find_tab_by_path(&path)
            && current_index == app.active_tab_index()
        {
            app.handle_command(AppCommand::ActivateView {
                view_id: current_view_id,
            });
            if app.is_settings_file_path(&path) {
                app.mark_active_buffer_as_settings_file();
            }
            return OpenHerePathOutcome::Migrated;
        }
        OpenHerePathOutcome::Failed
    }

    fn queue_open_here_path(
        app: &mut ScratchpadApp,
        path: PathBuf,
        pending_files: &mut Vec<LoadedFile>,
    ) -> OpenHerePathOutcome {
        match FileService::read_file(&path) {
            Ok(file_content) => {
                let mut loaded_file = LoadedFile::from_file_content(path, file_content);
                Self::mark_settings_buffer(app, &mut loaded_file.buffer);
                pending_files.push(loaded_file);
                OpenHerePathOutcome::Queued
            }
            Err(_) => OpenHerePathOutcome::Failed,
        }
    }

    fn build_pending_open_here_workspace(
        app: &mut ScratchpadApp,
        pending_files: Vec<LoadedFile>,
    ) -> Option<(WorkspaceTab, Vec<Option<String>>)> {
        let mut pending_iter = pending_files.into_iter();
        let first_file = pending_iter.next()?;
        let (buffer, log_entry) = first_file.into_parts();
        let mut pending_workspace = WorkspaceTab::new(buffer);
        let mut log_entries = vec![log_entry];

        for pending_file in pending_iter {
            if !Self::append_pending_file_to_workspace(
                app,
                &mut pending_workspace,
                &mut log_entries,
                pending_file,
            ) {
                return None;
            }
        }

        Some((pending_workspace, log_entries))
    }

    fn append_pending_file_to_workspace(
        app: &mut ScratchpadApp,
        pending_workspace: &mut WorkspaceTab,
        log_entries: &mut Vec<Option<String>>,
        pending_file: LoadedFile,
    ) -> bool {
        let (buffer, artifact_warning) = pending_file.into_parts();
        log_entries.push(artifact_warning);

        if pending_workspace
            .open_buffer_with_balanced_layout(buffer)
            .is_some()
        {
            true
        } else {
            app.set_error_status("Open Here failed to create a balanced tile layout.");
            false
        }
    }

    fn attach_open_here_workspace(
        app: &mut ScratchpadApp,
        anchor_view_id: Option<ViewId>,
        pending_workspace: WorkspaceTab,
    ) -> bool {
        let opened = if let Some(tab) = app.active_tab_mut() {
            if let Some(anchor_view_id) = anchor_view_id {
                let _ = tab.activate_view(anchor_view_id);
            }
            tab.combine_with_tab(pending_workspace, SplitAxis::Vertical, false, 0.5)
                .is_some()
        } else {
            false
        };

        if opened {
            true
        } else {
            app.set_error_status("Open Here failed to create a new tile layout.");
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
            let action_count = summary.opened_count + summary.migrated_count;
            let title = if action_count == 1 {
                "Open file here"
            } else {
                "Open files here"
            };
            app.record_transaction(
                title,
                action.affected_items,
                None,
                action.transaction_snapshot,
            );
        }

        Self::apply_open_here_summary(app, summary);
    }
}
