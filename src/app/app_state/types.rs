use crate::app::domain::{BufferId, DiskFileState, ViewId};
use crate::app::services::file_controller::OpenBatchSummary;
use crate::app::startup::StartupOptions;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppSurface {
    Workspace,
    Settings,
}

pub(crate) struct TabRenameState {
    pub(crate) buffer_id: BufferId,
    pub(crate) draft: String,
    pub(crate) request_focus: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct PendingTabContextMenu {
    pub(crate) slot_index: usize,
    pub(crate) click_x: f32,
    pub(crate) open: bool,
}

#[derive(Clone)]
pub(crate) struct StartupRestoreConflict {
    pub(crate) tab_index: usize,
    pub(crate) view_id: ViewId,
    pub(crate) buffer_name: String,
    pub(crate) path: PathBuf,
}

pub(crate) struct PendingOpenTabsAction {
    /// Streaming accumulator: filled in as individual paths arrive across
    /// multiple `PathsLoaded { is_partial: true }` messages. Finalized when
    /// the terminating `is_partial: false` message arrives.
    pub(crate) accumulator: OpenBatchSummary,
}

pub(crate) struct PendingOpenHereAction {
    pub(crate) already_here_count: usize,
    pub(crate) migrated_count: usize,
    pub(crate) failure_count: usize,
    pub(crate) anchor_view_id: Option<ViewId>,
}

pub(crate) struct PendingStartupRestoreAction {
    pub(crate) startup_options: StartupOptions,
    pub(crate) loaded_from_settings: bool,
}

pub(crate) struct PendingReloadBufferAction {
    pub(crate) buffer_id: BufferId,
    pub(crate) expected_path: PathBuf,
    pub(crate) buffer_name: String,
    pub(crate) previous_disk_state: Option<DiskFileState>,
    pub(crate) mode: PendingReloadMode,
}

pub(crate) struct PendingReopenWithEncodingAction {
    pub(crate) buffer_id: BufferId,
    pub(crate) expected_path: PathBuf,
    pub(crate) buffer_name: String,
}

pub(crate) struct PendingStartupRestoreCompareAction {
    pub(crate) conflict: StartupRestoreConflict,
}

pub(crate) struct PendingSessionPersistAction;

pub(crate) struct PendingEncodingComplianceAction {
    pub(crate) buffer_id: BufferId,
    pub(crate) revision: u64,
}

pub(crate) struct PendingTextMetadataAction {
    pub(crate) buffer_id: BufferId,
    pub(crate) revision: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingReloadMode {
    AutoRefreshCleanBuffer,
    ExplicitReload,
}

pub(crate) enum PendingBackgroundAction {
    OpenTabs(PendingOpenTabsAction),
    OpenHere(PendingOpenHereAction),
    StartupRestore(PendingStartupRestoreAction),
    ReloadBuffer(PendingReloadBufferAction),
    ReopenWithEncoding(PendingReopenWithEncodingAction),
    StartupRestoreCompare(PendingStartupRestoreCompareAction),
    PersistSession(PendingSessionPersistAction),
    RefreshTextMetadata(PendingTextMetadataAction),
    RefreshEncodingCompliance(PendingEncodingComplianceAction),
}
