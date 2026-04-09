use crate::app::chrome::handle_window_resize;
use crate::app::commands::AppCommand;
use crate::app::domain::{
    EditorViewState, PendingAction, SplitAxis, SplitPath, TabManager, ViewId, WorkspaceTab,
};
use crate::app::fonts::{self, EditorFontPreset};
use crate::app::logging::{self, LogLevel};
use crate::app::services::file_controller::FileController;
use crate::app::services::session_manager;
use crate::app::services::session_store::SessionStore;
use crate::app::services::settings_store::{AppSettings, SettingsStore};
use crate::app::shortcuts;
use crate::app::startup::{StartupOpenTarget, StartupOptions};
use crate::app::ui::{dialogs, editor_area, settings, status_bar, tab_strip};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(crate) const SESSION_SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AppSurface {
    Workspace,
    Settings,
}

pub struct ScratchpadApp {
    pub(crate) tab_manager: TabManager,
    pub(crate) font_size: f32,
    pub(crate) word_wrap: bool,
    pub(crate) logging_enabled: bool,
    pub(crate) editor_font: EditorFontPreset,
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
}

impl Default for ScratchpadApp {
    fn default() -> Self {
        Self::with_session_store_and_startup(SessionStore::default(), StartupOptions::default())
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
        self.sync_editor_fonts(&ctx);
        session_manager::maybe_persist_session(self, &ctx);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));

        tab_strip::show_header(ui, self);
        status_bar::show_status_bar(ui, self);
        match self.active_surface {
            AppSurface::Workspace => editor_area::show_editor(ui, self),
            AppSurface::Settings => settings::show_page(ui, self),
        }
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
        let settings_root = session_store.root().to_path_buf();
        Self::with_stores_and_startup(
            session_store,
            SettingsStore::new(settings_root),
            StartupOptions::default(),
        )
    }

    pub fn with_startup_options(startup_options: StartupOptions) -> Self {
        let session_store = SessionStore::default();
        let settings_root = session_store.root().to_path_buf();
        Self::with_stores_and_startup(
            session_store,
            SettingsStore::new(settings_root),
            startup_options,
        )
    }

    pub fn with_session_store_and_startup(
        session_store: SessionStore,
        startup_options: StartupOptions,
    ) -> Self {
        let settings_root = session_store.root().to_path_buf();
        Self::with_stores_and_startup(
            session_store,
            SettingsStore::new(settings_root),
            startup_options,
        )
    }

    pub fn with_stores_and_startup(
        session_store: SessionStore,
        settings_store: SettingsStore,
        startup_options: StartupOptions,
    ) -> Self {
        let mut app = Self {
            tab_manager: TabManager::default(),
            font_size: AppSettings::default().font_size,
            word_wrap: AppSettings::default().word_wrap,
            logging_enabled: AppSettings::default().logging_enabled,
            editor_font: AppSettings::default().editor_font,
            app_settings: AppSettings::default(),
            status_message: None,
            pending_editor_focus: None,
            settings_store,
            session_store,
            last_session_persist: Instant::now(),
            close_in_progress: false,
            overflow_popup_open: false,
            applied_editor_font: None,
            active_surface: AppSurface::Workspace,
            settings_tab_index: usize::MAX,
        };

        let loaded_from_yaml = app.load_settings_from_store();
        if startup_options.restore_session {
            let legacy_settings = session_manager::restore_session_state(&mut app);
            if !loaded_from_yaml && let Some(legacy_settings) = legacy_settings {
                app.apply_settings(legacy_settings);
                let _ = app.persist_settings_now();
            }
        }
        app.request_focus_for_active_view();
        app.apply_startup_options(startup_options);

        app
    }

    fn apply_startup_options(&mut self, startup_options: StartupOptions) {
        if startup_options.log_cli {
            logging::log(
                LogLevel::Info,
                &format!("Startup options resolved: {}", startup_options.describe()),
            );
        }

        if startup_options.files.is_empty() {
            if let Some(message) = startup_options.startup_notice {
                self.set_warning_status(message);
            }
            return;
        }

        match startup_options.open_target {
            StartupOpenTarget::SeparateTabs => {
                FileController::open_external_paths(self, startup_options.files)
            }
            StartupOpenTarget::ActiveTab => {
                FileController::open_external_paths_here(self, startup_options.files)
            }
            StartupOpenTarget::TabIndex(index) => {
                FileController::open_external_paths_into_tab(self, index, startup_options.files)
            }
        }

        if let Some(message) = startup_options.startup_notice {
            self.set_warning_status(message);
        }
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
        let tab_count = self.total_tab_slots();
        if tab_count == 0 {
            return 0.0;
        }

        (tab_count as f32 * crate::app::theme::TAB_BUTTON_WIDTH)
            + ((tab_count.saturating_sub(1)) as f32 * spacing)
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
        self.request_focus_for_active_view();
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
        self.request_focus_for_active_view();
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
        self.request_focus_for_active_view();
        self.log_event(
            LogLevel::Info,
            format!("Closed tab without immediate persist at index {index}: {tab_description}"),
        );
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

    pub fn append_tab(&mut self, tab: WorkspaceTab) {
        self.tab_manager.append_tab(tab);
        self.request_focus_for_active_view();
    }

    pub fn create_untitled_tab(&mut self) {
        self.tab_manager.create_untitled_tab();
        self.request_focus_for_active_view();
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

    pub(crate) fn total_tab_slots(&self) -> usize {
        self.tabs().len() + usize::from(self.showing_settings())
    }

    pub(crate) fn settings_slot_index(&self) -> Option<usize> {
        self.showing_settings()
            .then_some(self.settings_tab_index.min(self.tabs().len()))
    }

    pub(crate) fn tab_slot_is_settings(&self, slot_index: usize) -> bool {
        self.settings_slot_index() == Some(slot_index)
    }

    pub(crate) fn workspace_index_for_slot(&self, slot_index: usize) -> Option<usize> {
        if slot_index >= self.total_tab_slots() || self.tab_slot_is_settings(slot_index) {
            return None;
        }

        Some(match self.settings_slot_index() {
            Some(settings_index) if slot_index > settings_index => slot_index - 1,
            _ => slot_index,
        })
    }

    pub(crate) fn slot_for_workspace_index(&self, workspace_index: usize) -> usize {
        match self.settings_slot_index() {
            Some(settings_index) if workspace_index >= settings_index => workspace_index + 1,
            _ => workspace_index,
        }
    }

    pub(crate) fn active_tab_slot_index(&self) -> usize {
        if self.showing_settings() {
            self.settings_slot_index()
                .unwrap_or_else(|| self.tabs().len())
        } else {
            self.slot_for_workspace_index(self.active_tab_index())
        }
    }

    pub(crate) fn display_tab_name_at_slot(&self, slot_index: usize) -> Option<String> {
        if self.tab_slot_is_settings(slot_index) {
            return Some("Settings".to_owned());
        }

        let workspace_index = self.workspace_index_for_slot(slot_index)?;
        let tab = self.tabs().get(workspace_index)?;
        let duplicate_count = self
            .tabs()
            .iter()
            .filter(|candidate| candidate.buffer.name == tab.buffer.name)
            .count();
        Some(tab.full_display_name(duplicate_count > 1))
    }

    pub(crate) fn reorder_display_tab(&mut self, from_slot: usize, to_slot: usize) -> bool {
        let total_slots = self.total_tab_slots();
        if from_slot >= total_slots || to_slot >= total_slots || from_slot == to_slot {
            return false;
        }

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum DisplayTabSlot {
            Workspace(usize),
            Settings,
        }

        let mut display_slots = Vec::with_capacity(total_slots);
        for slot_index in 0..total_slots {
            if self.tab_slot_is_settings(slot_index) {
                display_slots.push(DisplayTabSlot::Settings);
            } else if let Some(workspace_index) = self.workspace_index_for_slot(slot_index) {
                display_slots.push(DisplayTabSlot::Workspace(workspace_index));
            }
        }

        let moved_slot = display_slots.remove(from_slot);
        display_slots.insert(to_slot, moved_slot);

        if let Some(settings_index) = display_slots
            .iter()
            .position(|slot| *slot == DisplayTabSlot::Settings)
        {
            self.settings_tab_index = settings_index;
        }

        let workspace_order = display_slots
            .into_iter()
            .filter_map(|slot| match slot {
                DisplayTabSlot::Workspace(index) => Some(index),
                DisplayTabSlot::Settings => None,
            })
            .collect::<Vec<_>>();

        self.apply_workspace_tab_order(workspace_order);
        true
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

    pub fn editor_font(&self) -> EditorFontPreset {
        self.editor_font
    }

    pub fn showing_settings(&self) -> bool {
        self.active_surface == AppSurface::Settings
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

    pub fn settings_path(&self) -> PathBuf {
        self.settings_store.path().to_path_buf()
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

    fn load_settings_from_store(&mut self) -> bool {
        match self.settings_store.load() {
            Ok(Some(settings)) => {
                self.apply_settings(settings);
                true
            }
            Ok(None) => {
                self.apply_settings(AppSettings::default());
                false
            }
            Err(error) => {
                self.apply_settings(AppSettings::default());
                self.set_warning_status(format!("Settings load failed; using defaults: {error}"));
                false
            }
        }
    }

    fn apply_settings(&mut self, settings: AppSettings) {
        self.font_size = settings.font_size;
        self.word_wrap = settings.word_wrap;
        self.logging_enabled = settings.logging_enabled;
        self.editor_font = settings.editor_font;
        self.app_settings = settings;
    }

    fn refresh_settings_snapshot(&mut self) {
        self.app_settings.font_size = self.font_size;
        self.app_settings.word_wrap = self.word_wrap;
        self.app_settings.logging_enabled = self.logging_enabled;
        self.app_settings.editor_font = self.editor_font;
    }

    pub(crate) fn persist_settings_now(&mut self) -> std::io::Result<()> {
        self.refresh_settings_snapshot();
        self.settings_store.save(&self.app_settings)
    }

    pub(crate) fn set_font_size(&mut self, font_size: f32) {
        let next = font_size.clamp(8.0, 72.0);
        if (self.font_size - next).abs() < f32::EPSILON {
            return;
        }

        self.font_size = next;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn set_editor_font(&mut self, editor_font: EditorFontPreset) {
        if self.editor_font == editor_font {
            return;
        }

        self.editor_font = editor_font;
        self.applied_editor_font = None;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn set_word_wrap(&mut self, enabled: bool) {
        if self.word_wrap == enabled {
            return;
        }

        self.word_wrap = enabled;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn request_focus_for_view(&mut self, view_id: ViewId) {
        self.pending_editor_focus = Some(view_id);
    }

    pub(crate) fn request_focus_for_active_view(&mut self) {
        if let Some(view_id) = self.active_tab().map(|tab| tab.active_view_id) {
            self.request_focus_for_view(view_id);
        }
    }

    pub(crate) fn should_focus_view(&self, view_id: ViewId) -> bool {
        self.pending_editor_focus == Some(view_id)
    }

    pub(crate) fn consume_focus_request(&mut self, view_id: ViewId) {
        if self.pending_editor_focus == Some(view_id) {
            self.pending_editor_focus = None;
        }
    }

    fn sync_editor_fonts(&mut self, ctx: &egui::Context) {
        if self.applied_editor_font == Some(self.editor_font) {
            return;
        }

        if let Err(error) = fonts::apply_editor_fonts(ctx, self.editor_font) {
            self.set_warning_status(format!(
                "Editor font '{}' unavailable; using default fallback: {error}",
                self.editor_font.label()
            ));
        }
        self.applied_editor_font = Some(self.editor_font);
    }

    pub(crate) fn open_settings(&mut self) {
        self.settings_tab_index = self.settings_tab_index.min(self.tabs().len());
        self.active_surface = AppSurface::Settings;
        self.tab_manager.pending_scroll_to_active = true;
    }

    pub(crate) fn close_settings(&mut self) {
        self.active_surface = AppSurface::Workspace;
        self.settings_tab_index = self.settings_tab_index.min(self.tabs().len());
        self.tab_manager.pending_scroll_to_active = true;
        self.request_focus_for_active_view();
    }

    pub(crate) fn reset_settings_to_defaults(&mut self) {
        self.apply_settings(AppSettings::default());
        self.applied_editor_font = None;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
            return;
        }

        self.set_info_status("Settings reset to defaults.");
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
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
            return;
        }

        let state = if enabled { "enabled" } else { "disabled" };
        if enabled {
            logging::log(LogLevel::Info, &format!("Runtime logging {state}"));
        } else {
            self.status_message = Some(format!("Runtime logging {state}."));
        }
    }

    fn apply_workspace_tab_order(&mut self, workspace_order: Vec<usize>) {
        if workspace_order.len() != self.tab_manager.tabs.len() {
            return;
        }

        let active_workspace_index = self.tab_manager.active_tab_index;
        let mut tabs = std::mem::take(&mut self.tab_manager.tabs)
            .into_iter()
            .map(Some)
            .collect::<Vec<_>>();
        self.tab_manager.tabs = workspace_order
            .iter()
            .filter_map(|&index| tabs.get_mut(index).and_then(Option::take))
            .collect();
        self.tab_manager.active_tab_index = workspace_order
            .iter()
            .position(|&index| index == active_workspace_index)
            .unwrap_or(0);
        self.tab_manager.pending_scroll_to_active = true;
        self.mark_session_dirty();
    }
}
