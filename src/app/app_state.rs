use crate::app::domain::{BufferId, SplitAxis, TabManager};
use crate::app::fonts::EditorFontPreset;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::{AppSettings, AppThemeMode, SettingsStore};
use crate::app::startup::StartupOptions;
use crate::app::text_history::TextHistoryCache;
use eframe::egui;
use search_state::SearchState;
use std::collections::VecDeque;
use std::fmt::Display;
use std::path::PathBuf;
use std::time::{Duration, Instant};

mod background_io;
mod chrome_state;
mod dialog_state;
mod file_watch;
mod focus_state;
pub(crate) mod frame;
mod runtime_state;
mod search_state;
pub(crate) mod settings_state;
mod startup_state;
mod types;
pub(crate) mod workspace;

pub(crate) use chrome_state::ChromeState;
pub(crate) use dialog_state::DialogState;
pub(crate) use focus_state::FocusState;
pub use frame::prepare_context_before_first_frame;
pub(crate) use runtime_state::{BackgroundIoState, FileWatchState};
pub use search_state::SearchScope;
pub(crate) use search_state::api as search_controller;
pub(crate) use search_state::replace as search_replace;
pub(crate) use search_state::runtime as search_runtime;
pub(crate) use search_state::visual as search_visual;
pub(crate) use search_state::{
    SearchFocusTarget, SearchFreshness, SearchProgress, SearchReplaceAvailability,
    SearchResultEntry, SearchResultGroup, SearchScopeOrigin, SearchStatus,
};
pub(crate) use settings_state::mutators as settings_controller;
pub(crate) use types::{
    AppSurface, PendingBackgroundAction, PendingEncodingComplianceAction, PendingOpenHereAction,
    PendingOpenTabsAction, PendingReloadBufferAction, PendingReloadMode,
    PendingReopenWithEncodingAction, PendingSavePathAction, PendingSessionHydrationAction,
    PendingSessionPersistAction, PendingStartupRestoreAction, PendingStartupRestoreCompareAction,
    PendingTabContextMenu, PendingTextMetadataAction, StartupRestoreConflict, TabRenameState,
};
use workspace::display_tabs::WorkspaceSelectionState;
pub(crate) use workspace::lifecycle as workspace_controller;

pub(crate) const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);
const CHROME_TRANSITION_FRAMES: u8 = 2;
const STATUS_HISTORY_LIMIT: usize = 100;
pub(crate) const RECENTLY_CLOSED_FILE_LIMIT: usize = 10;

#[derive(Clone, Debug)]
pub(crate) struct StatusMessage {
    pub(crate) id: u64,
    pub(crate) severity: StatusSeverity,
    pub(crate) domain: StatusDomain,
    pub(crate) text: String,
    pub(crate) detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusSeverity {
    Info,
    Warning,
    Error,
}

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
}

#[derive(Default)]
pub(crate) struct StatusState {
    pub(crate) current: Option<StatusMessage>,
    pub(crate) history: VecDeque<StatusMessage>,
    pub(crate) next_id: u64,
}

pub struct ScratchpadApp {
    pub(crate) tab_manager: TabManager,
    pub(crate) state: ScratchpadAppState,
}

// Stop adding new impl ScratchpadApp blocks except for top-level orchestration.
// New feature code should take the narrow dependency it needs, not &mut ScratchpadApp.
pub struct ScratchpadAppState {
    pub(crate) app_settings: AppSettings,
    pub(crate) status: StatusState,
    pub(crate) focus: FocusState,
    pub(crate) dialogs: DialogState,
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
    pub(crate) applied_theme_mode: Option<AppThemeMode>,
    pub(crate) chrome: ChromeState,
    pub(crate) settings_tab_index: usize,
    pub(crate) pending_settings_toml_refresh: Option<BufferId>,
    pub(crate) text_history_cache: TextHistoryCache,
    pub(crate) search_state: SearchState,
    pub(crate) workspace_selection: WorkspaceSelectionState,
    pub(crate) pending_open_file_paths: Vec<PathBuf>,
    pub(crate) recently_closed_files: VecDeque<PathBuf>,
    pub(crate) workspace_reflow_axis: SplitAxis,
    pub(crate) settings_preview_quote_index: usize,
    pub(crate) background_io: BackgroundIoState,
    pub(crate) file_watch: FileWatchState,
}

impl Default for ScratchpadApp {
    fn default() -> Self {
        Self::with_session_store_and_startup(SessionStore::default(), StartupOptions::default())
    }
}

impl eframe::App for ScratchpadApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if frame::handle_pending_close_request(self, &ctx) {
            return;
        }

        let frame_started_at = std::time::Instant::now();
        frame::prepare_frame(self, &ctx);
        frame::render_frame(self, ui, &ctx);
        crate::app::capacity_metrics::record_frame(frame_started_at.elapsed());
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        let color = self.state.app_settings.editor_background_color();
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
        if self.state.persist_session_on_drop {
            let _ = crate::app::app_state::workspace::accessors::persist_session_now(self);
        }
    }
}

