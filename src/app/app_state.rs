use crate::app::chrome::handle_window_resize;
use crate::app::commands::AppCommand;
use crate::app::domain::{
    EditorViewState, PendingAction, SplitAxis, SplitPath, TabManager, ViewId, WorkspaceTab,
};
use crate::app::logging::{self, LogLevel};
use crate::app::services::file_controller::FileController;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionStore;
use crate::app::shortcuts;
use crate::app::ui::{dialogs, editor_area, status_bar, tab_strip};
use eframe::egui;
use std::path::Path;
use std::time::{Duration, Instant};

pub(crate) const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

pub struct ScratchpadApp {
    pub(crate) tab_manager: TabManager,
    pub(crate) font_size: f32,
    pub(crate) word_wrap: bool,
    pub(crate) logging_enabled: bool,
    pub(crate) status_message: Option<String>,
    pub(crate) session_store: SessionStore,
    pub(crate) last_session_persist: Instant,
    pub(crate) close_in_progress: bool,
    pub(crate) overflow_popup_open: bool,
}

impl Default for ScratchpadApp {
    fn default() -> Self {
        Self::with_session_store(SessionStore::default())
    }
}

impl eframe::App for ScratchpadApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|input| input.viewport().close_requested()) && !self.close_in_progress {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_exit(&ctx);
            return;
        }

        handle_window_resize(&ctx);
        session_manager::maybe_persist_session(self, &ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        tab_strip::show_header(ui, self);
        status_bar::show_status_bar(ui, self);
        editor_area::show_editor(ui, self);
        dialogs::show_pending_action_modal(&ctx, self);
        shortcuts::handle_shortcuts(self, &ctx);
        let _ = frame;
    }
}

impl Drop for ScratchpadApp {
    fn drop(&mut self) {
        let _ = self.persist_session_now();
    }
}

impl ScratchpadApp {
    pub fn with_session_store(session_store: SessionStore) -> Self {
        let mut app = Self {
            tab_manager: TabManager::default(),
            font_size: 14.0,
            word_wrap: true,
            logging_enabled: true,
            status_message: None,
            session_store,
            last_session_persist: Instant::now(),
            close_in_progress: false,
            overflow_popup_open: false,
        };

        session_manager::restore_session_state(&mut app);

        app
    }

    pub(crate) fn active_tab(&self) -> Option<&WorkspaceTab> {
        self.tab_manager.active_tab()
    }

    pub(crate) fn active_tab_mut(&mut self) -> Option<&mut WorkspaceTab> {
        self.tab_manager.active_tab_mut()
    }

    pub(crate) fn log_event(&self, level: LogLevel, message: impl Into<String>) {
        if self.logging_enabled {
            logging::log(level, &message.into());
        }
    }

    pub(crate) fn describe_tab_at(&self, index: usize) -> String {
        self.tab_manager.describe_tab_at(index)
    }

    pub(crate) fn describe_active_tab(&self) -> String {
        self.tab_manager.describe_active_tab()
    }

    pub(crate) fn active_view_mut(&mut self) -> Option<&mut EditorViewState> {
        self.tab_manager
            .active_tab_mut()
            .and_then(WorkspaceTab::active_view_mut)
    }

    pub(crate) fn mark_session_dirty(&mut self) {
        self.tab_manager.mark_session_dirty();
    }

    pub(crate) fn session_dirty(&self) -> bool {
        self.tab_manager.session_dirty
    }

    pub(crate) fn clear_session_dirty(&mut self) {
        self.tab_manager.session_dirty = false;
    }

    pub(crate) fn persist_session_now(&mut self) -> std::io::Result<()> {
        session_manager::persist_session_now(self)
    }

    pub(crate) fn estimated_tab_strip_width(&self, spacing: f32) -> f32 {
        self.tab_manager.estimated_tab_strip_width(spacing)
    }

