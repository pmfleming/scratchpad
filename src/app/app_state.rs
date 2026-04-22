use crate::app::domain::{BufferId, DiskFileState, SplitAxis, TabManager, ViewId};
use crate::app::fonts::EditorFontPreset;
use crate::app::services::background_io::{BackgroundIoRequest, BackgroundIoResult};
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::{AppSettings, SettingsStore};
use crate::app::startup::StartupOptions;
use crate::app::transactions::{
    PendingLayoutTransaction, PendingTextTransaction, TransactionLog, TransactionSnapshot,
};
use eframe::egui;
use search_state::SearchState;
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender};
use std::time::{Duration, Instant};

mod background_io;
mod frame;
mod search_state;
mod settings_state;
mod startup_state;
mod workspace;

pub use search_state::SearchScope;
pub(crate) use search_state::{
    SearchFocusTarget, SearchFreshness, SearchProgress, SearchReplaceAvailability,
    SearchResultEntry, SearchResultGroup, SearchScopeOrigin, SearchStatus,
};

pub(crate) const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);
const CHROME_TRANSITION_FRAMES: u8 = 2;

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

#[derive(Clone)]
pub(crate) struct StartupRestoreConflict {
    pub(crate) tab_index: usize,
    pub(crate) view_id: ViewId,
    pub(crate) buffer_name: String,
    pub(crate) path: PathBuf,
}

pub(crate) struct PendingOpenTabsAction {
    pub(crate) duplicate_count: usize,
    pub(crate) affected_items: Vec<String>,
    pub(crate) transaction_snapshot: TransactionSnapshot,
}

pub(crate) struct PendingOpenHereAction {
    pub(crate) already_here_count: usize,
    pub(crate) migrated_count: usize,
    pub(crate) failure_count: usize,
    pub(crate) affected_items: Vec<String>,
    pub(crate) transaction_snapshot: TransactionSnapshot,
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
}

pub struct ScratchpadApp {
    pub(crate) tab_manager: TabManager,
    pub(crate) app_settings: AppSettings,
    pub(crate) status_message: Option<String>,
    pub(crate) pending_editor_focus: Option<ViewId>,
    pub(crate) encoding_dialog_open: bool,
    pub(crate) encoding_dialog_choice: String,
    pub(crate) settings_store: SettingsStore,
    pub(crate) user_manual_path: PathBuf,
    pub(crate) session_store: SessionStore,
    pub(crate) persist_session_on_drop: bool,
    pub(crate) last_session_persist: Instant,
    pub(crate) close_in_progress: bool,
    pub(crate) overflow_popup_open: bool,
    pub(crate) applied_editor_font: Option<EditorFontPreset>,
    pub(crate) active_surface: AppSurface,
    pub(crate) settings_tab_index: usize,
    pub(crate) pending_settings_toml_refresh: Option<BufferId>,
    pub(crate) vertical_tab_list_open: bool,
    pub(crate) vertical_tab_list_hide_deadline: Option<Instant>,
    pub(crate) transaction_log: TransactionLog,
    pub(crate) transaction_log_open: bool,
    pub(crate) pending_layout_transaction: Option<PendingLayoutTransaction>,
    pub(crate) pending_text_transaction: Option<PendingTextTransaction>,
    pub(crate) search_state: SearchState,
    pub(crate) chrome_transition_frames_remaining: u8,
    pub(crate) selected_tab_slots: BTreeSet<usize>,
    pub(crate) tab_selection_anchor: Option<usize>,
    pub(crate) tab_rename_state: Option<TabRenameState>,
    pub(crate) startup_restore_conflicts: Vec<StartupRestoreConflict>,
    pub(crate) workspace_reflow_axis: SplitAxis,
    pub(crate) settings_preview_quote_index: usize,
    pub(crate) background_io_tx: Sender<BackgroundIoRequest>,
    pub(crate) background_io_rx: Receiver<BackgroundIoResult>,
    pub(crate) next_background_request_id: u64,
    pub(crate) pending_background_actions: HashMap<u64, PendingBackgroundAction>,
}

impl Default for ScratchpadApp {
    fn default() -> Self {
        Self::with_session_store_and_startup(SessionStore::default(), StartupOptions::default())
    }
}

impl eframe::App for ScratchpadApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.handle_pending_close_request(&ctx) {
            return;
        }

        self.prepare_frame(&ctx);
        self.render_frame(ui, &ctx);
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let color = self.editor_background_color();
        [
            f32::from(color.r()) / 255.0,
            f32::from(color.g()) / 255.0,
            f32::from(color.b()) / 255.0,
            f32::from(color.a()) / 255.0,
        ]
    }
}

impl Drop for ScratchpadApp {
    fn drop(&mut self) {
        if self.persist_session_on_drop {
            let _ = self.persist_session_now();
        }
    }
}

impl ScratchpadApp {
    pub(crate) fn set_info_status(&mut self, message: impl Into<String>) {
        self.set_status(message);
    }

    pub(crate) fn set_warning_status(&mut self, message: impl Into<String>) {
        self.set_status(message);
    }

    pub(crate) fn set_error_status(&mut self, message: impl Into<String>) {
        self.set_status(message);
    }

    fn set_status(&mut self, message: impl Into<String>) {
        let message = message.into();
        self.status_message = Some(message);
    }
}
