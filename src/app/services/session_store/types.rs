use super::model::SessionActiveSurface;
use crate::app::domain::WorkspaceTab;
use crate::app::services::settings_store::AppSettings;
pub struct RestoredSession {
    pub tabs: Vec<WorkspaceTab>,
    pub active_tab_index: usize,
    pub active_surface: SessionActiveSurface,
    pub legacy_settings: AppSettings,
    pub restore_status: Option<RestoreStatus>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SessionPersistProfile {
    pub total_ns: u128,
    pub snapshot_capture_ns: u128,
    pub snapshot_write_ns: u128,
    pub snapshot_bytes_read: u64,
    pub snapshot_bytes_hashed: u64,
    pub stale_cleanup_ns: u128,
    pub manifest_serialize_ns: u128,
    pub manifest_write_ns: u128,
    pub tab_count: usize,
    pub buffer_count: usize,
    pub manifest_size_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SessionRestoreProfile {
    pub total_ns: u128,
    pub manifest_read_parse_ns: u128,
    pub restore_reconstruction_ns: u128,
    pub tab_count: usize,
    pub buffer_count: usize,
}

pub struct ProfiledRestoredSession {
    pub restored: Option<RestoredSession>,
    pub profile: SessionRestoreProfile,
}

#[derive(Clone)]
pub struct RestoreStatus {
    pub level: RestoreStatusLevel,
    pub message: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RestoreStatusLevel {
    Info,
    Warning,
}