    pub(crate) fn request_exit(&mut self, ctx: &egui::Context) {
        if self.close_in_progress {
            return;
        }

        match self.persist_session_now() {
            Ok(()) => {
                self.close_in_progress = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(error) => {
                self.set_error_status(format!("Session save failed: {error}"));
            }
        }
    }

    pub fn new_tab(&mut self) {
        self.tab_manager.create_untitled_tab();
        let description = self.tab_manager.describe_active_tab();
        self.log_event(
            LogLevel::Info,
            format!(
                "Created new tab at index {}: {} (total tabs={})",
                self.tab_manager.active_tab_index,
                description,
                self.tab_manager.tabs.len()
            ),
        );
        let _ = self.persist_session_now();
    }

    pub fn open_file(&mut self) {
        FileController::open_file(self);
    }

    pub fn open_file_here(&mut self) {
        FileController::open_file_here(self);
    }

    pub fn save_file(&mut self) {
        FileController::save_file(self);
    }

    pub fn save_file_at(&mut self, index: usize) -> bool {
        FileController::save_file_at(self, index)
    }

    pub fn save_file_as(&mut self) {
        FileController::save_file_as(self);
    }

    pub fn save_file_as_at(&mut self, index: usize) -> bool {
        FileController::save_file_as_at(self, index)
    }

    pub(crate) fn perform_close_tab(&mut self, index: usize) {
        let tab_description = self.tab_manager.describe_tab_at(index);
        self.tab_manager.close_tab_internal(index);
        self.log_event(
            LogLevel::Info,
            format!(
                "Closed tab at index {index}: {tab_description} (remaining tabs={})",
                self.tab_manager.tabs.len()
            ),
        );
        let _ = self.persist_session_now();
    }

    pub fn perform_close_tab_no_persist(&mut self, index: usize) {
        let tab_description = self.tab_manager.describe_tab_at(index);
        self.tab_manager.close_tab_internal(index);
        self.log_event(
            LogLevel::Info,
            format!("Closed tab without immediate persist at index {index}: {tab_description}"),
        );
    }

    pub(crate) fn window_title(&self) -> String {
        if self.tab_manager.tabs.is_empty() {
            return "Scratchpad".to_owned();
        }

        let index = self
            .tab_manager
            .active_tab_index
            .min(self.tab_manager.tabs.len() - 1);
        let tab = &self.tab_manager.tabs[index];
        let marker = if tab.active_buffer().is_dirty {
            "*"
        } else {
            ""
        };
        format!("{}{} - Scratchpad", marker, tab.active_buffer().name)
    }

    pub(crate) fn split_active_view_with_placement(
        &mut self,
        axis: SplitAxis,
        new_view_first: bool,
        ratio: f32,
    ) {
        self.handle_command(AppCommand::SplitActiveView {
            axis,
            new_view_first,
            ratio,
        });
    }

    pub(crate) fn close_view(&mut self, view_id: ViewId) {
        self.handle_command(AppCommand::CloseView { view_id });
    }

    pub(crate) fn promote_view_to_tab(&mut self, view_id: ViewId) {
        self.handle_command(AppCommand::PromoteViewToTab { view_id });
    }

    pub(crate) fn activate_view(&mut self, view_id: ViewId) {
        self.handle_command(AppCommand::ActivateView { view_id });
    }

    pub(crate) fn resize_split(&mut self, path: SplitPath, ratio: f32) {
        self.handle_command(AppCommand::ResizeSplit { path, ratio });
    }

    pub(crate) fn append_tab(&mut self, tab: WorkspaceTab) {
        self.tab_manager.append_tab(tab);
    }

    pub fn create_untitled_tab(&mut self) {
        self.tab_manager.create_untitled_tab();
    }

    pub fn tabs(&self) -> &[WorkspaceTab] {
        &self.tab_manager.tabs
    }

    pub fn tabs_mut(&mut self) -> &mut [WorkspaceTab] {
        &mut self.tab_manager.tabs
    }

    pub fn active_tab_index(&self) -> usize {
        self.tab_manager.active_tab_index
    }

    pub(crate) fn find_tab_by_path(&self, candidate: &Path) -> Option<(usize, ViewId)> {
        self.tab_manager.find_tab_by_path(candidate)
    }

    pub fn reorder_tab(&mut self, from_index: usize, to_index: usize) {
        self.handle_command(AppCommand::ReorderTab {
            from_index,
            to_index,
        });
    }

    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn word_wrap(&self) -> bool {
        self.word_wrap
    }

    pub fn logging_enabled(&self) -> bool {
        self.logging_enabled
    }

    pub fn session_store(&self) -> &SessionStore {
        &self.session_store
    }

    pub fn tab_manager(&self) -> &TabManager {
        &self.tab_manager
    }

    pub fn tab_manager_mut(&mut self) -> &mut TabManager {
        &mut self.tab_manager
    }

    pub fn pending_action(&self) -> Option<PendingAction> {
        self.tab_manager.pending_action
    }

    pub fn set_pending_action(&mut self, action: Option<PendingAction>) -> Option<PendingAction> {
        let old = self.tab_manager.pending_action;
        self.tab_manager.pending_action = action;
        old
    }

    pub(crate) fn clear_status_message(&mut self) {
        self.status_message = None;
    }

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
        if self.logging_enabled {
            logging::log(level, &message);
        }
    }

    pub(crate) fn set_logging_enabled(&mut self, enabled: bool) {
        if self.logging_enabled == enabled {
            return;
        }

        self.logging_enabled = enabled;
        self.mark_session_dirty();

        let state = if enabled { "enabled" } else { "disabled" };
        if enabled {
            logging::log(LogLevel::Info, &format!("Runtime logging {state}"));
        } else {
            self.status_message = Some(format!("Runtime logging {state}."));
        }
    }
}
