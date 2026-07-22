use super::FileController;
use super::support::{LoadedFile, OpenPathDecision};
use crate::app::app_state::{
    PendingBackgroundAction, PendingOpenTabsAction, ScratchpadApp, StatusDomain,
    workspace::accessors as workspace_accessors,
};
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::diagnostics;
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::background_io::LoadedPathResult;
use crate::app::services::file_service::FileService;
use crate::app::utils::summarize_open_results;
use std::path::{Path, PathBuf};

#[cfg(not(test))]
const LAZY_OPEN_BATCH_THRESHOLD: usize = 2_048;
#[cfg(test)]
const LAZY_OPEN_BATCH_THRESHOLD: usize = 4;

macro_rules! external_open_methods {
    ($($vis:vis fn $name:ident => ($log_prefix:literal, $open_action:path);)+) => {
        $(
            $vis fn $name(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
                Self::handle_external_paths(app, paths, $log_prefix, $open_action);
            }
        )+
    };
}

macro_rules! blocking_open_methods {
    ($($vis:vis fn $name:ident => $open_action:path;)+) => {
        $(
            $vis fn $name(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
                $open_action(app, paths);
                app.wait_for_background_io_idle();
            }
        )+
    };
}

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
        Self::handle_open_dialog(app, crate::app::platform_file::OpenFileDialogKind::OpenFile);
    }

    pub fn open_file_here(app: &mut ScratchpadApp) {
        Self::handle_open_dialog(
            app,
            crate::app::platform_file::OpenFileDialogKind::OpenFileHere,
        );
    }

    external_open_methods! {
        pub fn open_paths => (
            "Open requested for",
            Self::open_selected_paths_background_blocking
        );
        pub fn open_paths_async => (
            "Background open requested for",
            Self::open_selected_paths_async
        );
        pub fn open_external_paths => (
            "Startup open requested for",
            Self::open_selected_paths_background_blocking
        );
        pub fn open_external_paths_async => (
            "Background open requested for",
            Self::open_selected_paths_async
        );
        pub fn open_external_paths_here => (
            "Startup workspace-open requested for",
            Self::open_selected_paths_here_background_blocking
        );
        pub fn open_external_paths_here_async => (
            "Background workspace-open requested for",
            Self::open_selected_paths_here_async
        );
    }

    pub fn open_external_paths_into_tab(
        app: &mut ScratchpadApp,
        target_index: usize,
        paths: Vec<PathBuf>,
    ) {
        Self::open_external_paths_into_tab_with(
            app,
            target_index,
            paths,
            Self::open_external_paths_here,
        );
    }

    pub fn open_external_paths_into_tab_async(
        app: &mut ScratchpadApp,
        target_index: usize,
        paths: Vec<PathBuf>,
    ) {
        Self::open_external_paths_into_tab_with(
            app,
            target_index,
            paths,
            Self::open_external_paths_here_async,
        );
    }

    fn open_external_paths_into_tab_with(
        app: &mut ScratchpadApp,
        target_index: usize,
        paths: Vec<PathBuf>,
        open_paths: fn(&mut ScratchpadApp, Vec<PathBuf>),
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

        crate::app::commands::handle_command(
            app,
            AppCommand::Workspace(WorkspaceCommand::ActivateTab {
                index: target_index,
            }),
        );
        open_paths(app, paths);
    }

    blocking_open_methods! {
        fn open_selected_paths_background_blocking => Self::open_selected_paths_async;
        pub(super) fn open_selected_paths_here_background_blocking => Self::open_selected_paths_here_async;
    }

    pub(super) fn open_selected_paths_async(app: &mut ScratchpadApp, paths: Vec<PathBuf>) {
        Self::prepare_to_open_paths(app);
        let batch = Self::prepare_open_batch(
            app,
            paths,
            |app, path| {
                if Self::activate_existing_path(app, &path).is_some() {
                    OpenPathDecision::Resolved(OpenPathOutcome::AlreadyOpen)
                } else {
                    OpenPathDecision::Unresolved(path)
                }
            },
            || OpenPathOutcome::AlreadyOpen,
        );
        let duplicate_count = batch.resolved.len();
        let pending_paths = batch.pending_paths;

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

        if pending_paths.len() >= LAZY_OPEN_BATCH_THRESHOLD {
            Self::open_selected_paths_as_cold_tabs(app, pending_paths, duplicate_count);
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

    fn open_selected_paths_as_cold_tabs(
        app: &mut ScratchpadApp,
        paths: Vec<PathBuf>,
        duplicate_count: usize,
    ) {
        let mut summary = OpenBatchSummary {
            duplicate_count,
            ..OpenBatchSummary::default()
        };

        let mut tabs_to_add = Vec::new();
        for path in paths {
            Self::release_pending_open_path(app, &path);
            if Self::activate_existing_path(app, &path).is_some() {
                summary.record_outcome(OpenPathOutcome::AlreadyOpen);
                continue;
            }
            let Ok(disk_state) = FileService::read_disk_state(&path) else {
                summary.record_outcome(OpenPathOutcome::Failed);
                continue;
            };

            let mut buffer = BufferState::new(
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                String::new(),
                Some(path),
            );
            buffer.sync_to_disk_state(Some(disk_state));
            Self::mark_settings_buffer(app, &mut buffer);

            let tab = WorkspaceTab::new(buffer);
            let cold_tab = crate::app::services::session_store::cold_tab_from_workspace_tab(&tab);
            tabs_to_add.push((tab, cold_tab));
            summary.record_outcome(OpenPathOutcome::Opened {
                artifact_warning: None,
            });
        }

        if !tabs_to_add.is_empty() {
            app.reload_settings_before_workspace_change();
            crate::app::app_state::frame::begin_layout_transition(app);
            for (tab, cold_tab) in tabs_to_add {
                app.tab_manager.tabs.push(tab);
                let index = app.tab_manager.tabs.len() - 1;
                app.tab_manager.set_cold_session_tab(index, cold_tab);
            }
            app.tab_manager
                .set_active_tab_index_clamped(app.tab_manager.tabs.len().saturating_sub(1));
            app.tab_manager.rebuild_buffer_tab_index();
            app.apply_current_tab_ordering();

            // The newly selected tab is a metadata-only shell. Hydrate it before the
            // first paint so large batches open on the most recently selected file
            // instead of briefly presenting an empty editor, then run the same disk
            // refresh used by ordinary tab activation.
            let active_index = app.tab_manager.active_tab_index;
            app.hydrate_tab_if_needed(active_index);
            let _ = Self::refresh_active_buffer_disk_state(app);

            crate::app::app_state::settings_controller::activate_workspace_surface(app);
            crate::app::app_state::workspace::display_tabs::select_only_tab_slot(
                app,
                crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app),
            );
            crate::app::app_state::workspace::accessors::request_focus_for_active_view(app);
            crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
            crate::app::app_state::search_runtime::mark_search_dirty(app);
        }
        if summary.opened_count > 0 {
            app.tab_manager.mark_session_dirty();
            let _ = crate::app::app_state::workspace::accessors::persist_session_now(app);
        }
        Self::apply_open_summary(app, summary);
    }

    pub(super) fn activate_existing_path(app: &mut ScratchpadApp, path: &Path) -> Option<String> {
        if let Some((index, view_id)) = app.tab_manager.find_tab_by_path(path) {
            crate::app::commands::handle_command(
                app,
                AppCommand::Workspace(WorkspaceCommand::ActivateTab { index }),
            );
            crate::app::commands::handle_command(
                app,
                AppCommand::Workspace(WorkspaceCommand::ActivateView { view_id }),
            );
            if crate::app::app_state::settings_state::is_settings_file_path(app, path) {
                crate::app::app_state::settings_state::mark_active_buffer_as_settings_file(app);
            }
            Some(path.file_name().map_or_else(
                || path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ))
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
                debug_assert!(
                    buffer
                        .path_key
                        .as_ref()
                        .and_then(|key| app.tab_manager.path_owner(key))
                        .is_none(),
                    "loaded file collided with existing path owner"
                );
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
mod tests;
