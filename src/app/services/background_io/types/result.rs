use crate::app::domain::buffer::BufferLength;
use crate::app::domain::{
    BufferState, DiskFileState, TextArtifactSummary, TextFormatMetadata, WorkspaceTab,
};
use crate::app::services::session_store::{
    ColdSessionTab, RestoreStatus, RestoredSession, SessionActiveSurface,
};
use crate::app::services::settings_store::AppSettings;
use std::path::PathBuf;

pub(crate) enum BackgroundIoResult {
    PathsLoaded {
        request_id: u64,
        results: Vec<LoadedPathResult>,
        /// When true, more `PathsLoaded` messages will follow for this
        /// `request_id`; the action stays in `pending_background_actions`.
        /// When false (terminal), the action is removed and finalized.
        is_partial: bool,
    },
    ColdFileShellsBuilt {
        request_id: u64,
        shells: Vec<ColdFileShellResult>,
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

pub(crate) struct ColdFileShellResult {
    pub(crate) path: PathBuf,
    pub(crate) result: Result<(WorkspaceTab, ColdSessionTab), String>,
}

pub(crate) struct LoadedPathResult {
    pub(crate) path: PathBuf,
    pub(crate) disk_state: Option<DiskFileState>,
    pub(crate) result: Result<BufferState, String>,
}
