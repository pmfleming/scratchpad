use crate::app::domain::{BufferId, SplitAxis, TabManager, ViewId};
use crate::app::fonts::EditorFontPreset;
use crate::app::logging::{self, LogLevel};
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::{AppSettings, SettingsStore};
use crate::app::startup::StartupOptions;
use crate::app::transactions::{PendingTextTransaction, TransactionLog};
use eframe::egui;
use search_state::SearchState;
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod frame;
mod search_state;
mod settings_state;
mod startup_state;
mod workspace;

pub use search_state::SearchScope;
pub(crate) use search_state::{
    SearchFocusTarget, SearchProgress, SearchResultEntry, SearchResultGroup,
};

pub(crate) const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);
const CHROME_TRANSITION_FRAMES: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppSurface {
    Workspace,
    Settings,
}

pub struct ScratchpadApp {
    pub(crate) tab_manager: TabManager,
    pub(crate) app_settings: AppSettings,
    pub(crate) status_message: Option<String>,
    pub(crate) pending_editor_focus: Option<ViewId>,
    pub(crate) encoding_dialog_open: bool,
    pub(crate) reopen_with_encoding_choice: String,
    pub(crate) save_with_encoding_choice: String,
    pub(crate) settings_store: SettingsStore,
    pub(crate) user_manual_path: PathBuf,
    pub(crate) session_store: SessionStore,
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
    pub(crate) pending_text_transaction: Option<PendingTextTransaction>,
    pub(crate) search_state: SearchState,
    pub(crate) chrome_transition_frames_remaining: u8,
    pub(crate) selected_tab_slots: BTreeSet<usize>,
    pub(crate) tab_selection_anchor: Option<usize>,
    pub(crate) workspace_reflow_axis: SplitAxis,
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
}

impl Drop for ScratchpadApp {
    fn drop(&mut self) {
        let _ = self.persist_session_now();
    }
}

impl ScratchpadApp {
    pub(crate) fn set_info_status(&mut self, message: impl Into<String>) {
        self.set_status(LogLevel::Info, message);
    }

    pub(crate) fn set_warning_status(&mut self, message: impl Into<String>) {
        self.set_status(LogLevel::Warn, message);
    }

    pub(crate) fn set_error_status(&mut self, message: impl Into<String>) {
        self.set_status(LogLevel::Error, message);
    }

    fn set_status(&mut self, level: LogLevel, message: impl Into<String>) {
        let message = message.into();
        self.status_message = Some(message.clone());
        if self.app_settings.logging_enabled {
            logging::log(level, &message);
        }
    }
}
