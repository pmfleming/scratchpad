use crate::app::chrome::handle_window_resize;
use crate::app::commands::AppCommand;
use crate::app::domain::{
    BufferId, EditorViewState, PendingAction, SplitAxis, SplitPath, TabManager, ViewId,
    WorkspaceTab,
};
use crate::app::fonts::{self, EditorFontPreset};
use crate::app::logging::{self, LogLevel};
use crate::app::services::file_controller::FileController;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::{AppSettings, SettingsStore};
use crate::app::shortcuts;
use crate::app::startup::StartupOptions;
use crate::app::ui::{dialogs, editor_area, settings, status_bar, tab_strip};
use eframe::egui;
use std::path::Path;
use std::time::{Duration, Instant};

mod settings_state;
mod startup_state;

pub(crate) const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

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
    pub(crate) settings_store: SettingsStore,
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
}

impl Default for ScratchpadApp {
    fn default() -> Self {
        Self::with_session_store_and_startup(SessionStore::default(), StartupOptions::default())
    }
}

impl eframe::App for ScratchpadApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if ctx.input(|input| input.viewport().close_requested()) && !self.close_in_progress {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_exit(&ctx);
            return;
        }

        handle_window_resize(&ctx);
        self.apply_theme_to_context(&ctx);
        self.sync_editor_fonts(&ctx);
        session_manager::maybe_persist_session(self, &ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        if self.tab_list_position() == crate::app::services::settings_store::TabListPosition::Top {
            tab_strip::show_header(ui, self);
        }
        status_bar::show_status_bar(ui, self);
        tab_strip::show_bottom_tab_list(ui, self);
        tab_strip::show_vertical_tab_list(ui, self);
        match self.active_surface {
            AppSurface::Workspace => editor_area::show_editor(ui, self),
            AppSurface::Settings => settings::show_page(ui, self),
        }
        dialogs::show_pending_action_modal(&ctx, self);
        shortcuts::handle_shortcuts(self, &ctx);
    }
}

impl Drop for ScratchpadApp {
    fn drop(&mut self) {
        let _ = self.persist_session_now();
    }
}

impl ScratchpadApp {
    pub(crate) fn active_tab(&self) -> Option<&WorkspaceTab> { self.tab_manager.active_tab() }

    pub(crate) fn active_tab_mut(&mut self) -> Option<&mut WorkspaceTab> { self.tab_manager.active_tab_mut() }

    pub(crate) fn log_event(&self, level: LogLevel, message: impl Into<String>) {
        if self.app_settings.logging_enabled {
            logging::log(level, &message.into());
        }
    }

    pub(crate) fn describe_tab_at(&self, index: usize) -> String { self.tab_manager.describe_tab_at(index) }

    pub(crate) fn describe_active_tab(&self) -> String { self.tab_manager.describe_active_tab() }

    pub(crate) fn active_view_mut(&mut self) -> Option<&mut EditorViewState> {
        self.active_tab_mut().and_then(WorkspaceTab::active_view_mut)
    }

    pub(crate) fn mark_session_dirty(&mut self) { self.tab_manager.mark_session_dirty(); }

    pub(crate) fn session_dirty(&self) -> bool { self.tab_manager.session_dirty }

    pub(crate) fn clear_session_dirty(&mut self) { self.tab_manager.session_dirty = false; }

    pub(crate) fn persist_session_now(&mut self) -> std::io::Result<()> { session_manager::persist_session_now(self) }

    pub(crate) fn estimated_tab_strip_width(&self, spacing: f32) -> f32 {
        let tab_count = self.total_tab_slots();
        (tab_count > 0)
            .then_some(
                (tab_count as f32 * crate::app::theme::TAB_BUTTON_WIDTH)
                    + ((tab_count.saturating_sub(1)) as f32 * spacing),
            )
            .unwrap_or(0.0)
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
        self.create_workspace_tab(WorkspaceTab::untitled());
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
        match self.file_open_disposition() {
            crate::app::services::settings_store::FileOpenDisposition::NewTab => {
                FileController::open_file(self)
            }
            crate::app::services::settings_store::FileOpenDisposition::CurrentTab => {
                FileController::open_file_here(self)
            }
        }
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
        let tab_description = self.close_tab_internal(index);
        self.log_event(LogLevel::Info, format!(
            "Closed tab at index {index}: {tab_description} (remaining tabs={})",
            self.tab_manager.tabs.len()
        ));
        let _ = self.persist_session_now();
    }

    pub fn perform_close_tab_no_persist(&mut self, index: usize) {
        let tab_description = self.close_tab_internal(index);
        self.log_event(LogLevel::Info, format!(
            "Closed tab without immediate persist at index {index}: {tab_description}"
        ));
    }

