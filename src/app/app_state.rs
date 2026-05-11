use crate::app::domain::{BufferId, SplitAxis, TabManager, ViewId};
use crate::app::fonts::EditorFontPreset;
use crate::app::services::background_io::{BackgroundIoDispatcher, BackgroundIoResult};
use crate::app::services::file_watch::FileWatchService;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::{AppSettings, SettingsStore};
use crate::app::startup::StartupOptions;
use crate::app::text_history::TextHistoryCache;
use eframe::egui;
use search_state::SearchState;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::fmt::Display;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant, SystemTime};

mod background_io;
mod file_watch;
mod frame;
mod search_state;
mod settings_state;
mod startup_state;
mod types;
mod workspace;

pub use search_state::SearchScope;
pub(crate) use search_state::{
    SearchFocusTarget, SearchFreshness, SearchProgress, SearchReplaceAvailability,
    SearchResultEntry, SearchResultGroup, SearchScopeOrigin, SearchStatus,
};
pub(crate) use types::{
    AppSurface, PendingBackgroundAction, PendingEncodingComplianceAction, PendingOpenHereAction,
    PendingOpenTabsAction, PendingReloadBufferAction, PendingReloadMode,
    PendingReopenWithEncodingAction, PendingSessionPersistAction, PendingStartupRestoreAction,
    PendingStartupRestoreCompareAction, PendingTabContextMenu, PendingTextMetadataAction,
    StartupRestoreConflict, TabRenameState,
};

