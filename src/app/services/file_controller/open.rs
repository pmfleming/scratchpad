use super::FileController;
use super::support::LoadedFile;
use crate::app::app_state::{PendingBackgroundAction, PendingOpenTabsAction, ScratchpadApp};
use crate::app::commands::AppCommand;
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

        if target_index >= app.tabs().len() {
            app.set_error_status(format!(
                "Startup /addto:index:{} target does not exist.",
                target_index + 1
            ));
            return;
        }

        app.handle_command(AppCommand::ActivateTab {
            index: target_index,
        });
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

        if target_index >= app.tabs().len() {
            app.set_error_status(format!(
                "Startup /addto:index:{} target does not exist.",
                target_index + 1
            ));
            return;
        }

        app.handle_command(AppCommand::ActivateTab {
            index: target_index,
        });
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
            } else {
                pending_paths.push(path);
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
        if let Some((index, view_id)) = app.find_tab_by_path(path) {
            app.handle_command(AppCommand::ActivateTab { index });
            app.handle_command(AppCommand::ActivateView { view_id });
            if app.is_settings_file_path(path) {
                app.mark_active_buffer_as_settings_file();
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
                app.tab_manager_mut().append_tab(WorkspaceTab::new(buffer));
                app.ensure_active_tab_slot_selected();
                Self::queue_deferred_buffer_refreshes(app, deferred_refresh);
                app.mark_search_dirty();
                app.request_focus_for_active_view();
                summary.record_outcome(OpenPathOutcome::Opened { artifact_warning });
            }
            Err(_) => {
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
            let _ = app.persist_session_now();
        }

        Self::apply_open_summary(app, summary);
    }
}
