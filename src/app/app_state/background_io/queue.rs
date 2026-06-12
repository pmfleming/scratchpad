use super::fallback::BackgroundIoFallback;
use super::{
    BackgroundIoRequest, BackgroundIoResult, PathBuf, PathLoadRequest, PendingBackgroundAction,
    PendingEncodingComplianceAction, PendingSavePathAction, PendingSessionHydrationAction,
    PendingSessionPersistAction, PendingStartupRestoreAction, PendingTextMetadataAction,
    ScratchpadApp, SessionPersistRequest, StartupOptions, cold_session_tab_buffer_ids,
    record_background_send_error,
};
use crate::app::domain::{DocumentSnapshot, TextFormatMetadata};

#[derive(Clone, Copy)]
enum AnalysisRefreshKind {
    TextMetadata,
    EncodingCompliance,
}

impl ScratchpadApp {
    pub(crate) fn queue_active_buffer_encoding_compliance_refresh(&mut self) {
        let Some((buffer_id, revision, snapshot, format)) =
            self.tab_manager.active_tab().and_then(|tab| {
                let buffer = tab.active_buffer();
                buffer.encoding_compliance_refresh_needed().then(|| {
                    (
                        buffer.id,
                        buffer.document_revision(),
                        buffer.document_snapshot(),
                        buffer.format.clone(),
                    )
                })
            })
        else {
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

        let request_id = self.state.background_io.allocate_background_request_id();
        self.state
            .background_io
            .insert_pending_background_action(request_id, action);

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
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state
            .background_io
            .insert_pending_background_action(request_id, action);

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
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state.background_io.insert_pending_background_action(
            request_id,
            PendingBackgroundAction::SavePath(action),
        );

        let request = BackgroundIoRequest::SavePath {
            request_id,
            path,
            snapshot,
            format,
        };
        if let Err(error) = self.state.background_io.tx.send(request) {
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
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state.background_io.insert_pending_background_action(
            request_id,
            PendingBackgroundAction::StartupRestore(PendingStartupRestoreAction {
                startup_options,
                loaded_from_settings,
                restore_started: false,
                streamed_tab_count: 0,
                streamed_tab_indices: Vec::new(),
            }),
        );

        let request = BackgroundIoRequest::RestoreSession {
            request_id,
            session_store: self.state.session_store.clone(),
        };
        self.send_background_request_or_apply(request_id, request, |app, request_id, request| {
            app.apply_background_io_result(BackgroundIoResult::SessionRestored {
                request_id,
                result: request.into_restore_result(),
            });
        });
    }

    pub(crate) fn queue_next_progressive_session_hydration(&mut self) {
        if self.state.background_io.has_pending_session_hydration() {
            return;
        }

        let active_tab_index = self.tab_manager.active_tab_index;
        let Some((tab_index, cold_session_tab)) = self
            .tab_manager
            .cold_session_tabs()
            .iter()
            .min_by_key(|(index, _)| index.abs_diff(active_tab_index))
            .map(|(index, tab)| (*index, tab.clone()))
        else {
            return;
        };

        let expected_buffer_ids = cold_session_tab_buffer_ids(&cold_session_tab);
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state.background_io.insert_pending_background_action(
            request_id,
            PendingBackgroundAction::HydrateSessionTab(PendingSessionHydrationAction {
                expected_buffer_ids,
            }),
        );

        let request = BackgroundIoRequest::HydrateSessionTab {
            request_id,
            session_store: self.state.session_store.clone(),
            tab_index,
            cold_session_tab,
        };
        if let Err(error) = self.state.background_io.tx.send(request) {
            record_background_send_error(&error);
            self.apply_background_io_result(
                error.into_request().into_hydrated_session_tab_result(),
            );
        }
    }

    pub(crate) fn queue_background_session_persist(&mut self, request: SessionPersistRequest) {
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state.background_io.insert_pending_background_action(
            request_id,
            PendingBackgroundAction::PersistSession(PendingSessionPersistAction),
        );

        let request = BackgroundIoRequest::PersistSession {
            request_id,
            session_store: self.state.session_store.clone(),
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
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
    ) {
        self.queue_background_analysis_refresh(
            AnalysisRefreshKind::TextMetadata,
            buffer_id,
            revision,
            snapshot,
            format,
        );
    }

    pub(crate) fn queue_background_encoding_compliance_refresh(
        &mut self,
        buffer_id: u64,
        revision: u64,
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
    ) {
        self.queue_background_analysis_refresh(
            AnalysisRefreshKind::EncodingCompliance,
            buffer_id,
            revision,
            snapshot,
            format,
        );
    }

    fn queue_background_analysis_refresh(
        &mut self,
        kind: AnalysisRefreshKind,
        buffer_id: u64,
        revision: u64,
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
    ) {
        if self.has_pending_analysis_refresh(kind, buffer_id, revision) {
            return;
        }

        let request_id = self.state.background_io.allocate_background_request_id();
        self.state.background_io.insert_pending_background_action(
            request_id,
            pending_analysis_refresh_action(kind, buffer_id, revision),
        );

        let request =
            analysis_refresh_request(kind, request_id, buffer_id, revision, snapshot, format);
        self.send_background_request_or_apply(
            request_id,
            request,
            move |app, request_id, request| {
                app.apply_background_io_result(analysis_refresh_fallback_result(
                    kind, request_id, buffer_id, revision, request,
                ));
            },
        );
    }

    fn has_pending_analysis_refresh(
        &self,
        kind: AnalysisRefreshKind,
        buffer_id: u64,
        revision: u64,
    ) -> bool {
        match kind {
            AnalysisRefreshKind::TextMetadata => self
                .state
                .background_io
                .has_pending_text_metadata_refresh(buffer_id, revision),
            AnalysisRefreshKind::EncodingCompliance => self
                .state
                .background_io
                .has_pending_encoding_compliance_refresh(buffer_id, revision),
        }
    }

    fn send_background_request_or_apply(
        &mut self,
        request_id: u64,
        request: BackgroundIoRequest,
        fallback: impl FnOnce(&mut Self, u64, BackgroundIoRequest),
    ) {
        if let Err(error) = self.state.background_io.tx.send(request) {
            record_background_send_error(&error);
            self.state
                .background_io
                .drop_pending_background_action(request_id);
            fallback(self, request_id, error.into_request());
        }
    }
}

fn pending_analysis_refresh_action(
    kind: AnalysisRefreshKind,
    buffer_id: u64,
    revision: u64,
) -> PendingBackgroundAction {
    match kind {
        AnalysisRefreshKind::TextMetadata => {
            PendingBackgroundAction::RefreshTextMetadata(PendingTextMetadataAction {
                buffer_id,
                revision,
            })
        }
        AnalysisRefreshKind::EncodingCompliance => {
            PendingBackgroundAction::RefreshEncodingCompliance(PendingEncodingComplianceAction {
                buffer_id,
                revision,
            })
        }
    }
}

fn analysis_refresh_request(
    kind: AnalysisRefreshKind,
    request_id: u64,
    buffer_id: u64,
    revision: u64,
    snapshot: DocumentSnapshot,
    format: TextFormatMetadata,
) -> BackgroundIoRequest {
    match kind {
        AnalysisRefreshKind::TextMetadata => BackgroundIoRequest::RefreshTextMetadata {
            request_id,
            buffer_id,
            revision,
            snapshot,
            format,
        },
        AnalysisRefreshKind::EncodingCompliance => BackgroundIoRequest::RefreshEncodingCompliance {
            request_id,
            buffer_id,
            revision,
            snapshot,
            format,
        },
    }
}

fn analysis_refresh_fallback_result(
    kind: AnalysisRefreshKind,
    request_id: u64,
    buffer_id: u64,
    revision: u64,
    request: BackgroundIoRequest,
) -> BackgroundIoResult {
    match kind {
        AnalysisRefreshKind::TextMetadata => BackgroundIoResult::TextMetadataRefreshed {
            request_id,
            buffer_id,
            revision,
            result: request.into_text_metadata_result(),
        },
        AnalysisRefreshKind::EncodingCompliance => {
            BackgroundIoResult::EncodingComplianceRefreshed {
                request_id,
                buffer_id,
                revision,
                result: request.into_encoding_compliance_result(),
            }
        }
    }
}
