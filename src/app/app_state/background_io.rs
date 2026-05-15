use super::{
    PendingBackgroundAction, PendingEncodingComplianceAction, PendingSavePathAction,
    PendingSessionHydrationAction, PendingSessionPersistAction, PendingStartupRestoreAction,
    PendingTextMetadataAction, ScratchpadApp,
};
use crate::app::app_state::workspace::{
    accessors as workspace_accessors, restore_conflict as workspace_restore_conflict,
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

        if self.state.background_io.has_pending_background_actions() {
            ctx.request_repaint_after(BACKGROUND_IO_POLL_INTERVAL);
        }
    }

    pub fn drain_background_io_results(&mut self) {
        while let Ok(result) = self.state.background_io.rx.try_recv() {
            self.apply_background_io_result(result);
        }
    }

    pub fn wait_for_background_io_idle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            self.drain_background_io_results();
            if !self.state.background_io.has_pending_background_actions() {
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
                active_surface,
                legacy_settings,
            } => {
                self.apply_session_restore_started_result(
                    request_id,
                    active_tab_index,
                    active_surface,
                    legacy_settings,
                );
            }
            BackgroundIoResult::SessionTabRestored {
                request_id,
                tab_index,
                cold_session_tab,
                tab,
            } => {
                self.apply_session_tab_restored_result(
                    request_id,
                    tab_index,
                    cold_session_tab,
                    *tab,
                );
            }
            BackgroundIoResult::SessionTabHydrated {
                request_id,
                tab_index,
                restore_status,
                tab,
            } => {
                self.apply_session_tab_hydrated_result(request_id, tab_index, restore_status, *tab);
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

        match self
            .state
            .background_io
            .remove_pending_background_action(request_id)
        {
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
            Some(
                PendingBackgroundAction::SavePath(_)
                | PendingBackgroundAction::StartupRestore(_)
                | PendingBackgroundAction::HydrateSessionTab(_)
                | PendingBackgroundAction::PersistSession(_)
                | PendingBackgroundAction::RefreshTextMetadata(_)
                | PendingBackgroundAction::RefreshEncodingCompliance(_),
            )
            | None => {}
            Some(PendingBackgroundAction::StartupRestoreCompare(action)) => {
                workspace_restore_conflict::apply_async_startup_restore_compare_result(
                    self, action, results,
                );
            }
        }
    }

    fn apply_path_saved_result(
        &mut self,
        request_id: u64,
        path: PathBuf,
        disk_state: Option<crate::app::domain::DiskFileState>,
        result: Result<(), String>,
    ) {
        let Some(PendingBackgroundAction::SavePath(action)) = self
            .state
            .background_io
            .remove_pending_background_action(request_id)
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
        let Some(PendingBackgroundAction::OpenTabs(action)) = self
            .state
            .background_io
            .pending_background_action_mut(request_id)
        else {
            return;
        };

        let mut summary = std::mem::take(&mut action.accumulator);
        for loaded in results {
            FileController::process_open_tab_result(self, &mut summary, loaded);
        }
        if let Some(PendingBackgroundAction::OpenTabs(action)) = self
            .state
            .background_io
            .pending_background_action_mut(request_id)
        {
            action.accumulator = summary;
        }
    }

    fn apply_session_restored_result(
        &mut self,
        request_id: u64,
        result: Result<Option<crate::app::services::session_store::RestoredSession>, String>,
    ) {
        let Some(PendingBackgroundAction::StartupRestore(action)) = self
            .state
            .background_io
            .remove_pending_background_action(request_id)
        else {
            return;
        };
        self.apply_runtime_startup_restore_result(action, result);
    }

    fn apply_session_restore_started_result(
        &mut self,
        request_id: u64,
        active_tab_index: usize,
        active_surface: crate::app::services::session_store::SessionActiveSurface,
        legacy_settings: crate::app::services::settings_store::AppSettings,
    ) {
        let apply_legacy_settings = {
            let Some(PendingBackgroundAction::StartupRestore(action)) = self
                .state
                .background_io
                .pending_background_action_mut(request_id)
            else {
                return;
            };
            action.restore_started = true;
            !action.loaded_from_settings
        };
        if apply_legacy_settings {
            crate::app::app_state::settings_state::apply_settings(self, legacy_settings);
            let _ = crate::app::app_state::settings_state::persist_settings_now(self);
        }
        if !self.tab_manager.tabs.as_slice().is_empty() {
            self.tab_manager
                .set_active_tab_index_clamped(active_tab_index);
        }
        session_manager::apply_restored_active_surface(self, active_surface);
    }

    fn apply_session_tab_restored_result(
        &mut self,
        request_id: u64,
        tab_index: usize,
        cold_session_tab: Option<crate::app::services::session_store::ColdSessionTab>,
        tab: crate::app::domain::WorkspaceTab,
    ) {
        let first_streamed_tab = {
            let Some(PendingBackgroundAction::StartupRestore(action)) = self
                .state
                .background_io
                .pending_background_action_mut(request_id)
            else {
                return;
            };
            let first_streamed_tab = action.streamed_tab_count == 0;
            action.streamed_tab_count += 1;
            action.streamed_tab_indices.push(tab_index);
            first_streamed_tab
        };

        if first_streamed_tab {
            self.tab_manager.set_tabs(vec![tab], 0);
        } else {
            self.tab_manager.append_restored_tab(tab);
        }
        if let Some(cold_session_tab) = cold_session_tab {
            let restored_index = self.tab_manager.tabs.as_slice().len().saturating_sub(1);
            self.tab_manager
                .set_cold_session_tab(restored_index, cold_session_tab);
        }
        crate::app::app_state::workspace::display_tabs::ensure_active_tab_slot_selected(self);
        workspace_restore_conflict::refresh_startup_restore_conflicts(self);
        crate::app::app_state::search_runtime::mark_search_dirty(self);
    }

    fn apply_session_tab_hydrated_result(
        &mut self,
        request_id: u64,
        _tab_index: usize,
        restore_status: Option<crate::app::services::session_store::RestoreStatus>,
        tab: crate::app::domain::WorkspaceTab,
    ) {
        let Some(PendingBackgroundAction::HydrateSessionTab(action)) = self
            .state
            .background_io
            .remove_pending_background_action(request_id)
        else {
            return;
        };

        let Some(current_tab_index) = self.find_cold_session_tab_index(&action.expected_buffer_ids)
        else {
            self.queue_next_progressive_session_hydration();
            return;
        };

        if self
            .tab_manager
            .replace_restored_tab(current_tab_index, tab)
        {
            let _ = self.tab_manager.take_cold_session_tab(current_tab_index);
            if let Some(status) = restore_status {
                self.apply_session_restore_status(status);
            }
            workspace_restore_conflict::refresh_startup_restore_conflicts(self);
            crate::app::app_state::search_runtime::mark_search_dirty(self);
        }
        self.queue_next_progressive_session_hydration();
    }

    fn apply_session_persisted_result(&mut self, request_id: u64, result: Result<(), String>) {
        let Some(PendingBackgroundAction::PersistSession(_)) = self
            .state
            .background_io
            .remove_pending_background_action(request_id)
        else {
            return;
        };
        match result {
            Ok(()) => {
                self.state.last_session_persist = Instant::now();
            }
            Err(error) => {
                diagnostics::record_background_failure(
                    "session_persist_result",
                    "app_state::background_io",
                    &error,
                    [("request_id", request_id.to_string())],
                );
                self.tab_manager.mark_session_dirty();
                self.state.status.report_session_save_failed(error);
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
                crate::app::domain::buffer::BufferLength,
                usize,
                crate::app::domain::TextArtifactSummary,
                crate::app::domain::TextFormatMetadata,
            ),
            String,
        >,
    ) {
        let Some(PendingBackgroundAction::RefreshTextMetadata(_)) = self
            .state
            .background_io
            .remove_pending_background_action(request_id)
        else {
            return;
        };
        if let Ok((_length, line_count, artifact_summary, format)) = result
            && let Some(buffer) = self
                .tab_manager
                .tabs
                .as_mut_slice()
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
        let Some(PendingBackgroundAction::RefreshEncodingCompliance(_)) = self
            .state
            .background_io
            .remove_pending_background_action(request_id)
        else {
            return;
        };
        if let Ok(has_non_compliant_characters) = result
            && let Some(buffer) = self
                .tab_manager
                .tabs
                .as_mut_slice()
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
                self.tab_manager
                    .reorder_tabs_by_original_indices(&action.streamed_tab_indices);
                if !self.tab_manager.tabs.as_slice().is_empty() {
                    self.tab_manager
                        .set_active_tab_index_clamped(restored.active_tab_index);
                }
                session_manager::apply_restored_active_surface(self, restored.active_surface);
                self.tab_manager.evict_inactive_tab_state();
                if let Some(status) = restored.restore_status.as_ref() {
                    self.apply_session_restore_status(status.clone());
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
                self.state.status.report_session_restore_failed(error);
                None
            }
        };

        if !action.loaded_from_settings
            && let Some(legacy_settings) = legacy_settings
        {
            crate::app::app_state::settings_state::apply_settings(self, legacy_settings);
            let _ = crate::app::app_state::settings_state::persist_settings_now(self);
        }
        if !restored_session && action.startup_options.files.is_empty() {
            self.initialize_default_workspace_tabs();
        }
        workspace_accessors::request_focus_for_active_view(self);
        self.apply_startup_options_async(action.startup_options);
        if restored_session {
            self.queue_next_progressive_session_hydration();
        }
    }

    pub(crate) fn hydrate_tab_if_needed(&mut self, index: usize) -> bool {
        let Some(cold_tab) = self.tab_manager.take_cold_session_tab(index) else {
            return false;
        };

        let (restored_tab, restore_status) =
            self.state.session_store.restore_cold_session_tab(cold_tab);
        if !self.tab_manager.replace_restored_tab(index, restored_tab) {
            return false;
        }
        if let Some(status) = restore_status {
            self.apply_session_restore_status(status);
        }
        crate::app::app_state::search_runtime::mark_search_dirty(self);
        true
    }

    fn find_cold_session_tab_index(
        &self,
        expected_buffer_ids: &[crate::app::domain::BufferId],
    ) -> Option<usize> {
        self.tab_manager
            .cold_session_tabs()
            .iter()
            .find_map(|(index, cold_session_tab)| {
                (cold_session_tab_buffer_ids(cold_session_tab) == expected_buffer_ids)
                    .then_some(*index)
            })
    }

    fn apply_session_restore_status(
        &mut self,
        status: crate::app::services::session_store::RestoreStatus,
    ) {
        match status.level {
            crate::app::services::session_store::RestoreStatusLevel::Info => {
                self.state.status.set_info_status_in_domain(
                    crate::app::app_state::StatusDomain::Session,
                    status.message,
                );
            }
            crate::app::services::session_store::RestoreStatusLevel::Warning => {
                self.state.status.set_warning_status_in_domain(
                    crate::app::app_state::StatusDomain::Session,
                    status.message,
                );
            }
        }
    }
}

fn cold_session_tab_buffer_ids(
    tab: &crate::app::services::session_store::ColdSessionTab,
) -> Vec<crate::app::domain::BufferId> {
    tab.buffer_ids().collect()
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
