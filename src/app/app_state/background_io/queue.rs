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
    pub(crate) fn queue_retained_open(
        &mut self,
        target: crate::app::services::session_store::RetainedTarget,
        document: Option<String>,
        node: Option<u64>,
    ) {
        let store = self.state.persistence.session_store.clone();
        let request_id = self.state.background_io.allocate_background_request_id();
        let claimed = self
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(|tab| tab.buffers())
            .filter_map(|buffer| buffer.path_key.clone())
            .collect();
        self.state.background_io.insert_pending_background_action(
            request_id,
            PendingBackgroundAction::ReopenRetained(
                crate::app::app_state::PendingRetainedOpenAction {
                    expected_epoch: store.history_epoch(),
                    document,
                    node,
                },
            ),
        );
        let request = BackgroundIoRequest::ReopenRetained {
            request_id,
            session_store: store,
            target,
            claimed,
        };
        self.send_background_request_or_apply(request_id, request, |app, request_id, _| {
            app.apply_background_io_result(BackgroundIoResult::RetainedReopened {
                request_id,
                result: Err("Background document reopening unavailable.".into()),
            });
        });
    }

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

    fn reload_preparation_base(
        &self,
        action: &PendingBackgroundAction,
    ) -> Option<(
        crate::app::domain::BufferState,
        Option<crate::app::domain::PieceSource>,
    )> {
        let (PendingBackgroundAction::ReloadBuffer(load)
        | PendingBackgroundAction::ReopenWithEncoding(load)) = action
        else {
            return None;
        };
        let original = self
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .find_map(|tab| tab.buffer_by_id(load.buffer_id))?;
        let mut buffer = original.clone();
        buffer.prepared_history_base = Some(original.document().history_token());
        let source = match load.mode {
            crate::app::app_state::PendingReloadMode::ObserveDiskChange => {
                Some(crate::app::domain::PieceSource::External)
            }
            crate::app::app_state::PendingReloadMode::ReopenWithEncoding => {
                Some(crate::app::domain::PieceSource::Load)
            }
            crate::app::app_state::PendingReloadMode::ExplicitReload => None,
        };
        Some((buffer, source))
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
        mut action: PendingBackgroundAction,
        streaming: bool,
    ) {
        if paths.is_empty() {
            return;
        }
        let observe = matches!(&action,PendingBackgroundAction::ReloadBuffer(load) if load.mode==crate::app::app_state::PendingReloadMode::ObserveDiskChange);
        let disk_reload = matches!(&action, PendingBackgroundAction::ReloadBuffer(_));
        let materialize = matches!(&action,PendingBackgroundAction::ReloadBuffer(load) if load.materialize_replacement || load.mode==crate::app::app_state::PendingReloadMode::ExplicitReload);
        let observed_digest = match &action {
            PendingBackgroundAction::ReloadBuffer(load) if observe => {
                load.expected_observed_digest.clone()
            }
            _ => None,
        };
        if let PendingBackgroundAction::ReloadBuffer(load) = &mut action {
            load.expected_epoch = self.state.persistence.session_store.history_epoch();
        }

        let retained = !self.state.persistence.session_restore_failed
            && matches!(
                action,
                PendingBackgroundAction::OpenTabs(_) | PendingBackgroundAction::OpenHere(_)
            )
            && !self.state.persistence.session_store.writes_blocked();
        let claimed: Vec<_> = self
            .tab_manager
            .tabs
            .as_slice()
            .iter()
            .flat_map(|t| t.buffers())
            .filter(|b| !b.is_loading_preview)
            .filter_map(|b| b.path_key.clone())
            .collect();
        let base = self.reload_preparation_base(&action);
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state
            .background_io
            .insert_pending_background_action(request_id, action);
        let store = self.state.persistence.session_store.opened_chunk_store();
        let request = BackgroundIoRequest::LoadPaths {
            request_id,
            requests: paths
                .into_iter()
                .map(|path| {
                    if retained {
                        PathLoadRequest::Retained {
                            path,
                            session_store: self.state.persistence.session_store.clone(),
                            claimed: claimed.clone(),
                        }
                    } else if disk_reload {
                        PathLoadRequest::Observed {
                            path,
                            materialize,
                            observed_digest: observed_digest.clone(),
                        }
                    } else {
                        PathLoadRequest::Standard(path)
                    }
                })
                .map(|request| match &base {
                    Some((base, source)) => PathLoadRequest::Reconcile {
                        base: Box::new(base.clone()),
                        source: *source,
                        materialized_only: true,
                        request: Box::new(request),
                    },
                    None => request,
                })
                .map(|request| match &store {
                    Some(store) => PathLoadRequest::Bound {
                        store: store.clone(),
                        request: Box::new(request),
                    },
                    None => request,
                })
                .map(|request| {
                    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let tokens = self
                        .state
                        .background_io
                        .path_imports
                        .entry(request.path().clone())
                        .or_default();
                    tokens.retain(|token| token.strong_count() > 0);
                    tokens.push(std::sync::Arc::downgrade(&cancelled));
                    PathLoadRequest::Cancelable {
                        cancelled,
                        request: Box::new(request),
                    }
                })
                .collect(),
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

    pub(crate) fn queue_background_cold_file_shells(
        &mut self,
        paths: Vec<PathBuf>,
        action: PendingBackgroundAction,
    ) {
        if paths.is_empty() {
            return;
        }
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state
            .background_io
            .insert_pending_background_action(request_id, action);
        let request = BackgroundIoRequest::BuildColdFileShells {
            request_id,
            paths,
            session_store: self.state.persistence.session_store.clone(),
        };
        self.send_background_request_or_apply(request_id, request, |app, request_id, request| {
            let shells = request.into_cold_file_shell_results().unwrap_or_default();
            app.apply_background_io_result(BackgroundIoResult::ColdFileShellsBuilt {
                request_id,
                shells,
            });
        });
    }

    pub(crate) fn queue_background_path_load_with_encoding(
        &mut self,
        path: PathBuf,
        encoding_name: String,
        action: PendingBackgroundAction,
    ) {
        let base = self.reload_preparation_base(&action);
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state
            .background_io
            .insert_pending_background_action(request_id, action);

        let request = PathLoadRequest::WithEncoding {
            path,
            encoding_name,
        };
        let request = match base {
            Some((base, source)) => PathLoadRequest::Reconcile {
                base: Box::new(base),
                source,
                materialized_only: false,
                request: Box::new(request),
            },
            None => request,
        };
        let request = BackgroundIoRequest::LoadPaths {
            request_id,
            requests: vec![request],
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

    pub(crate) fn queue_background_session_restore(&mut self, startup_options: StartupOptions) {
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state.background_io.insert_pending_background_action(
            request_id,
            PendingBackgroundAction::StartupRestore(PendingStartupRestoreAction {
                startup_options,
                streamed_tab_count: 0,
                streamed_tab_indices: Vec::new(),
            }),
        );

        let request = BackgroundIoRequest::RestoreSession {
            request_id,
            session_store: self.state.persistence.session_store.clone(),
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

        self.queue_session_tab_hydration(tab_index, cold_session_tab, true);
    }

    pub(crate) fn queue_session_tab_hydration(
        &mut self,
        tab_index: usize,
        cold_session_tab: crate::app::services::session_store::ColdSessionTab,
        progressive: bool,
    ) {
        let expected_buffer_ids = cold_session_tab_buffer_ids(&cold_session_tab);
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state.background_io.insert_pending_background_action(
            request_id,
            PendingBackgroundAction::HydrateSessionTab(PendingSessionHydrationAction {
                progressive,
                expected_buffer_ids,
            }),
        );

        let request = BackgroundIoRequest::HydrateSessionTab {
            request_id,
            session_store: self.state.persistence.session_store.clone(),
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
        self.queue_session_persist_with_close(request, None);
    }
    pub(crate) fn queue_background_session_close(
        &mut self,
        request: SessionPersistRequest,
        close: crate::app::services::session_manager::CloseAfterPersist,
    ) {
        self.queue_session_persist_with_close(request, Some(close));
    }
    fn queue_session_persist_with_close(
        &mut self,
        request: SessionPersistRequest,
        close: Option<crate::app::services::session_manager::CloseAfterPersist>,
    ) {
        let request_id = self.state.background_io.allocate_background_request_id();
        self.state.background_io.insert_pending_background_action(
            request_id,
            PendingBackgroundAction::PersistSession(PendingSessionPersistAction {
                close,
                acknowledgement: request.acknowledgement(),
            }),
        );

        let request = BackgroundIoRequest::PersistSession {
            request_id,
            session_store: self.state.persistence.session_store.clone(),
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
