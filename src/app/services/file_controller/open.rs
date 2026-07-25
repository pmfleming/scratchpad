use super::FileController;
use super::support::{LoadedFile, OpenPathDecision};
use crate::app::app_state::{
    PendingBackgroundAction, PendingOpenTabsAction, ScratchpadApp, StatusDomain,
    workspace::accessors as workspace_accessors,
};
use crate::app::commands::{AppCommand, WorkspaceCommand};
use crate::app::diagnostics;
use crate::app::domain::{BufferState, WorkspaceTab};
use crate::app::services::background_io::{ColdFileShellResult, LoadedPathResult};
use crate::app::services::file_service::FileService;
use crate::app::utils::summarize_open_results;
use std::path::{Path, PathBuf};

#[cfg(not(test))]
const LAZY_OPEN_BATCH_THRESHOLD: usize = 2_048;
#[cfg(test)]
const LAZY_OPEN_BATCH_THRESHOLD: usize = 4;
#[cfg(not(test))]
const LARGE_FILE_STAGED_OPEN_BYTES: u64 = 128 * 1024 * 1024;
#[cfg(test)]
const LARGE_FILE_STAGED_OPEN_BYTES: u64 = 16;
#[cfg(not(test))]
const LARGE_FILE_FIRST_VISIBLE_BYTES: usize = 192 * 1024;
#[cfg(test)]
const LARGE_FILE_FIRST_VISIBLE_BYTES: usize = 8;

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

        Self::stage_large_file_previews(app, &pending_paths);
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

    fn stage_large_file_previews(app: &mut ScratchpadApp, paths: &[PathBuf]) {
        for path in paths {
            let Ok(disk_state) = FileService::read_disk_state(path) else {
                continue;
            };
            if disk_state.len < LARGE_FILE_STAGED_OPEN_BYTES {
                continue;
            }
            let Ok(window) =
                FileService::read_first_visible_window(path, LARGE_FILE_FIRST_VISIBLE_BYTES)
            else {
                continue;
            };
            let mut buffer = BufferState::with_encoding(
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                window.text,
                Some(path.clone()),
                window.encoding_name,
                window.has_bom,
            );
            buffer.sync_to_disk_state(Some(disk_state));
            buffer.mark_as_loading_preview();
            Self::mark_settings_buffer(app, &mut buffer);
            crate::app::app_state::workspace_controller::insert_new_tab_from_settings(
                app,
                WorkspaceTab::new(buffer),
            );
        }
        if paths.iter().any(|path| {
            app.tab_manager
                .find_tab_by_path(path)
                .and_then(|(index, view_id)| {
                    app.tab_manager.tabs.as_slice()[index]
                        .buffer_for_view(view_id)
                        .map(|buffer| buffer.is_loading_preview)
                })
                .unwrap_or(false)
        }) {
            Self::activate_workspace_after_staged_open(app);
            app.state.status.set_info_status_in_domain(
                StatusDomain::File,
                "Showing the first window while the large file finishes loading.",
            );
        }
    }

    fn open_selected_paths_as_cold_tabs(
        app: &mut ScratchpadApp,
        mut paths: Vec<PathBuf>,
        duplicate_count: usize,
    ) {
        let mut summary = OpenBatchSummary {
            duplicate_count,
            ..OpenBatchSummary::default()
        };
        let active_path = paths.pop().expect("large open batch is non-empty");
        Self::release_pending_open_path(app, &active_path);

        if Self::activate_existing_path(app, &active_path).is_some() {
            summary.record_outcome(OpenPathOutcome::AlreadyOpen);
        } else if let Ok(disk_state) = FileService::read_disk_state(&active_path) {
            let mut buffer = FileService::build_cold_file_shell(&active_path, Some(disk_state));
            Self::mark_settings_buffer(app, &mut buffer);
            let tab = WorkspaceTab::new(buffer);
            let cold_tab = crate::app::services::session_store::cold_tab_from_workspace_tab(&tab);

            app.reload_settings_before_workspace_change();
            crate::app::app_state::frame::begin_layout_transition(app);
            app.tab_manager.append_tab(tab);
            let active_index = app.tab_manager.active_tab_index;
            app.tab_manager.set_cold_session_tab(active_index, cold_tab);
            app.hydrate_tab_if_needed(active_index);
            let _ = Self::refresh_active_buffer_disk_state(app);
            Self::activate_workspace_after_staged_open(app);
            summary.record_outcome(OpenPathOutcome::Opened {
                artifact_warning: None,
            });
        } else {
            summary.record_outcome(OpenPathOutcome::Failed);
        }

        if paths.is_empty() {
            Self::finalize_cold_file_shell_open(app, summary);
        } else {
            app.queue_background_cold_file_shells(
                paths,
                PendingBackgroundAction::OpenTabs(PendingOpenTabsAction {
                    accumulator: summary,
                }),
            );
        }
    }

    pub(crate) fn apply_async_cold_file_shells_result(
        app: &mut ScratchpadApp,
        action: PendingOpenTabsAction,
        shells: Vec<ColdFileShellResult>,
    ) {
        let mut summary = action.accumulator;
        let mut tabs = Vec::with_capacity(shells.len());
        for shell in shells {
            Self::release_pending_open_path(app, &shell.path);
            if app.tab_manager.find_tab_by_path(&shell.path).is_some() {
                summary.record_outcome(OpenPathOutcome::AlreadyOpen);
                continue;
            }
            match shell.result {
                Ok((tab, cold_tab)) => {
                    tabs.push((tab, cold_tab));
                    summary.record_outcome(OpenPathOutcome::Opened {
                        artifact_warning: None,
                    });
                }
                Err(error) => {
                    diagnostics::record_io_error(
                        "open_file_shell",
                        Some(&shell.path),
                        "file_controller::open",
                        &std::io::Error::other(error),
                    );
                    summary.record_outcome(OpenPathOutcome::Failed);
                }
            }
        }
        app.tab_manager.append_cold_file_tabs(tabs);
        app.apply_current_tab_ordering();
        Self::finalize_cold_file_shell_open(app, summary);
    }

    fn activate_workspace_after_staged_open(app: &mut ScratchpadApp) {
        crate::app::app_state::settings_controller::activate_workspace_surface(app);
        crate::app::app_state::workspace::display_tabs::select_only_tab_slot(
            app,
            crate::app::app_state::workspace::display_tabs::active_tab_slot_index(app),
        );
        crate::app::app_state::workspace::accessors::request_focus_for_active_view(app);
        crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(app);
        crate::app::app_state::search_runtime::mark_search_dirty(app);
    }

    fn finalize_cold_file_shell_open(app: &mut ScratchpadApp, summary: OpenBatchSummary) {
        if summary.opened_count > 0 {
            // Keep the first-visible path non-blocking. The normal session manager
            // captures and persists the completed cold-shell batch on its background
            // lane instead of serializing 10,000 tabs on this UI callback.
            app.tab_manager.mark_session_dirty();
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
        if Self::replace_staged_large_file_preview(app, summary, &loaded.path, &loaded.result) {
            Self::release_pending_open_path(app, &loaded.path);
            return;
        }
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

    fn replace_staged_large_file_preview(
        app: &mut ScratchpadApp,
        summary: &mut OpenBatchSummary,
        path: &Path,
        result: &Result<BufferState, String>,
    ) -> bool {
        let Some((tab_index, view_id)) = app.tab_manager.find_tab_by_path(path) else {
            return false;
        };
        let is_preview = app.tab_manager.tabs.as_slice()[tab_index]
            .buffer_for_view(view_id)
            .is_some_and(|buffer| buffer.is_loading_preview);
        if !is_preview {
            return false;
        }

        match result {
            Ok(loaded) => {
                let deferred_refresh = Self::deferred_buffer_refresh(loaded);
                let LoadedFile {
                    artifact_warning,
                    mut buffer,
                    ..
                } = LoadedFile::from_buffer(loaded.clone());
                Self::mark_settings_buffer(app, &mut buffer);
                let tab = &mut app.tab_manager.tabs.as_mut_slice()[tab_index];
                let buffer_id = tab
                    .buffer_for_view(view_id)
                    .expect("staged preview buffer exists")
                    .id;
                tab.clear_view_state_for_buffer_replacement(buffer_id);
                tab.buffer_by_id_mut(buffer_id)
                    .expect("staged preview buffer exists")
                    .replace_from_loaded_buffer(buffer);
                app.tab_manager.set_active_tab_index_clamped(tab_index);
                Self::queue_deferred_buffer_refreshes(app, deferred_refresh);
                crate::app::app_state::search_runtime::mark_search_dirty(app);
                summary.record_outcome(OpenPathOutcome::Opened { artifact_warning });
            }
            Err(error) => {
                diagnostics::record_io_error(
                    "open_large_file",
                    Some(path),
                    "file_controller::open",
                    &std::io::Error::other(error.clone()),
                );
                app.tab_manager.close_tab_internal(tab_index);
                summary.record_outcome(OpenPathOutcome::Failed);
            }
        }
        true
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