    pub(crate) fn window_title(&self) -> String {
        if self.showing_settings() {
            return "Settings - Scratchpad".to_owned();
        }

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

    pub(crate) fn close_view(&mut self, view_id: ViewId) { self.handle_command(AppCommand::CloseView { view_id }); }

    pub(crate) fn promote_view_to_tab(&mut self, view_id: ViewId) { self.handle_command(AppCommand::PromoteViewToTab { view_id }); }

    pub(crate) fn activate_view(&mut self, view_id: ViewId) { self.handle_command(AppCommand::ActivateView { view_id }); }

    pub(crate) fn resize_split(&mut self, path: SplitPath, ratio: f32) { self.handle_command(AppCommand::ResizeSplit { path, ratio }); }

    pub fn append_tab(&mut self, tab: WorkspaceTab) {
        self.create_workspace_tab(tab);
    }

    pub fn create_untitled_tab(&mut self) {
        self.create_workspace_tab(WorkspaceTab::untitled());
    }

    pub fn tabs(&self) -> &[WorkspaceTab] { &self.tab_manager.tabs }

    pub fn tabs_mut(&mut self) -> &mut [WorkspaceTab] { &mut self.tab_manager.tabs }

    pub fn active_tab_index(&self) -> usize { self.tab_manager.active_tab_index }

    pub(crate) fn find_tab_by_path(&self, candidate: &Path) -> Option<(usize, ViewId)> { self.tab_manager.find_tab_by_path(candidate) }

    pub fn reorder_tab(&mut self, from_index: usize, to_index: usize) {
        self.handle_command(AppCommand::ReorderTab {
            from_index,
            to_index,
        });
    }

    pub fn session_store(&self) -> &SessionStore { &self.session_store }

    pub fn tab_manager(&self) -> &TabManager { &self.tab_manager }

    pub fn tab_manager_mut(&mut self) -> &mut TabManager { &mut self.tab_manager }

    pub fn pending_action(&self) -> Option<PendingAction> { self.tab_manager.pending_action }

    pub fn set_pending_action(&mut self, action: Option<PendingAction>) -> Option<PendingAction> {
        let old = self.tab_manager.pending_action;
        self.tab_manager.pending_action = action;
        old
    }

    pub(crate) fn clear_status_message(&mut self) { self.status_message = None; }

    pub(crate) fn request_focus_for_view(&mut self, view_id: ViewId) { self.pending_editor_focus = Some(view_id); }

    pub(crate) fn request_focus_for_active_view(&mut self) {
        if let Some(view_id) = self.active_tab().map(|tab| tab.active_view_id) {
            self.request_focus_for_view(view_id);
        }
    }

    pub(crate) fn should_focus_view(&self, view_id: ViewId) -> bool { self.pending_editor_focus == Some(view_id) }

    pub(crate) fn consume_focus_request(&mut self, view_id: ViewId) {
        if self.pending_editor_focus == Some(view_id) {
            self.pending_editor_focus = None;
        }
    }

    fn create_workspace_tab(&mut self, tab: WorkspaceTab) {
        self.reload_settings_before_workspace_change();
        self.tab_manager.append_tab(tab);
        self.request_focus_for_active_view();
    }

    fn close_tab_internal(&mut self, index: usize) -> String {
        let tab_description = self.tab_manager.describe_tab_at(index);
        let settings_refresh = self.settings_toml_refresh_on_tab_close(index);
        self.tab_manager.close_tab_internal(index);
        self.request_focus_for_active_view();
        self.apply_settings_toml_refresh(settings_refresh);
        tab_description
    }

    fn sync_editor_fonts(&mut self, ctx: &egui::Context) {
        if self.applied_editor_font == Some(self.app_settings.editor_font) {
            return;
        }

        if let Err(error) = fonts::apply_editor_fonts(ctx, self.app_settings.editor_font) {
            self.set_warning_status(format!(
                "Editor font '{}' unavailable; using default fallback: {error}",
                self.app_settings.editor_font.label()
            ));
        }
        self.applied_editor_font = Some(self.app_settings.editor_font);
    }

    pub(crate) fn set_info_status(&mut self, message: impl Into<String>) { self.set_status(LogLevel::Info, message); }

    pub(crate) fn set_warning_status(&mut self, message: impl Into<String>) { self.set_status(LogLevel::Warn, message); }

    pub(crate) fn set_error_status(&mut self, message: impl Into<String>) { self.set_status(LogLevel::Error, message); }

    fn set_status(&mut self, level: LogLevel, message: impl Into<String>) {
        let message = message.into();
        self.status_message = Some(message.clone());
        if self.app_settings.logging_enabled {
            logging::log(level, &message);
        }
    }
}
