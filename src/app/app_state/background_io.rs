use super::{
    PendingBackgroundAction, PendingEncodingComplianceAction, PendingSavePathAction,
    PendingSessionPersistAction, PendingStartupRestoreAction, PendingTextMetadataAction,
    ScratchpadApp,
};
use crate::app::diagnostics;
use crate::app::services::background_io::{
    BackgroundIoRequest, BackgroundIoResult, BackgroundIoSendError, LoadedPathResult,
    PathLoadRequest,
};
use crate::app::services::file_controller::FileController;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionPersistRequest;
use crate::app::startup::StartupOptions;
use eframe::egui;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BACKGROUND_IO_POLL_INTERVAL: Duration = Duration::from_millis(16);

mod fallback;
mod queue;

impl ScratchpadApp {
    pub(crate) fn poll_background_io(&mut self, ctx: &egui::Context) {
        self.drain_background_io_results();

        if !self.io.pending_background_actions.is_empty() {
            ctx.request_repaint_after(BACKGROUND_IO_POLL_INTERVAL);
        }
    }

    pub fn drain_background_io_results(&mut self) {
        while let Ok(result) = self.io.background_io_rx.try_recv() {
            self.apply_background_io_result(result);
        }
    }

    pub fn wait_for_background_io_idle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            self.drain_background_io_results();
            if self.io.pending_background_actions.is_empty() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.drain_background_io_results();
    }

    fn apply_background_io_result(&mut self, result: BackgroundIoResult) {
        match result {
            BackgroundIoResult::PathsLoaded {
                request_id,
                results,
                is_partial,
            } => self.apply_paths_loaded_result(request_id, results, is_partial),
            BackgroundIoResult::PathSaved {
                request_id,
                path,
                disk_state,
                result,
            } => self.apply_path_saved_result(request_id, path, disk_state, result),
            BackgroundIoResult::SessionRestored { request_id, result } => {
                self.apply_session_restored_result(request_id, result);
            }
            BackgroundIoResult::SessionRestoreStarted {
                request_id,
                active_tab_index,
                legacy_settings,
            } => {
                self.apply_session_restore_started_result(
                    request_id,
                    active_tab_index,
                    legacy_settings,
                );
            }
            BackgroundIoResult::SessionTabRestored { request_id, tab } => {
                self.apply_session_tab_restored_result(request_id, *tab);
            }
            BackgroundIoResult::SessionPersisted { request_id, result } => {
                self.apply_session_persisted_result(request_id, result);
            }
            BackgroundIoResult::TextMetadataRefreshed {
                request_id,
                buffer_id,
                revision,
                result,
            } => {
                self.apply_text_metadata_refreshed_result(request_id, buffer_id, revision, result);
            }
            BackgroundIoResult::EncodingComplianceRefreshed {
                request_id,
                buffer_id,
                revision,
                result,
            } => {
                self.apply_encoding_compliance_refreshed_result(
                    request_id, buffer_id, revision, result,
                );
            }
        }
    }

    fn apply_paths_loaded_result(
        &mut self,
        request_id: u64,
        results: Vec<LoadedPathResult>,
        is_partial: bool,
    ) {
        if is_partial {
            self.apply_partial_paths_loaded_result(request_id, results);
            return;
        }

        match self.io.pending_background_actions.remove(&request_id) {
            Some(PendingBackgroundAction::OpenTabs(action)) => {
                FileController::apply_async_open_tabs_result(self, action, results);
            }
            Some(PendingBackgroundAction::OpenHere(action)) => {
                FileController::apply_async_open_here_result(self, action, results);
            }
            Some(PendingBackgroundAction::ReloadBuffer(action)) => {
                FileController::apply_async_reload_buffer_result(self, action, results);
            }
            Some(PendingBackgroundAction::ReopenWithEncoding(action)) => {
                FileController::apply_async_reopen_with_encoding_result(self, action, results);
            }
            Some(PendingBackgroundAction::SavePath(_)) => {}
            Some(PendingBackgroundAction::StartupRestoreCompare(action)) => {
                self.apply_async_startup_restore_compare_result(action, results);
            }
            Some(PendingBackgroundAction::StartupRestore(_))
            | Some(PendingBackgroundAction::PersistSession(_))
            | Some(PendingBackgroundAction::RefreshTextMetadata(_))
            | Some(PendingBackgroundAction::RefreshEncodingCompliance(_))
            | None => {}
        }
    }

    fn apply_path_saved_result(
        &mut self,
        request_id: u64,
        path: PathBuf,
        disk_state: Option<crate::app::domain::DiskFileState>,
        result: Result<(), String>,
    ) {
        let Some(PendingBackgroundAction::SavePath(action)) =
            self.io.pending_background_actions.remove(&request_id)
        else {
            return;
        };
        FileController::apply_async_save_result(self, action, path, disk_state, result);
    }

    fn apply_partial_paths_loaded_result(
        &mut self,
        request_id: u64,
        results: Vec<LoadedPathResult>,
    ) {
        let Some(PendingBackgroundAction::OpenTabs(action)) =
            self.io.pending_background_actions.get_mut(&request_id)
        else {
            return;
        };

        let mut summary = std::mem::take(&mut action.accumulator);
        for loaded in results {
            FileController::process_open_tab_result(self, &mut summary, loaded);
        }
        if let Some(PendingBackgroundAction::OpenTabs(action)) =
            self.io.pending_background_actions.get_mut(&request_id)
        {
            action.accumulator = summary;
        }
    }

    fn apply_session_restored_result(
        &mut self,
        request_id: u64,
        result: Result<Option<crate::app::services::session_store::RestoredSession>, String>,
    ) {
        let Some(PendingBackgroundAction::StartupRestore(action)) =
            self.io.pending_background_actions.remove(&request_id)
        else {
            return;
        };
        self.apply_runtime_startup_restore_result(action, result);
    }

    fn apply_session_restore_started_result(
        &mut self,
        request_id: u64,
        active_tab_index: usize,
        legacy_settings: crate::app::services::settings_store::AppSettings,
    ) {
        let apply_legacy_settings = {
            let Some(PendingBackgroundAction::StartupRestore(action)) =
                self.io.pending_background_actions.get_mut(&request_id)
            else {
                return;
            };
            action.restore_started = true;
            !action.loaded_from_settings
        };
        if apply_legacy_settings {
            self.apply_settings(legacy_settings);
            let _ = self.persist_settings_now();
        }
        if !self.tabs().is_empty() {
            self.tab_manager_mut().active_tab_index =
                active_tab_index.min(self.tabs().len().saturating_sub(1));
        }
    }

    fn apply_session_tab_restored_result(
        &mut self,
        request_id: u64,
        tab: crate::app::domain::WorkspaceTab,
    ) {
        let first_streamed_tab = {
            let Some(PendingBackgroundAction::StartupRestore(action)) =
                self.io.pending_background_actions.get_mut(&request_id)
            else {
                return;
            };
            let first_streamed_tab = action.streamed_tab_count == 0;
            action.streamed_tab_count += 1;
            first_streamed_tab
        };

        if first_streamed_tab {
            self.tab_manager_mut().set_tabs(vec![tab], 0);
        } else {
            self.tab_manager_mut().tabs.push(tab);
            self.rebuild_buffer_tab_index();
        }
        self.ensure_active_tab_slot_selected();
        self.refresh_startup_restore_conflicts();
        self.mark_search_dirty();
    }

    fn apply_session_persisted_result(&mut self, request_id: u64, result: Result<(), String>) {
        let Some(PendingBackgroundAction::PersistSession(_)) =
            self.io.pending_background_actions.remove(&request_id)
        else {
            return;
        };
        match result {
            Ok(()) => {
                self.last_session_persist = Instant::now();
            }
            Err(error) => {
                diagnostics::record_background_failure(
                    "session_persist_result",
                    "app_state::background_io",
                    &error,
                    [("request_id", request_id.to_string())],
                );
                self.mark_session_dirty();
                self.report_session_save_failed(error);
            }
        }
    }

    fn apply_text_metadata_refreshed_result(
        &mut self,
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        result: Result<
            (
                usize,
                crate::app::domain::TextArtifactSummary,
                crate::app::domain::TextFormatMetadata,
            ),
            String,
        >,
    ) {
        let Some(PendingBackgroundAction::RefreshTextMetadata(_)) =
            self.io.pending_background_actions.remove(&request_id)
        else {
            return;
        };
        if let Ok((line_count, artifact_summary, format)) = result
            && let Some(buffer) = self
                .tabs_mut()
                .iter_mut()
                .find_map(|tab| tab.buffer_by_id_mut(buffer_id))
        {
            buffer.apply_text_metadata_refresh(revision, line_count, artifact_summary, format);
        }
    }

    fn apply_encoding_compliance_refreshed_result(
        &mut self,
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        result: Result<bool, String>,
    ) {
        let Some(PendingBackgroundAction::RefreshEncodingCompliance(_)) =
            self.io.pending_background_actions.remove(&request_id)
        else {
            return;
        };
        if let Ok(has_non_compliant_characters) = result
            && let Some(buffer) = self
                .tabs_mut()
                .iter_mut()
                .find_map(|tab| tab.buffer_by_id_mut(buffer_id))
        {
            buffer.apply_encoding_compliance_refresh(revision, has_non_compliant_characters);
        }
    }

    fn apply_runtime_startup_restore_result(
        &mut self,
        action: PendingStartupRestoreAction,
        result: Result<Option<crate::app::services::session_store::RestoredSession>, String>,
    ) {
        let mut restored_session = false;
        let legacy_settings = match result {
            Ok(Some(restored)) if action.streamed_tab_count > 0 => {
                restored_session = true;
                if !self.tabs().is_empty() {
                    self.tab_manager_mut().active_tab_index = restored
                        .active_tab_index
                        .min(self.tabs().len().saturating_sub(1));
                }
                if let Some(status) = restored.restore_status.as_ref() {
                    match status.level {
                        crate::app::services::session_store::RestoreStatusLevel::Info => self
                            .set_info_status_in_domain(
                                crate::app::app_state::StatusDomain::Session,
                                status.message.clone(),
                            ),
                        crate::app::services::session_store::RestoreStatusLevel::Warning => self
                            .set_warning_status_in_domain(
                                crate::app::app_state::StatusDomain::Session,
                                status.message.clone(),
                            ),
                    }
                }
                (!action.loaded_from_settings && !action.restore_started)
                    .then_some(restored.legacy_settings)
            }
            Ok(Some(restored)) => {
                restored_session = true;
                Some(session_manager::apply_restored_session(self, restored))
            }
            Ok(None) => None,
            Err(error) => {
                diagnostics::record_background_failure(
                    "session_restore_result",
                    "app_state::background_io",
                    &error,
                    std::iter::empty(),
                );
                self.report_session_restore_failed(error);
                None
            }
        };

        if !action.loaded_from_settings
            && let Some(legacy_settings) = legacy_settings
        {
            self.apply_settings(legacy_settings);
            let _ = self.persist_settings_now();
        }
        if !restored_session && action.startup_options.files.is_empty() {
            self.initialize_default_workspace_tabs();
        }
        self.request_focus_for_active_view();
        self.apply_startup_options_async(action.startup_options);
    }
}

fn record_background_send_error(error: &BackgroundIoSendError) {
    diagnostics::record_background_failure(
        "background_send",
        "app_state::background_io",
        format!(
            "Background I/O request '{}' could not be queued: {}",
            error.request_kind(),
            error.reason()
        ),
        [
            ("lane", error.lane_name().to_owned()),
            ("reason", error.reason().to_owned()),
            ("request_kind", error.request_kind().to_owned()),
        ],
    );
}

#[cfg(test)]
mod tests;