pub(crate) const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);
const CHROME_TRANSITION_FRAMES: u8 = 2;
const STATUS_HISTORY_LIMIT: usize = 100;

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub(crate) struct StatusMessage {
    pub(crate) id: u64,
    pub(crate) created_at: SystemTime,
    pub(crate) severity: StatusSeverity,
    pub(crate) domain: StatusDomain,
    pub(crate) text: String,
    pub(crate) detail: Option<String>,
    pub(crate) action: Option<StatusAction>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusSeverity {
    Info,
    Warning,
    Error,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusDomain {
    File,
    Disk,
    Search,
    Settings,
    Session,
    Encoding,
    History,
    Layout,
    App,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StatusAction {
    OpenSettings,
}

pub struct ScratchpadApp {
    pub(crate) tab_manager: TabManager,
    pub(crate) app_settings: AppSettings,
    pub(crate) current_status: Option<StatusMessage>,
    pub(crate) status_history: VecDeque<StatusMessage>,
    pub(crate) next_status_message_id: u64,
    pub(crate) pending_editor_focus: Option<ViewId>,
    pub(crate) encoding_dialog_open: bool,
    pub(crate) encoding_dialog_choice: String,
    pub(crate) settings_store: SettingsStore,
    pub(crate) user_manual_path: PathBuf,
    pub(crate) session_store: SessionStore,
    pub(crate) persist_session_on_drop: bool,
    pub(crate) last_session_persist: Instant,
    pub(crate) close_in_progress: bool,
    pub(crate) window_shown_after_first_frame: bool,
    pub(crate) painted_frames_before_window_show: u8,
    pub(crate) current_window_title: Option<String>,
    pub(crate) overflow_popup_open: bool,
    pub(crate) applied_editor_font: Option<EditorFontPreset>,
    pub(crate) active_surface: AppSurface,
    pub(crate) settings_tab_index: usize,
    pub(crate) pending_settings_toml_refresh: Option<BufferId>,
    pub(crate) pending_status_bar_visible: Option<bool>,
    pub(crate) vertical_tab_list_open: bool,
    pub(crate) vertical_tab_list_hide_deadline: Option<Instant>,
    pub(crate) text_history_cache: TextHistoryCache,
    pub(crate) text_history_open: bool,
    pub(crate) status_history_open: bool,
    pub(crate) search_state: SearchState,
    pub(crate) chrome_transition_frames_remaining: u8,
    pub(crate) selected_tab_slots: BTreeSet<usize>,
    pub(crate) tab_selection_anchor: Option<usize>,
    pub(crate) tab_rename_state: Option<TabRenameState>,
    pub(crate) pending_tab_context_menu: Option<PendingTabContextMenu>,
    pub(crate) startup_restore_conflicts: Vec<StartupRestoreConflict>,
    pub(crate) workspace_reflow_axis: SplitAxis,
    pub(crate) settings_preview_quote_index: usize,
    pub(crate) background_io_tx: BackgroundIoDispatcher,
    pub(crate) background_io_rx: Receiver<BackgroundIoResult>,
    pub(crate) next_background_request_id: u64,
    pub(crate) pending_background_actions: HashMap<u64, PendingBackgroundAction>,
    pub(crate) file_watch_service: FileWatchService,
    pub(crate) pending_file_watch_rescans: HashMap<PathBuf, Instant>,
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

        let frame_started_at = std::time::Instant::now();
        self.prepare_frame(&ctx);
        self.render_frame(ui, &ctx);
        crate::app::capacity_metrics::record_frame(frame_started_at.elapsed());
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

    fn persist_egui_memory(&self) -> bool {
        false
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
    pub(crate) fn set_info_status_in_domain(
        &mut self,
        domain: StatusDomain,
        message: impl Into<String>,
    ) {
        self.set_status(
            StatusSeverity::Info,
            domain,
            message,
            Option::<String>::None,
            None,
        );
    }

    pub(crate) fn set_warning_status_in_domain(
        &mut self,
        domain: StatusDomain,
        message: impl Into<String>,
    ) {
        self.set_status(
            StatusSeverity::Warning,
            domain,
            message,
            Option::<String>::None,
            None,
        );
    }

    pub(crate) fn set_error_status_in_domain(
        &mut self,
        domain: StatusDomain,
        message: impl Into<String>,
    ) {
        self.set_status(
            StatusSeverity::Error,
            domain,
            message,
            Option::<String>::None,
            None,
        );
    }

    pub(crate) fn set_warning_status_with_detail(
        &mut self,
        domain: StatusDomain,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.set_status(
            StatusSeverity::Warning,
            domain,
            message,
            Some(detail.into()),
            None,
        );
    }

    pub(crate) fn set_error_status_with_detail(
        &mut self,
        domain: StatusDomain,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.set_status(
            StatusSeverity::Error,
            domain,
            message,
            Some(detail.into()),
            None,
        );
    }

    pub(crate) fn report_session_save_failed(&mut self, error: impl Display) {
        self.set_error_status_with_detail(
            StatusDomain::Session,
            "Could not save your session.",
            error.to_string(),
        );
    }

    pub(crate) fn report_session_restore_failed(&mut self, error: impl Display) {
        self.set_error_status_with_detail(
            StatusDomain::Session,
            "Could not restore your previous session.",
            error.to_string(),
        );
    }

    pub(crate) fn report_settings_load_failed(&mut self, error: impl Display) {
        self.set_warning_status_with_detail(
            StatusDomain::Settings,
            "Could not load your settings. Defaults are in use.",
            error.to_string(),
        );
    }

    pub(crate) fn report_settings_save_failed(&mut self, error: impl Display) {
        self.set_error_status_with_detail(
            StatusDomain::Settings,
            "Could not save your settings.",
            error.to_string(),
        );
    }

    pub(crate) fn report_settings_toml_parse_failed(&mut self, error: impl Display) {
        self.set_warning_status_with_detail(
            StatusDomain::Settings,
            "Could not apply settings.toml.",
            error.to_string(),
        );
    }

    pub(crate) fn report_search_results_stale_for_replace(&mut self) {
        self.set_error_status_in_domain(
            StatusDomain::Search,
            "Search results changed. Run search again before replacing.",
        );
    }

    pub(crate) fn report_save_failed(&mut self, error: impl Display) {
        self.set_error_status_with_detail(
            StatusDomain::Disk,
            "Could not save this file.",
            error.to_string(),
        );
    }

    pub(crate) fn report_reload_failed(&mut self, error: impl Display) {
        self.set_error_status_with_detail(
            StatusDomain::Disk,
            "Could not reload this file.",
            error.to_string(),
        );
    }

    fn set_status(
        &mut self,
        severity: StatusSeverity,
        domain: StatusDomain,
        message: impl Into<String>,
        detail: Option<String>,
        action: Option<StatusAction>,
    ) {
        let status = StatusMessage {
            id: self.next_status_message_id,
            created_at: SystemTime::now(),
            severity,
            domain,
            text: message.into(),
            detail,
            action,
        };
        self.next_status_message_id = self.next_status_message_id.saturating_add(1);
        self.current_status = Some(status.clone());
        self.status_history.push_back(status);
        while self.status_history.len() > STATUS_HISTORY_LIMIT {
            self.status_history.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{STATUS_HISTORY_LIMIT, ScratchpadApp, StatusSeverity};

    fn app_without_startup_status() -> ScratchpadApp {
        let mut app = ScratchpadApp::default();
        app.current_status = None;
        app.status_history.clear();
        app.next_status_message_id = 0;
        app
    }

    fn primary_texts_are_clean(messages: &[&str]) {
        for message in messages {
            assert!(!message.contains("file(s)"), "{message}");
            assert!(!message.contains("tab(s)"), "{message}");
            assert!(!message.contains("conflict(s)"), "{message}");
            assert!(
                !message.contains("Control characters detected: Control characters detected"),
                "{message}"
            );
        }
    }

    #[test]
    fn status_setters_preserve_severity() {
        let mut app = app_without_startup_status();

        app.set_info_status_in_domain(super::StatusDomain::App, "Saved.");
        assert_eq!(
            app.current_status.as_ref().map(|status| status.severity),
            Some(StatusSeverity::Info)
        );

        app.set_warning_status_in_domain(super::StatusDomain::Disk, "Changed on disk.");
        assert_eq!(
            app.current_status.as_ref().map(|status| status.severity),
            Some(StatusSeverity::Warning)
        );

        app.set_error_status_in_domain(super::StatusDomain::Disk, "Could not save.");
        assert_eq!(
            app.current_status.as_ref().map(|status| status.severity),
            Some(StatusSeverity::Error)
        );
    }

    #[test]
    fn setting_status_pushes_to_history() {
        let mut app = app_without_startup_status();

        app.set_info_status_in_domain(super::StatusDomain::Disk, "Saved.");
        app.set_warning_status_in_domain(super::StatusDomain::Disk, "Changed on disk.");

        assert_eq!(app.status_history.len(), 2);
        assert_eq!(app.status_history[0].text, "Saved.");
        assert_eq!(app.status_history[1].text, "Changed on disk.");
        assert_eq!(
            app.current_status
                .as_ref()
                .map(|status| status.text.as_str()),
            Some("Changed on disk.")
        );
    }

    #[test]
    fn status_history_is_capped() {
        let mut app = app_without_startup_status();

        for index in 0..(STATUS_HISTORY_LIMIT + 5) {
            app.set_info_status_in_domain(super::StatusDomain::App, format!("Message {index}."));
        }

        assert_eq!(app.status_history.len(), STATUS_HISTORY_LIMIT);
        assert_eq!(app.status_history.front().map(|status| status.id), Some(5));
        assert_eq!(
            app.status_history.back().map(|status| status.id),
            Some((STATUS_HISTORY_LIMIT + 4) as u64)
        );
    }

    #[test]
    fn clearing_current_status_keeps_history() {
        let mut app = app_without_startup_status();
        app.set_error_status_in_domain(super::StatusDomain::Disk, "Could not save.");

        app.clear_status_message();

        assert!(app.current_status.is_none());
        assert_eq!(app.status_history.len(), 1);
        assert_eq!(app.status_history[0].text, "Could not save.");
    }

    #[test]
    fn common_status_helpers_keep_raw_errors_in_detail() {
        let mut app = app_without_startup_status();

        app.report_session_save_failed("access denied");
        assert_eq!(
            app.current_status
                .as_ref()
                .map(|status| status.text.as_str()),
            Some("Could not save your session.")
        );
        assert_eq!(
            app.current_status
                .as_ref()
                .and_then(|status| status.detail.as_deref()),
            Some("access denied")
        );

        app.report_settings_toml_parse_failed("expected key");
        assert_eq!(
            app.current_status
                .as_ref()
                .map(|status| status.text.as_str()),
            Some("Could not apply settings.toml.")
        );
        assert_eq!(
            app.current_status
                .as_ref()
                .and_then(|status| status.detail.as_deref()),
            Some("expected key")
        );
    }

    #[test]
    fn common_status_helpers_avoid_fragile_vocabulary() {
        let mut app = app_without_startup_status();

        app.report_session_save_failed("disk full");
        let session_save = app.current_status.as_ref().unwrap().text.clone();
        app.report_settings_toml_parse_failed("bad toml");
        let settings_toml = app.current_status.as_ref().unwrap().text.clone();
        app.report_search_results_stale_for_replace();
        let stale_search = app.current_status.as_ref().unwrap().text.clone();

        primary_texts_are_clean(&[&session_save, &settings_toml, &stale_search]);
    }
}
