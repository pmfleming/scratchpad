use crate::app::capacity_metrics::BackgroundIoLane;
use crate::app::domain::{DocumentSnapshot, TextFormatMetadata};
use crate::app::services::session_store::{ColdSessionTab, SessionPersistRequest, SessionStore};
use std::path::PathBuf;

pub(crate) enum PathLoadRequest {
    Cancelable {
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        request: Box<Self>,
    },
    Reconcile {
        base: Box<crate::app::domain::BufferState>,
        source: Option<crate::app::domain::PieceSource>,
        materialized_only: bool,
        request: Box<Self>,
    },
    Bound {
        store: crate::app::services::chunk_store::ChunkStoreHandle,
        request: Box<Self>,
    },
    Standard(PathBuf),
    Observed {
        path: PathBuf,
        materialize: bool,
        observed_digest: Option<String>,
    },
    Retained {
        path: PathBuf,
        session_store: SessionStore,
        claimed: Vec<crate::app::CanonicalPathKey>,
    },
    WithEncoding {
        path: PathBuf,
        encoding_name: String,
    },
}

impl PathLoadRequest {
    pub(crate) fn path(&self) -> &PathBuf {
        match self {
            Self::Cancelable { request, .. }
            | Self::Reconcile { request, .. }
            | Self::Bound { request, .. } => request.path(),
            Self::Standard(path)
            | Self::Observed { path, .. }
            | Self::WithEncoding { path, .. }
            | Self::Retained { path, .. } => path,
        }
    }
}

pub(crate) enum BackgroundIoRequest {
    PrepareText {
        request_id: u64,
        buffer_id: u64,
        session_store: SessionStore,
        request: Box<crate::app::domain::buffer::PrepareTextRequest>,
    },
    ReopenRetained {
        request_id: u64,
        session_store: SessionStore,
        target: crate::app::services::session_store::RetainedTarget,
        claimed: Vec<crate::app::CanonicalPathKey>,
    },
    LoadPaths {
        request_id: u64,
        requests: Vec<PathLoadRequest>,
        /// When true, each path's `LoadedPathResult` is streamed back as a
        /// separate `PathsLoaded { is_partial: true }` message; the final
        /// path is delivered with `is_partial: false`. When false, all
        /// results are batched into one terminal `PathsLoaded` message.
        streaming: bool,
    },
    BuildColdFileShells {
        request_id: u64,
        paths: Vec<PathBuf>,
        session_store: SessionStore,
    },
    SavePath {
        request_id: u64,
        path: PathBuf,
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
    },
    RestoreSession {
        request_id: u64,
        session_store: SessionStore,
    },
    HydrateSessionTab {
        request_id: u64,
        session_store: SessionStore,
        tab_index: usize,
        cold_session_tab: ColdSessionTab,
    },
    PersistSession {
        request_id: u64,
        session_store: SessionStore,
        request: SessionPersistRequest,
    },
    RefreshTextMetadata {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
    },
    RefreshEncodingCompliance {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        snapshot: DocumentSnapshot,
        format: TextFormatMetadata,
    },
}

impl BackgroundIoRequest {
    pub(in crate::app::services::background_io) fn kind(&self) -> &'static str {
        match self {
            Self::PrepareText { .. } => "prepare_text",
            Self::ReopenRetained { .. } => "reopen_retained",
            Self::LoadPaths { .. } => "load_paths",
            Self::BuildColdFileShells { .. } => "build_cold_file_shells",
            Self::SavePath { .. } => "save_path",
            Self::RestoreSession { .. } => "restore_session",
            Self::HydrateSessionTab { .. } => "hydrate_session_tab",
            Self::PersistSession { .. } => "persist_session",
            Self::RefreshTextMetadata { .. } => "refresh_text_metadata",
            Self::RefreshEncodingCompliance { .. } => "refresh_encoding_compliance",
        }
    }

    pub(in crate::app::services::background_io) fn lane(&self) -> BackgroundIoLane {
        match self {
            Self::PrepareText { .. }
            | Self::LoadPaths { .. }
            | Self::BuildColdFileShells { .. }
            | Self::SavePath { .. }
            | Self::ReopenRetained { .. } => BackgroundIoLane::Path,
            Self::RestoreSession { .. }
            | Self::HydrateSessionTab { .. }
            | Self::PersistSession { .. } => BackgroundIoLane::Session,
            Self::RefreshTextMetadata { .. } | Self::RefreshEncodingCompliance { .. } => {
                BackgroundIoLane::Analysis
            }
        }
    }
}
