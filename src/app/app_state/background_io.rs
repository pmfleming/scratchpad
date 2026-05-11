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

impl ScratchpadApp {
    pub(crate) fn poll_background_io(&mut self, ctx: &egui::Context) {
        self.drain_background_io_results();

        if !self.pending_background_actions.is_empty() {
            ctx.request_repaint_after(BACKGROUND_IO_POLL_INTERVAL);
        }
    }

    pub fn drain_background_io_results(&mut self) {
        while let Ok(result) = self.background_io_rx.try_recv() {
            self.apply_background_io_result(result);
        }
    }

    pub fn wait_for_background_io_idle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(1);
        while Instant::now() < deadline {
            self.drain_background_io_results();
            if self.pending_background_actions.is_empty() {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.drain_background_io_results();
    }

    pub(crate) fn queue_active_buffer_encoding_compliance_refresh(&mut self) {
        let Some((buffer_id, revision, snapshot, format)) = self.active_tab().and_then(|tab| {
            let buffer = tab.active_buffer();
            buffer.encoding_compliance_refresh_needed().then(|| {
                (
                    buffer.id,
                    buffer.document_revision(),
                    buffer.document_snapshot(),
                    buffer.format.clone(),
                )
            })
        }) else {
            return;
        };

        self.queue_background_encoding_compliance_refresh(buffer_id, revision, snapshot, format);
    }

    pub(crate) fn queue_background_path_loads(
        &mut self,
        paths: Vec<PathBuf>,
        action: PendingBackgroundAction,
    ) {
        self.queue_background_path_loads_inner(paths, action, false);
    }

    /// Queue a multi-path load that streams individual results back as each
    /// file finishes loading. The action stays in `pending_background_actions`
    /// across multiple `PathsLoaded { is_partial: true }` deliveries; it is
    /// removed when the final `is_partial: false` arrives.
    pub(crate) fn queue_background_path_loads_streaming(
        &mut self,
        paths: Vec<PathBuf>,
        action: PendingBackgroundAction,
    ) {
        self.queue_background_path_loads_inner(paths, action, true);
    }

    fn queue_background_path_loads_inner(
        &mut self,
        paths: Vec<PathBuf>,
        action: PendingBackgroundAction,
        streaming: bool,
    ) {
        if paths.is_empty() {
            return;
        }

        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(request_id, action);

        let request = BackgroundIoRequest::LoadPaths {
            request_id,
            requests: paths.into_iter().map(PathLoadRequest::Standard).collect(),
            streaming,
        };
        self.send_background_request_or_apply(request_id, request, |app, request_id, request| {
            app.apply_background_io_result(BackgroundIoResult::PathsLoaded {
                request_id,
                results: request.into_loaded_path_results().unwrap_or_default(),
                is_partial: false,
            });
        });
    }

    pub(crate) fn queue_background_path_load_with_encoding(
        &mut self,
        path: PathBuf,
        encoding_name: String,
        action: PendingBackgroundAction,
    ) {
        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(request_id, action);

        let request = BackgroundIoRequest::LoadPaths {
            request_id,
            requests: vec![PathLoadRequest::WithEncoding {
                path,
                encoding_name,
            }],
            streaming: false,
        };
        self.send_background_request_or_apply(request_id, request, |app, request_id, request| {
            app.apply_background_io_result(BackgroundIoResult::PathsLoaded {
                request_id,
                results: request.into_loaded_path_results().unwrap_or_default(),
                is_partial: false,
            });
        });
    }

    pub(crate) fn queue_background_path_save(
        &mut self,
        path: PathBuf,
        snapshot: crate::app::domain::DocumentSnapshot,
        format: crate::app::domain::TextFormatMetadata,
        action: PendingSavePathAction,
    ) -> bool {
        let request_id = self.allocate_background_request_id();
        self.pending_background_actions
            .insert(request_id, PendingBackgroundAction::SavePath(action));

        let request = BackgroundIoRequest::SavePath {
            request_id,
            path,
            snapshot,
            format,
        };
        if let Err(error) = self.background_io_tx.send(request) {
            record_background_send_error(&error);
            self.apply_background_io_result(error.into_request().into_path_saved_result());
            return false;
        }

        true
    }

    pub(crate) fn queue_background_session_restore(
        &mut self,
        startup_options: StartupOptions,
        loaded_from_settings: bool,
    ) {
        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(
            request_id,
            PendingBackgroundAction::StartupRestore(PendingStartupRestoreAction {
                startup_options,
                loaded_from_settings,
                restore_started: false,
                streamed_tab_count: 0,
            }),
        );

        let request = BackgroundIoRequest::RestoreSession {
            request_id,
            session_store: self.session_store.clone(),
        };
        self.send_background_request_or_apply(request_id, request, |app, request_id, request| {
            app.apply_background_io_result(BackgroundIoResult::SessionRestored {
                request_id,
                result: request.into_restore_result(),
            });
        });
    }

    pub(crate) fn queue_background_session_persist(&mut self, request: SessionPersistRequest) {
        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(
            request_id,
            PendingBackgroundAction::PersistSession(PendingSessionPersistAction),
        );

        let request = BackgroundIoRequest::PersistSession {
            request_id,
            session_store: self.session_store.clone(),
            request,
        };
        self.send_background_request_or_apply(request_id, request, |app, request_id, request| {
            app.apply_background_io_result(BackgroundIoResult::SessionPersisted {
                request_id,
                result: request.into_persist_result(),
            });
        });
    }

    pub(crate) fn queue_background_text_metadata_refresh(
        &mut self,
        buffer_id: u64,
        revision: u64,
        snapshot: crate::app::domain::DocumentSnapshot,
        format: crate::app::domain::TextFormatMetadata,
    ) {
        if self.pending_background_actions.values().any(|action| {
            matches!(
                action,
                PendingBackgroundAction::RefreshTextMetadata(pending)
                    if pending.buffer_id == buffer_id && pending.revision == revision
            )
        }) {
            return;
        }

        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(
            request_id,
            PendingBackgroundAction::RefreshTextMetadata(PendingTextMetadataAction {
                buffer_id,
                revision,
            }),
        );

        let request = BackgroundIoRequest::RefreshTextMetadata {
            request_id,
            buffer_id,
            revision,
            snapshot,
            format,
        };
        self.send_background_request_or_apply(request_id, request, |app, request_id, request| {
            app.apply_background_io_result(BackgroundIoResult::TextMetadataRefreshed {
                request_id,
                buffer_id,
                revision,
                result: request.into_text_metadata_result(),
            });
        });
    }

    pub(crate) fn queue_background_encoding_compliance_refresh(
        &mut self,
        buffer_id: u64,
        revision: u64,
        snapshot: crate::app::domain::DocumentSnapshot,
        format: crate::app::domain::TextFormatMetadata,
    ) {
        if self.pending_background_actions.values().any(|action| {
            matches!(
                action,
                PendingBackgroundAction::RefreshEncodingCompliance(pending)
                    if pending.buffer_id == buffer_id && pending.revision == revision
            )
        }) {
            return;
        }

        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(
            request_id,
            PendingBackgroundAction::RefreshEncodingCompliance(PendingEncodingComplianceAction {
                buffer_id,
                revision,
            }),
        );

        let request = BackgroundIoRequest::RefreshEncodingCompliance {
            request_id,
            buffer_id,
            revision,
            snapshot,
            format,
        };
        self.send_background_request_or_apply(request_id, request, |app, request_id, request| {
            app.apply_background_io_result(BackgroundIoResult::EncodingComplianceRefreshed {
                request_id,
                buffer_id,
                revision,
                result: request.into_encoding_compliance_result(),
            });
        });
    }

    fn allocate_background_request_id(&mut self) -> u64 {
        let request_id = self.next_background_request_id;
        self.next_background_request_id = self.next_background_request_id.saturating_add(1);
        request_id
    }

    fn send_background_request_or_apply(
        &mut self,
        request_id: u64,
        request: BackgroundIoRequest,
        fallback: impl FnOnce(&mut Self, u64, BackgroundIoRequest),
    ) {
        if let Err(error) = self.background_io_tx.send(request) {
            record_background_send_error(&error);
            self.pending_background_actions.remove(&request_id);
            fallback(self, request_id, error.into_request());
        }
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
                self.apply_session_tab_restored_result(request_id, tab);
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
            self.pending_background_actions.remove(&request_id)
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

    fn apply_session_restore_started_result(
        &mut self,
        request_id: u64,
        active_tab_index: usize,
        legacy_settings: crate::app::services::settings_store::AppSettings,
    ) {
        let apply_legacy_settings = {
            let Some(PendingBackgroundAction::StartupRestore(action)) =
                self.pending_background_actions.get_mut(&request_id)
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
                self.pending_background_actions.get_mut(&request_id)
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
mod tests {
    use super::*;
    use crate::app::app_state::PendingOpenTabsAction;
    use crate::app::domain::{BufferState, TabManager, TextFormatMetadata, WorkspaceTab};
    use crate::app::services::session_store::SessionStore;
    use crate::app::services::settings_store::SettingsStore;

    fn test_app() -> ScratchpadApp {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.keep();
        let mut app = ScratchpadApp::with_stores_and_startup(
            SessionStore::new(root.join("session")),
            SettingsStore::new(root.join("settings")),
            StartupOptions::default(),
        );
        app.set_session_persist_on_drop(false);
        app
    }

    fn app_with_buffer(buffer: BufferState) -> ScratchpadApp {
        let mut app = test_app();
        app.tab_manager = TabManager {
            tabs: vec![WorkspaceTab::new(buffer)],
            active_tab_index: 0,
            pending_action: None,
            session_dirty: false,
            pending_scroll_to_active: false,
            buffer_tab_index: Default::default(),
        };
        app.rebuild_buffer_tab_index();
        app
    }

    #[test]
    fn text_metadata_result_updates_matching_buffer_and_clears_pending_action() {
        let buffer = BufferState::new("sample.txt".to_owned(), "one".to_owned(), None);
        let buffer_id = buffer.id;
        let revision = buffer.document_revision();
        let mut app = app_with_buffer(buffer);
        app.pending_background_actions.insert(
            42,
            PendingBackgroundAction::RefreshTextMetadata(PendingTextMetadataAction {
                buffer_id,
                revision,
            }),
        );
        let mut format = TextFormatMetadata::utf8_for_new_file("one\ntwo");
        format.refresh_from_text("one\ntwo");

        app.apply_text_metadata_refreshed_result(
            42,
            buffer_id,
            revision,
            Ok((
                2,
                crate::app::domain::TextArtifactSummary::default(),
                format,
            )),
        );

        assert!(!app.pending_background_actions.contains_key(&42));
        assert_eq!(app.tabs()[0].active_buffer().line_count, 2);
    }

    #[test]
    fn stale_encoding_compliance_result_clears_action_without_mutating_buffer() {
        let buffer = BufferState::new("sample.txt".to_owned(), "plain".to_owned(), None);
        let buffer_id = buffer.id;
        let stale_revision = buffer.document_revision().saturating_add(1);
        let mut app = app_with_buffer(buffer);
        app.pending_background_actions.insert(
            7,
            PendingBackgroundAction::RefreshEncodingCompliance(PendingEncodingComplianceAction {
                buffer_id,
                revision: stale_revision,
            }),
        );

        app.apply_encoding_compliance_refreshed_result(7, buffer_id, stale_revision, Ok(true));

        assert!(!app.pending_background_actions.contains_key(&7));
        assert!(!app.tabs()[0].active_buffer().has_non_compliant_characters);
    }

    #[test]
    fn partial_open_tabs_result_keeps_action_until_terminal_result() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("opened.txt");
        let buffer = BufferState::new(
            "opened.txt".to_owned(),
            "opened".to_owned(),
            Some(path.clone()),
        );
        let mut app = test_app();
        app.pending_background_actions.insert(
            3,
            PendingBackgroundAction::OpenTabs(PendingOpenTabsAction {
                accumulator: crate::app::services::file_controller::OpenBatchSummary::default(),
            }),
        );

        app.apply_paths_loaded_result(
            3,
            vec![LoadedPathResult {
                path,
                disk_state: None,
                result: Ok(buffer),
            }],
            true,
        );

        assert!(app.pending_background_actions.contains_key(&3));
        assert_eq!(app.tabs().len(), 2);
    }

    #[test]
    fn unknown_background_result_is_ignored() {
        let mut app = test_app();
        let original_tabs = app.tabs().len();

        app.apply_paths_loaded_result(999, Vec::new(), false);
        app.apply_session_persisted_result(999, Err("ignored".to_owned()));

        assert_eq!(app.tabs().len(), original_tabs);
        assert!(app.current_status.is_none());
    }
}

trait BackgroundIoFallback {
    fn into_loaded_path_results(self) -> Option<Vec<LoadedPathResult>>;
    fn into_path_saved_result(self) -> BackgroundIoResult;
    fn into_restore_result(
        self,
    ) -> Result<Option<crate::app::services::session_store::RestoredSession>, String>;
    fn into_persist_result(self) -> Result<(), String>;
    fn into_text_metadata_result(
        self,
    ) -> Result<
        (
            usize,
            crate::app::domain::TextArtifactSummary,
            crate::app::domain::TextFormatMetadata,
        ),
        String,
    >;
    fn into_encoding_compliance_result(self) -> Result<bool, String>;
}

impl BackgroundIoFallback for BackgroundIoRequest {
    fn into_loaded_path_results(self) -> Option<Vec<LoadedPathResult>> {
        match self {
            BackgroundIoRequest::LoadPaths { requests, .. } => Some(
                requests
                    .into_iter()
                    .map(|request| LoadedPathResult {
                        path: request.path().clone(),
                        disk_state: None,
                        result: Err("Background file loader unavailable.".to_owned()),
                    })
                    .collect(),
            ),
            _ => None,
        }
    }

    fn into_path_saved_result(self) -> BackgroundIoResult {
        match self {
            BackgroundIoRequest::SavePath {
                request_id, path, ..
            } => BackgroundIoResult::PathSaved {
                request_id,
                path,
                disk_state: None,
                result: Err("Background file saver unavailable.".to_owned()),
            },
            _ => BackgroundIoResult::PathSaved {
                request_id: 0,
                path: PathBuf::new(),
                disk_state: None,
                result: Err("Background file saver unavailable.".to_owned()),
            },
        }
    }

    fn into_restore_result(
        self,
    ) -> Result<Option<crate::app::services::session_store::RestoredSession>, String> {
        match self {
            BackgroundIoRequest::RestoreSession { .. } => {
                Err("Background session restore unavailable.".to_owned())
            }
            _ => Ok(None),
        }
    }

    fn into_persist_result(self) -> Result<(), String> {
        match self {
            BackgroundIoRequest::PersistSession { .. } => {
                Err("Background session save unavailable.".to_owned())
            }
            _ => Ok(()),
        }
    }

    fn into_text_metadata_result(
        self,
    ) -> Result<
        (
            usize,
            crate::app::domain::TextArtifactSummary,
            crate::app::domain::TextFormatMetadata,
        ),
        String,
    > {
        match self {
            BackgroundIoRequest::RefreshTextMetadata { .. } => {
                Err("Background text metadata refresh unavailable.".to_owned())
            }
            _ => Err("Background I/O channel unavailable.".to_owned()),
        }
    }

    fn into_encoding_compliance_result(self) -> Result<bool, String> {
        match self {
            BackgroundIoRequest::RefreshEncodingCompliance { .. } => {
                Err("Background encoding compliance refresh unavailable.".to_owned())
            }
            _ => Ok(false),
        }
    }
}