impl StatusState {
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
        );
    }

    pub(crate) fn set_error_status_with_detail(
        &mut self,
        domain: StatusDomain,
        message: impl Into<String>,
        detail: impl Into<String>,
    ) {
        self.set_status(StatusSeverity::Error, domain, message, Some(detail.into()));
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

    pub(crate) fn clear_current(&mut self) {
        self.current = None;
    }

    fn set_status(
        &mut self,
        severity: StatusSeverity,
        domain: StatusDomain,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        let status = StatusMessage {
            id: self.next_id,
            severity,
            domain,
            text: message.into(),
            detail,
        };
        self.next_id = self.next_id.saturating_add(1);
        self.current = Some(status.clone());
        self.history.push_back(status);
        while self.history.len() > STATUS_HISTORY_LIMIT {
            self.history.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{STATUS_HISTORY_LIMIT, ScratchpadApp, StatusSeverity, StatusState};

    fn app_without_startup_status() -> ScratchpadApp {
        let mut app = ScratchpadApp::default();
        app.state.status = StatusState::default();
        app
    }

    fn primary_texts_are_clean(messages: &[&str]) {
        for message in messages {
            assert!(!message.contains("file(s)"), "{message}");
            assert!(!message.contains("tab(s)"), "{message}");
            assert!(!message.contains("conflict(s)"), "{message}");
            assert!(!message.contains("1 files"), "{message}");
            assert!(!message.contains("1 tabs"), "{message}");
            assert!(!message.contains("1 conflicts"), "{message}");
            assert!(
                !message.contains("Control characters detected: Control characters detected"),
                "{message}"
            );
        }
    }

    #[test]
    fn status_setters_preserve_severity() {
        let mut app = app_without_startup_status();

        app.state
            .status
            .set_info_status_in_domain(super::StatusDomain::Disk, "Saved.");
        assert_eq!(
            app.state
                .status
                .current
                .as_ref()
                .map(|status| status.severity),
            Some(StatusSeverity::Info)
        );

        app.state
            .status
            .set_warning_status_in_domain(super::StatusDomain::Disk, "Changed on disk.");
        assert_eq!(
            app.state
                .status
                .current
                .as_ref()
                .map(|status| status.severity),
            Some(StatusSeverity::Warning)
        );

        app.state
            .status
            .set_error_status_in_domain(super::StatusDomain::Disk, "Could not save.");
        assert_eq!(
            app.state
                .status
                .current
                .as_ref()
                .map(|status| status.severity),
            Some(StatusSeverity::Error)
        );
    }

    #[test]
    fn setting_status_pushes_to_history() {
        let mut app = app_without_startup_status();

        app.state
            .status
            .set_info_status_in_domain(super::StatusDomain::Disk, "Saved.");
        app.state
            .status
            .set_warning_status_in_domain(super::StatusDomain::Disk, "Changed on disk.");

        assert_eq!(app.state.status.history.len(), 2);
        assert_eq!(app.state.status.history[0].text, "Saved.");
        assert_eq!(app.state.status.history[1].text, "Changed on disk.");
        assert_eq!(
            app.state
                .status
                .current
                .as_ref()
                .map(|status| status.text.as_str()),
            Some("Changed on disk.")
        );
    }

    #[test]
    fn status_history_is_capped() {
        let mut app = app_without_startup_status();

        for index in 0..(STATUS_HISTORY_LIMIT + 5) {
            app.state
                .status
                .set_info_status_in_domain(super::StatusDomain::Disk, format!("Message {index}."));
        }

        assert_eq!(app.state.status.history.len(), STATUS_HISTORY_LIMIT);
        assert_eq!(
            app.state.status.history.front().map(|status| status.id),
            Some(5)
        );
        assert_eq!(
            app.state.status.history.back().map(|status| status.id),
            Some((STATUS_HISTORY_LIMIT + 4) as u64)
        );
    }

    #[test]
    fn clearing_current_status_keeps_history() {
        let mut app = app_without_startup_status();
        app.state
            .status
            .set_error_status_in_domain(super::StatusDomain::Disk, "Could not save.");

        crate::app::app_state::workspace::accessors::clear_status_message(&mut app);

        assert!(app.state.status.current.is_none());
        assert_eq!(app.state.status.history.len(), 1);
        assert_eq!(app.state.status.history[0].text, "Could not save.");
    }

    #[test]
    fn common_status_helpers_keep_raw_errors_in_detail() {
        let mut app = app_without_startup_status();

        app.state.status.report_session_save_failed("access denied");
        assert_eq!(
            app.state
                .status
                .current
                .as_ref()
                .map(|status| status.text.as_str()),
            Some("Could not save your session.")
        );
        assert_eq!(
            app.state
                .status
                .current
                .as_ref()
                .and_then(|status| status.detail.as_deref()),
            Some("access denied")
        );

        app.state
            .status
            .report_settings_toml_parse_failed("expected key");
        assert_eq!(
            app.state
                .status
                .current
                .as_ref()
                .map(|status| status.text.as_str()),
            Some("Could not apply settings.toml.")
        );
        assert_eq!(
            app.state
                .status
                .current
                .as_ref()
                .and_then(|status| status.detail.as_deref()),
            Some("expected key")
        );
    }

    #[test]
    fn common_status_helpers_avoid_fragile_vocabulary() {
        let mut app = app_without_startup_status();

        app.state.status.report_session_save_failed("disk full");
        let session_save = app.state.status.current.as_ref().unwrap().text.clone();
        app.state
            .status
            .report_settings_toml_parse_failed("bad toml");
        let settings_toml = app.state.status.current.as_ref().unwrap().text.clone();
        app.state.status.report_search_results_stale_for_replace();
        let stale_search = app.state.status.current.as_ref().unwrap().text.clone();

        primary_texts_are_clean(&[&session_save, &settings_toml, &stale_search]);
    }
}
