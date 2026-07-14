use crate::app::domain::{BufferId, DiskFileState, TextFormatMetadata, ViewId};
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
    pub(crate) click_y: f32,
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
    pub(crate) restore_started: bool,
    pub(crate) streamed_tab_count: usize,
    pub(crate) streamed_tab_indices: Vec<usize>,
}

pub(crate) struct PendingSessionHydrationAction {
    pub(crate) expected_buffer_ids: Vec<BufferId>,
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

pub(crate) struct PendingSavePathAction {
    pub(crate) buffer_id: BufferId,
    pub(crate) expected_path: PathBuf,
    pub(crate) previous_path: Option<PathBuf>,
    pub(crate) buffer_name: String,
    pub(crate) saved_revision: u64,
    pub(crate) update_buffer_path: bool,
    pub(crate) format_override: Option<TextFormatMetadata>,
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
    HydrateSessionTab(PendingSessionHydrationAction),
    ReloadBuffer(PendingReloadBufferAction),
    ReopenWithEncoding(PendingReopenWithEncodingAction),
    SavePath(PendingSavePathAction),
    StartupRestoreCompare(PendingStartupRestoreCompareAction),
    PersistSession(PendingSessionPersistAction),
    RefreshTextMetadata(PendingTextMetadataAction),
    RefreshEncodingCompliance(PendingEncodingComplianceAction),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PendingActionKind {
    OpenTabs,
    OpenHere,
    StartupRestore,
    HydrateSessionTab,
    ReloadBuffer,
    ReopenWithEncoding,
    SavePath,
    StartupRestoreCompare,
    PersistSession,
    RefreshTextMetadata,
    RefreshEncodingCompliance,
}

macro_rules! pending_action_extractors {
    ($($method:ident, $variant:ident, $ty:ty;)+) => {
        $(
            pub(crate) fn $method(self) -> Option<$ty> {
                match self {
                    Self::$variant(action) => Some(action),
                    _ => None,
                }
            }
        )+
    };
}

impl PendingBackgroundAction {
    pub(crate) fn kind(&self) -> PendingActionKind {
        match self {
            Self::OpenTabs(_) => PendingActionKind::OpenTabs,
            Self::OpenHere(_) => PendingActionKind::OpenHere,
            Self::StartupRestore(_) => PendingActionKind::StartupRestore,
            Self::HydrateSessionTab(_) => PendingActionKind::HydrateSessionTab,
            Self::ReloadBuffer(_) => PendingActionKind::ReloadBuffer,
            Self::ReopenWithEncoding(_) => PendingActionKind::ReopenWithEncoding,
            Self::SavePath(_) => PendingActionKind::SavePath,
            Self::StartupRestoreCompare(_) => PendingActionKind::StartupRestoreCompare,
            Self::PersistSession(_) => PendingActionKind::PersistSession,
            Self::RefreshTextMetadata(_) => PendingActionKind::RefreshTextMetadata,
            Self::RefreshEncodingCompliance(_) => PendingActionKind::RefreshEncodingCompliance,
        }
    }

    pending_action_extractors! {
        into_save_path, SavePath, PendingSavePathAction;
        into_startup_restore, StartupRestore, PendingStartupRestoreAction;
        into_session_hydration, HydrateSessionTab, PendingSessionHydrationAction;
        into_session_persist, PersistSession, PendingSessionPersistAction;
        into_text_metadata, RefreshTextMetadata, PendingTextMetadataAction;
        into_encoding_compliance, RefreshEncodingCompliance, PendingEncodingComplianceAction;
    }

    pub(crate) fn as_open_tabs_mut(&mut self) -> Option<&mut PendingOpenTabsAction> {
        match self {
            Self::OpenTabs(action) => Some(action),
            _ => None,
        }
    }

    pub(crate) fn as_startup_restore_mut(&mut self) -> Option<&mut PendingStartupRestoreAction> {
        match self {
            Self::StartupRestore(action) => Some(action),
            _ => None,
        }
    }
}
