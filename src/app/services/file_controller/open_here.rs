use super::{FileController, LoadedFile, PendingOpenLogEntry};
use crate::app::app_state::ScratchpadApp;
use crate::app::commands::AppCommand;
use crate::app::domain::{SplitAxis, ViewId, WorkspaceTab};
use crate::app::logging::LogLevel;
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
    pub(super) fn open_selected_paths_here(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::prepare_to_open_paths(app);
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
        }

        Self::apply_open_here_summary(app, summary);
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

    fn rebalance_open_here_layout(app: &mut ScratchpadApp) {
        let rebalanced = if let Some(tab) = app.active_tab_mut() {
            tab.rebalance_views_equally()
        } else {
            false
        };

        if !rebalanced {
            app.log_event(
                LogLevel::Error,
                "Open Here could not rebalance the workspace layout equally.",
            );
            return;
        }

        app.mark_session_dirty();
        let _ = app.persist_session_now();
        app.log_event(
            LogLevel::Info,
            format!(
                "Rebalanced Open Here layout in current workspace to equal tile shares (views={}).",
                app.tabs()
                    .get(app.active_tab_index())
                    .map(|tab| tab.views.len())
                    .unwrap_or_default()
            ),
        );
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
                app.log_event(
                    LogLevel::Info,
                    format!(
                        "Open Here found file already in current workspace, activating view: {}",
                        path.display()
                    ),
                );
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
            app.log_event(
                LogLevel::Info,
                format!(
                    "Open Here migrated existing tab into current workspace: {}",
                    path.display()
                ),
            );
            return OpenHerePathOutcome::Migrated;
        }

        app.log_event(
            LogLevel::Error,
            format!(
                "Open Here could not migrate existing tab into current workspace: {}",
                path.display()
            ),
        );
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
            Err(error) => {
                app.log_event(
                    LogLevel::Error,
                    format!("Open Here failed for {}: {error}", path.display()),
                );
                OpenHerePathOutcome::Failed
            }
        }
    }

    fn build_pending_open_here_workspace(
        app: &mut ScratchpadApp,
        pending_files: Vec<LoadedFile>,
    ) -> Option<(WorkspaceTab, Vec<PendingOpenLogEntry>)> {
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
        log_entries: &mut Vec<PendingOpenLogEntry>,
        pending_file: LoadedFile,
    ) -> bool {
        let (buffer, log_entry) = pending_file.into_parts();
        let failed_path = log_entry.path_display.clone();
        log_entries.push(log_entry);

        if pending_workspace
            .open_buffer_with_balanced_layout(buffer)
            .is_some()
        {
            true
        } else {
            app.log_event(
                LogLevel::Error,
                format!("Open Here could not build balanced layout for {failed_path}"),
            );
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
            app.log_event(
                LogLevel::Error,
                "Open Here could not insert the balanced tile layout into the current workspace.",
            );
            app.set_error_status("Open Here failed to create a new tile layout.");
            false
        }
    }

    fn log_open_here_success(
        app: &mut ScratchpadApp,
        log_entries: Vec<PendingOpenLogEntry>,
    ) -> Vec<OpenHerePathOutcome> {
        let tab_index = app.active_tab_index();
        let tab_description = app.describe_active_tab();

        log_entries
            .into_iter()
            .map(|entry| {
                app.log_event(
                    LogLevel::Info,
                    format!(
                        "Opened file into balanced current workspace layout at tab index {tab_index}: {tab_description} [encoding={}, bom={}, artifact_status={}] from {}",
                        entry.encoding,
                        entry.has_bom,
                        entry.artifact_summary.unwrap_or_else(|| "none".to_owned()),
                        entry.path_display
                    ),
                );
                OpenHerePathOutcome::Opened {
                    artifact_warning: entry.artifact_warning,
                }
            })
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
}
