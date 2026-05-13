use crate::app::capacity_metrics::BackgroundIoLane;
use crate::app::domain::buffer::BufferLength;
use crate::app::domain::{
    BufferState, DiskFileState, DocumentSnapshot, TextArtifactSummary, TextFormatMetadata,
    WorkspaceTab,
};
use crate::app::services::session_store::{
    ColdSessionTab, RestoreStatus, RestoredSession, SessionActiveSurface, SessionPersistRequest,
    SessionStore,
};
use crate::app::services::settings_store::AppSettings;
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
            Self::Standard(path) => path,
            Self::WithEncoding { path, .. } => path,
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
    pub(super) fn kind(&self) -> &'static str {
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

    pub(super) fn lane(&self) -> BackgroundIoLane {
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

pub(crate) enum BackgroundIoResult {
    PathsLoaded {
        request_id: u64,
        results: Vec<LoadedPathResult>,
        /// When true, more `PathsLoaded` messages will follow for this
        /// `request_id`; the action stays in `pending_background_actions`.
        /// When false (terminal), the action is removed and finalized.
        is_partial: bool,
    },
    PathSaved {
        request_id: u64,
        path: PathBuf,
        disk_state: Option<DiskFileState>,
        result: Result<(), String>,
    },
    SessionRestored {
        request_id: u64,
        result: Result<Option<RestoredSession>, String>,
    },
    SessionRestoreStarted {
        request_id: u64,
        active_tab_index: usize,
        active_surface: SessionActiveSurface,
        legacy_settings: AppSettings,
    },
    SessionTabRestored {
        request_id: u64,
        tab_index: usize,
        cold_session_tab: Option<crate::app::services::session_store::ColdSessionTab>,
        tab: Box<WorkspaceTab>,
    },
    SessionTabHydrated {
        request_id: u64,
        tab_index: usize,
        restore_status: Option<RestoreStatus>,
        tab: Box<WorkspaceTab>,
    },
    SessionPersisted {
        request_id: u64,
        result: Result<(), String>,
    },
    TextMetadataRefreshed {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        result: Result<(BufferLength, usize, TextArtifactSummary, TextFormatMetadata), String>,
    },
    EncodingComplianceRefreshed {
        request_id: u64,
        buffer_id: u64,
        revision: u64,
        result: Result<bool, String>,
    },
}

pub(crate) struct LoadedPathResult {
    pub(crate) path: PathBuf,
    pub(crate) disk_state: Option<DiskFileState>,
    pub(crate) result: Result<BufferState, String>,
}
