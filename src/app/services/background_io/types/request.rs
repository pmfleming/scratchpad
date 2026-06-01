use crate::app::capacity_metrics::BackgroundIoLane;
use crate::app::domain::{DocumentSnapshot, TextFormatMetadata};
use crate::app::services::session_store::{ColdSessionTab, SessionPersistRequest, SessionStore};
use std::path::PathBuf;

pub(crate) enum PathLoadRequest {
    Standard(PathBuf),
    WithEncoding {
        path: PathBuf,
        encoding_name: String,
    },
}

impl PathLoadRequest {
    pub(crate) fn path(&self) -> &PathBuf {
        match self {
            Self::Standard(path) | Self::WithEncoding { path, .. } => path,
        }
    }
}

pub(crate) enum BackgroundIoRequest {
    LoadPaths {
        request_id: u64,
        requests: Vec<PathLoadRequest>,
        /// When true, each path's `LoadedPathResult` is streamed back as a
        /// separate `PathsLoaded { is_partial: true }` message; the final
        /// path is delivered with `is_partial: false`. When false, all
        /// results are batched into one terminal `PathsLoaded` message.
        streaming: bool,
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
            Self::LoadPaths { .. } => "load_paths",
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
            Self::LoadPaths { .. } | Self::SavePath { .. } => BackgroundIoLane::Path,
            Self::RestoreSession { .. }
            | Self::HydrateSessionTab { .. }
            | Self::PersistSession { .. } => BackgroundIoLane::Session,
            Self::RefreshTextMetadata { .. } | Self::RefreshEncodingCompliance { .. } => {
                BackgroundIoLane::Analysis
            }
        }
    }
}
