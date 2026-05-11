use super::fallback::BackgroundIoFallback;
use super::*;

impl ScratchpadApp {
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
        self.io
            .pending_background_actions
            .insert(request_id, action);

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
        self.io
            .pending_background_actions
            .insert(request_id, action);

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
        self.io
            .pending_background_actions
            .insert(request_id, PendingBackgroundAction::SavePath(action));

        let request = BackgroundIoRequest::SavePath {
            request_id,
            path,
            snapshot,
            format,
        };
        if let Err(error) = self.io.background_io_tx.send(request) {
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
        self.io.pending_background_actions.insert(
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
        self.io.pending_background_actions.insert(
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
        if self.io.pending_background_actions.values().any(|action| {
            matches!(
                action,
                PendingBackgroundAction::RefreshTextMetadata(pending)
                    if pending.buffer_id == buffer_id && pending.revision == revision
            )
        }) {
            return;
        }

        let request_id = self.allocate_background_request_id();
        self.io.pending_background_actions.insert(
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
        if self.io.pending_background_actions.values().any(|action| {
            matches!(
                action,
                PendingBackgroundAction::RefreshEncodingCompliance(pending)
                    if pending.buffer_id == buffer_id && pending.revision == revision
            )
        }) {
            return;
        }

        let request_id = self.allocate_background_request_id();
        self.io.pending_background_actions.insert(
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
        let request_id = self.io.next_background_request_id;
        self.io.next_background_request_id = self.io.next_background_request_id.saturating_add(1);
        request_id
    }

    fn send_background_request_or_apply(
        &mut self,
        request_id: u64,
        request: BackgroundIoRequest,
        fallback: impl FnOnce(&mut Self, u64, BackgroundIoRequest),
    ) {
        if let Err(error) = self.io.background_io_tx.send(request) {
            record_background_send_error(&error);
            self.io.pending_background_actions.remove(&request_id);
            fallback(self, request_id, error.into_request());
        }
    }
}
