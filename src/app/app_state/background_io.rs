use super::{
    PendingBackgroundAction, PendingEncodingComplianceAction, PendingSessionPersistAction,
    PendingStartupRestoreAction, PendingTextMetadataAction, ScratchpadApp,
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

mod apply;

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
            _ => None,
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
