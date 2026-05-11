use super::*;

impl ScratchpadApp {
    pub(super) fn apply_background_io_result(&mut self, result: BackgroundIoResult) {
        match result {
            BackgroundIoResult::PathsLoaded {
                request_id,
                results,
                is_partial,
            } => self.apply_paths_loaded_result(request_id, results, is_partial),
            BackgroundIoResult::SessionRestored { request_id, result } => {
                self.apply_session_restored_result(request_id, result);
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

        match self.pending_background_actions.remove(&request_id) {
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

    fn apply_partial_paths_loaded_result(
        &mut self,
        request_id: u64,
        results: Vec<LoadedPathResult>,
    ) {
        let Some(PendingBackgroundAction::OpenTabs(action)) =
            self.pending_background_actions.get_mut(&request_id)
        else {
            return;
        };

        let mut summary = std::mem::take(&mut action.accumulator);
        for loaded in results {
            FileController::process_open_tab_result(self, &mut summary, loaded);
        }
        if let Some(PendingBackgroundAction::OpenTabs(action)) =
            self.pending_background_actions.get_mut(&request_id)
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
            self.pending_background_actions.remove(&request_id)
        else {
            return;
        };
        self.apply_runtime_startup_restore_result(action, result);
    }

    fn apply_session_persisted_result(&mut self, request_id: u64, result: Result<(), String>) {
        let Some(PendingBackgroundAction::PersistSession(_)) =
            self.pending_background_actions.remove(&request_id)
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
                self.set_error_status(format!("Session save failed: {error}"));
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
            self.pending_background_actions.remove(&request_id)
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
            self.pending_background_actions.remove(&request_id)
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
                self.set_error_status(format!("Session restore failed: {error}"));
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
