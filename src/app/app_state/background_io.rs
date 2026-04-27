use super::{
    PendingBackgroundAction, PendingEncodingComplianceAction, PendingSessionPersistAction,
    PendingStartupRestoreAction, PendingTextMetadataAction, ScratchpadApp,
};
use crate::app::services::background_io::{
    BackgroundIoRequest, BackgroundIoResult, LoadedPathResult, PathLoadRequest,
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
        if paths.is_empty() {
            return;
        }

        let request_id = self.allocate_background_request_id();
        self.pending_background_actions.insert(request_id, action);

        let request = BackgroundIoRequest::LoadPaths {
            request_id,
            requests: paths.into_iter().map(PathLoadRequest::Standard).collect(),
        };
        if let Err(error) = self.background_io_tx.send(request) {
            self.pending_background_actions.remove(&request_id);
            self.apply_background_io_result(BackgroundIoResult::PathsLoaded {
                request_id,
                results: error
                    .into_request()
                    .into_loaded_path_results()
                    .unwrap_or_default(),
            });
        }
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
        };
        if let Err(error) = self.background_io_tx.send(request) {
            self.pending_background_actions.remove(&request_id);
            self.apply_background_io_result(BackgroundIoResult::PathsLoaded {
                request_id,
                results: error
                    .into_request()
                    .into_loaded_path_results()
                    .unwrap_or_default(),
            });
        }
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
            }),
        );

        let request = BackgroundIoRequest::RestoreSession {
            request_id,
            session_store: self.session_store.clone(),
        };
        if let Err(error) = self.background_io_tx.send(request) {
            self.pending_background_actions.remove(&request_id);
            self.apply_background_io_result(BackgroundIoResult::SessionRestored {
                request_id,
                result: error.into_request().into_restore_result(),
            });
        }
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
        if let Err(error) = self.background_io_tx.send(request) {
            self.pending_background_actions.remove(&request_id);
            self.apply_background_io_result(BackgroundIoResult::SessionPersisted {
                request_id,
                result: error.into_request().into_persist_result(),
            });
        }
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
        if let Err(error) = self.background_io_tx.send(request) {
            self.pending_background_actions.remove(&request_id);
            self.apply_background_io_result(BackgroundIoResult::TextMetadataRefreshed {
                request_id,
                buffer_id,
                revision,
                result: error.into_request().into_text_metadata_result(),
            });
        }
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
        if let Err(error) = self.background_io_tx.send(request) {
            self.pending_background_actions.remove(&request_id);
            self.apply_background_io_result(BackgroundIoResult::EncodingComplianceRefreshed {
                request_id,
                buffer_id,
                revision,
                result: error.into_request().into_encoding_compliance_result(),
            });
        }
    }

    fn allocate_background_request_id(&mut self) -> u64 {
        let request_id = self.next_background_request_id;
        self.next_background_request_id = self.next_background_request_id.saturating_add(1);
        request_id
    }

    fn apply_background_io_result(&mut self, result: BackgroundIoResult) {
        match result {
            BackgroundIoResult::PathsLoaded {
                request_id,
                results,
            } => match self.pending_background_actions.remove(&request_id) {
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
            },
            BackgroundIoResult::SessionRestored { request_id, result } => {
                let Some(PendingBackgroundAction::StartupRestore(action)) =
                    self.pending_background_actions.remove(&request_id)
                else {
                    return;
                };
                self.apply_runtime_startup_restore_result(action, result);
            }
            BackgroundIoResult::SessionPersisted { request_id, result } => {
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
                        self.mark_session_dirty();
                        self.set_error_status(format!("Session save failed: {error}"));
                    }
                }
            }
            BackgroundIoResult::TextMetadataRefreshed {
                request_id,
                buffer_id,
                revision,
                result,
            } => {
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
                    buffer.apply_text_metadata_refresh(
                        revision,
                        line_count,
                        artifact_summary,
                        format,
                    );
                }
            }
            BackgroundIoResult::EncodingComplianceRefreshed {
                request_id,
                buffer_id,
                revision,
                result,
            } => {
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
                    buffer
                        .apply_encoding_compliance_refresh(revision, has_non_compliant_characters);
                }
            }
        }
    }

    fn apply_runtime_startup_restore_result(
        &mut self,
        action: PendingStartupRestoreAction,
        result: Result<Option<crate::app::services::session_store::RestoredSession>, String>,
    ) {
        let legacy_settings = match result {
            Ok(Some(restored)) => Some(session_manager::apply_restored_session(self, restored)),
            Ok(None) => None,
            Err(error) => {
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
        self.request_focus_for_active_view();
        self.apply_startup_options_async(action.startup_options);
    }
}

trait BackgroundIoFallback {
    fn into_loaded_path_results(self) -> Option<Vec<LoadedPathResult>>;
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
            BackgroundIoRequest::RestoreSession { .. }
            | BackgroundIoRequest::PersistSession { .. }
            | BackgroundIoRequest::RefreshTextMetadata { .. }
            | BackgroundIoRequest::RefreshEncodingCompliance { .. } => None,
        }
    }

    fn into_restore_result(
        self,
    ) -> Result<Option<crate::app::services::session_store::RestoredSession>, String> {
        match self {
            BackgroundIoRequest::RestoreSession { .. } => {
                Err("Background session restore unavailable.".to_owned())
            }
            BackgroundIoRequest::LoadPaths { .. }
            | BackgroundIoRequest::PersistSession { .. }
            | BackgroundIoRequest::RefreshTextMetadata { .. }
            | BackgroundIoRequest::RefreshEncodingCompliance { .. } => Ok(None),
        }
    }

    fn into_persist_result(self) -> Result<(), String> {
        match self {
            BackgroundIoRequest::PersistSession { .. } => {
                Err("Background session save unavailable.".to_owned())
            }
            BackgroundIoRequest::LoadPaths { .. }
            | BackgroundIoRequest::RestoreSession { .. }
            | BackgroundIoRequest::RefreshTextMetadata { .. }
            | BackgroundIoRequest::RefreshEncodingCompliance { .. } => Ok(()),
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
            BackgroundIoRequest::LoadPaths { .. }
            | BackgroundIoRequest::RestoreSession { .. }
            | BackgroundIoRequest::PersistSession { .. }
            | BackgroundIoRequest::RefreshEncodingCompliance { .. } => {
                Err("Background I/O channel unavailable.".to_owned())
            }
        }
    }

    fn into_encoding_compliance_result(self) -> Result<bool, String> {
        match self {
            BackgroundIoRequest::RefreshEncodingCompliance { .. } => {
                Err("Background encoding compliance refresh unavailable.".to_owned())
            }
            BackgroundIoRequest::LoadPaths { .. }
            | BackgroundIoRequest::RestoreSession { .. }
            | BackgroundIoRequest::PersistSession { .. }
            | BackgroundIoRequest::RefreshTextMetadata { .. } => Ok(false),
        }
    }
}
