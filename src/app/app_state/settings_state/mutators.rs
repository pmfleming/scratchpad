use super::{
    AppSettings, AppSurface, AppThemeMode, FileController, FileOpenDisposition, NewTabPlacement,
    ScratchpadApp, StartupSessionBehavior, TabListPosition, color_to_hex,
    sanitize_tab_list_auto_hide_delay_seconds, stock_editor_palette_for_selection,
};
use crate::app::domain::TextHistoryBudget;
use crate::app::fonts::EditorFontPreset;
use eframe::egui;
use std::time::Instant;

impl AppSettings {
    fn set_font_size(&mut self, font_size: f32) -> bool {
        let next = font_size.clamp(8.0, 72.0);
        if (self.font_size - next).abs() < f32::EPSILON {
            return false;
        }
        self.font_size = next;
        true
    }

    fn set_editor_font(&mut self, editor_font: EditorFontPreset) -> bool {
        replace_if_changed(&mut self.editor_font, editor_font)
    }

    fn set_word_wrap(&mut self, enabled: bool) -> bool {
        replace_if_changed(&mut self.word_wrap, enabled)
    }

    fn set_editor_gutter(&mut self, gutter: u8) -> bool {
        replace_if_changed(&mut self.editor_gutter, gutter.min(32))
    }

    fn apply_theme_mode_preset(
        &mut self,
        theme_mode: AppThemeMode,
        system_theme: Option<egui::Theme>,
    ) -> bool {
        let (text_color, background_color) =
            stock_editor_palette_for_selection(theme_mode, system_theme);
        if self.theme_mode == theme_mode
            && self.editor_text_color == text_color
            && self.editor_background_color == background_color
        {
            return false;
        }

        self.theme_mode = theme_mode;
        self.editor_text_color = text_color.to_owned();
        self.editor_background_color = background_color.to_owned();
        true
    }

    fn set_editor_text_color(&mut self, color: egui::Color32) -> bool {
        self.set_editor_palette_color(color_to_hex(color), true)
    }

    fn set_editor_background_color(&mut self, color: egui::Color32) -> bool {
        self.set_editor_palette_color(color_to_hex(color), false)
    }

    fn set_editor_text_highlight_color(&mut self, color: egui::Color32) -> bool {
        let next = color_to_hex(color);
        let next_text = color_to_hex(crate::app::color_contrast::optimal_text_color(color));
        if self.editor_text_highlight_color == next
            && self.editor_text_highlight_text_color == next_text
        {
            return false;
        }

        self.editor_text_highlight_color = next;
        self.editor_text_highlight_text_color = next_text;
        true
    }

    fn set_editor_palette_color(&mut self, next: String, is_text_color: bool) -> bool {
        let current = if is_text_color {
            &mut self.editor_text_color
        } else {
            &mut self.editor_background_color
        };
        replace_if_changed(current, next)
    }

    fn set_file_open_disposition(&mut self, disposition: FileOpenDisposition) -> bool {
        replace_if_changed(&mut self.file_open_disposition, disposition)
    }

    fn set_new_tab_placement(&mut self, placement: NewTabPlacement) -> bool {
        replace_if_changed(&mut self.new_tab_placement, placement)
    }

    fn set_startup_session_behavior(&mut self, behavior: StartupSessionBehavior) -> bool {
        replace_if_changed(&mut self.startup_session_behavior, behavior)
    }

    fn set_history_budget(&mut self, mut budget: TextHistoryBudget) -> bool {
        budget = budget.sanitized();
        if self.history_budget == budget {
            return false;
        }
        budget.derived_from_memory = false;
        self.history_budget = budget;
        true
    }

    fn reset_history_budget_to_auto(&mut self) {
        self.history_budget = TextHistoryBudget::derive_from_available_memory();
    }
}

fn replace_if_changed<T: PartialEq>(current: &mut T, next: T) -> bool {
    if *current == next {
        return false;
    }
    *current = next;
    true
}

impl ScratchpadApp {
    pub(super) fn persist_settings_or_error(&mut self) {
        if let Err(error) = self.persist_settings_now() {
            self.state.status.report_settings_save_failed(error);
        }
    }

    fn reset_tab_list_visibility_state(&mut self, keep_open: bool) {
        self.state.vertical_tab_list_open = keep_open;
        self.state.vertical_tab_list_hide_deadline = None;
    }

    fn clear_tab_list_hide_deadline(&mut self) {
        self.state.vertical_tab_list_hide_deadline = None;
    }

    fn set_tab_list_width(&mut self, width: f32) {
        self.state.app_settings.tab_list_width = width;
        self.persist_settings_or_error();
    }

    fn set_settings_surface(&mut self, surface: AppSurface, open: bool) -> bool {
        let changed = self.settings_tab_open() != open;
        self.state.settings_tab_index = self
            .state
            .settings_tab_index
            .min(self.tab_manager.tabs.as_slice().len());
        self.state.app_settings.settings_tab_open = open;
        self.state.active_surface = surface;
        self.ensure_active_tab_slot_selected();
        self.tab_manager.pending_scroll_to_active = true;
        changed
    }
}

pub(crate) fn set_font_size(app: &mut ScratchpadApp, font_size: f32) {
    if app.state.app_settings.set_font_size(font_size) {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_editor_font(app: &mut ScratchpadApp, editor_font: EditorFontPreset) {
    if app.state.app_settings.set_editor_font(editor_font) {
        app.state.applied_editor_font = None;
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_word_wrap(app: &mut ScratchpadApp, enabled: bool) {
    if app.state.app_settings.set_word_wrap(enabled) {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_editor_gutter(app: &mut ScratchpadApp, gutter: u8) {
    if app.state.app_settings.set_editor_gutter(gutter) {
        app.persist_settings_or_error();
    }
}

pub(crate) fn apply_theme_mode_preset(
    app: &mut ScratchpadApp,
    theme_mode: AppThemeMode,
    system_theme: Option<egui::Theme>,
) {
    if app
        .state
        .app_settings
        .apply_theme_mode_preset(theme_mode, system_theme)
    {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_editor_text_color(app: &mut ScratchpadApp, color: egui::Color32) {
    if app.state.app_settings.set_editor_text_color(color) {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_editor_background_color(app: &mut ScratchpadApp, color: egui::Color32) {
    if app.state.app_settings.set_editor_background_color(color) {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_editor_text_highlight_color(app: &mut ScratchpadApp, color: egui::Color32) {
    if app
        .state
        .app_settings
        .set_editor_text_highlight_color(color)
    {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_tab_list_position(app: &mut ScratchpadApp, position: TabListPosition) {
    if app.state.app_settings.tab_list_position == position {
        return;
    }

    app.state.app_settings.tab_list_position = position;
    app.begin_layout_transition();
    app.reset_tab_list_visibility_state(false);
    if position.is_vertical() {
        app.state.overflow_popup_open = false;
    }
    app.tab_manager.pending_scroll_to_active = true;
    app.persist_settings_or_error();
}

pub(crate) fn set_file_open_disposition(app: &mut ScratchpadApp, disposition: FileOpenDisposition) {
    if app
        .state
        .app_settings
        .set_file_open_disposition(disposition)
    {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_new_tab_placement(app: &mut ScratchpadApp, placement: NewTabPlacement) {
    if app.state.app_settings.set_new_tab_placement(placement) {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_startup_session_behavior(
    app: &mut ScratchpadApp,
    behavior: StartupSessionBehavior,
) {
    if app
        .state
        .app_settings
        .set_startup_session_behavior(behavior)
    {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_auto_hide_tab_list(app: &mut ScratchpadApp, enabled: bool) {
    if app.state.app_settings.auto_hide_tab_list == enabled {
        return;
    }

    app.state.app_settings.auto_hide_tab_list = enabled;
    app.begin_layout_transition();
    app.reset_tab_list_visibility_state(enabled && app.state.vertical_tab_list_open);
    app.persist_settings_or_error();
}

pub(crate) fn set_tab_list_auto_hide_delay_seconds(app: &mut ScratchpadApp, seconds: f32) {
    let next = sanitize_tab_list_auto_hide_delay_seconds(seconds);
    if (app.state.app_settings.tab_list_auto_hide_delay_seconds - next).abs() < f32::EPSILON {
        return;
    }

    app.state.app_settings.tab_list_auto_hide_delay_seconds = next;
    app.clear_tab_list_hide_deadline();
    app.persist_settings_or_error();
}

pub(crate) fn set_recent_files_enabled(app: &mut ScratchpadApp, enabled: bool) {
    if replace_if_changed(&mut app.state.app_settings.recent_files_enabled, enabled) {
        app.persist_settings_or_error();
    }
}

pub(crate) fn set_status_bar_visible(app: &mut ScratchpadApp, visible: bool) {
    if app.state.app_settings.status_bar_visible == visible {
        app.state.pending_status_bar_visible = None;
        return;
    }

    app.state.app_settings.status_bar_visible = visible;
    app.begin_layout_transition();
    app.persist_settings_or_error();
}

pub(crate) fn defer_status_bar_visible(
    app: &mut ScratchpadApp,
    visible: bool,
    ctx: &egui::Context,
) {
    app.state.pending_status_bar_visible =
        (app.state.app_settings.status_bar_visible != visible).then_some(visible);
    if app.state.pending_status_bar_visible.is_some() {
        ctx.request_repaint();
    }
}

pub(crate) fn set_history_budget(app: &mut ScratchpadApp, budget: TextHistoryBudget) {
    if app.state.app_settings.set_history_budget(budget) {
        app.apply_history_budget_to_open_buffers();
        app.persist_settings_or_error();
    }
}

pub(crate) fn reset_history_budget_to_auto(app: &mut ScratchpadApp) {
    app.state.app_settings.reset_history_budget_to_auto();
    app.apply_history_budget_to_open_buffers();
    app.persist_settings_or_error();
}

pub(crate) fn set_tab_list_width_from_layout(app: &mut ScratchpadApp, width: f32) {
    let next = width.clamp(
        ScratchpadApp::VERTICAL_TAB_LIST_MIN_WIDTH,
        ScratchpadApp::VERTICAL_TAB_LIST_MAX_WIDTH,
    );
    if (app.state.app_settings.tab_list_width - next).abs() < 1.0 {
        return;
    }

    app.begin_layout_transition();
    app.set_tab_list_width(next);
}

impl ScratchpadApp {
    pub(crate) fn open_settings(&mut self) {
        self.open_settings_with_tab_selection(false);
    }

    pub(crate) fn open_settings_preserving_tab_selection(&mut self) {
        self.open_settings_with_tab_selection(true);
    }

    fn open_settings_with_tab_selection(&mut self, preserve_tab_selection: bool) {
        self.reload_settings_before_workspace_change();
        self.begin_layout_transition();
        if !self.settings_tab_open() {
            self.state.settings_preview_quote_index = (self.state.settings_preview_quote_index + 1)
                % crate::app::ui::settings::PREVIEW_QUOTES.len();
        }
        if self.set_settings_surface(AppSurface::Settings, true) {
            self.persist_settings_or_error();
        }
        if !preserve_tab_selection {
            self.select_only_tab_slot(self.active_tab_slot_index());
        }
    }

    pub(crate) fn open_settings_file_tab(&mut self) {
        let path = self.settings_path().to_path_buf();
        self.activate_workspace_surface();
        FileController::open_paths_async(self, vec![path]);
    }

    pub(crate) fn close_settings(&mut self) {
        self.begin_layout_transition();
        if self.set_settings_surface(AppSurface::Workspace, false) {
            self.persist_settings_or_error();
        }
        self.select_only_tab_slot(self.active_tab_slot_index());
        self.request_focus_for_active_view();
    }

    pub(crate) fn reset_settings_to_defaults(&mut self) {
        self.initialize_default_workspace_tabs();
        self.apply_settings(AppSettings::default());
        self.state.applied_editor_font = None;
        self.select_only_tab_slot(self.active_tab_slot_index());
        let _ = self.persist_session_now();
        match self.persist_settings_now() {
            Ok(()) => self.state.status.set_info_status_in_domain(
                crate::app::app_state::StatusDomain::Settings,
                "Settings reset to defaults.",
            ),
            Err(error) => self.state.status.report_settings_save_failed(error),
        }
    }

    pub(crate) fn activate_workspace_surface(&mut self) {
        self.state.active_surface = AppSurface::Workspace;
    }

    pub(crate) fn keep_tab_list_open(&mut self) {
        self.reset_tab_list_visibility_state(true);
    }

    pub(crate) fn delay_tab_list_hide(&mut self, now: Instant) {
        self.state.vertical_tab_list_open = true;
        self.state.vertical_tab_list_hide_deadline =
            Some(now + self.state.app_settings.tab_list_auto_hide_delay());
    }

    pub(crate) fn close_tab_list(&mut self) {
        self.reset_tab_list_visibility_state(false);
    }
}

#[cfg(test)]
mod tests {
    use super::ScratchpadApp;
    use crate::app::domain::{BufferState, TabManager, WorkspaceTab};
    use crate::app::services::session_store::SessionStore;
    use crate::app::services::settings_store::SettingsStore;
    use crate::app::startup::StartupOptions;

    #[test]
    fn command_open_settings_selects_only_settings_slot() {
        let mut app = test_app(["one.txt", "two.txt"]);
        app.select_only_tab_slot(0);
        app.toggle_tab_slot_selection(1);

        app.open_settings();

        assert!(app.showing_settings());
        assert_eq!(selected_slots(&app), vec![app.active_tab_slot_index()]);
        assert!(app.tab_slot_is_settings(app.active_tab_slot_index()));
    }

    #[test]
    fn tab_strip_open_settings_can_preserve_existing_selection() {
        let mut app = test_app(["one.txt", "two.txt"]);
        app.select_only_tab_slot(0);
        app.toggle_tab_slot_selection(1);

        app.open_settings_preserving_tab_selection();

        assert!(app.showing_settings());
        assert_eq!(
            selected_slots(&app),
            vec![0, 1, app.active_tab_slot_index()]
        );
    }

    #[test]
    fn close_settings_selects_only_active_workspace_slot() {
        let mut app = test_app(["one.txt", "two.txt"]);
        app.open_settings_preserving_tab_selection();
        app.toggle_tab_slot_selection(0);

        app.close_settings();

        assert!(!app.showing_settings());
        assert_eq!(selected_slots(&app), vec![app.active_tab_slot_index()]);
        assert!(!app.tab_slot_is_settings(app.active_tab_slot_index()));
    }

    #[test]
    fn reset_settings_to_defaults_restores_startup_default_state() {
        let mut app = test_app(["custom.txt"]);
        app.close_settings();

        app.reset_settings_to_defaults();

        assert!(app.showing_settings());
        assert_eq!(
            app.state.app_settings.tab_list_position(),
            crate::app::services::settings_store::TabListPosition::Top
        );
        assert_eq!(
            app.state.app_settings.theme_mode(),
            crate::app::services::settings_store::AppThemeMode::System
        );
        assert_eq!(app.tab_manager.tabs.as_slice().len(), 1);
        assert_eq!(
            app.tab_manager.tabs.as_slice()[0].buffer.name,
            crate::app::services::manual_files::USER_MANUAL_FILE_NAME
        );
        assert!(app.tab_slot_is_settings(app.active_tab_slot_index()));
    }

    fn test_app<const N: usize>(names: [&str; N]) -> ScratchpadApp {
        let temp_dir = tempfile::tempdir().expect("create temp app root");
        let root = temp_dir.keep();
        let mut app = ScratchpadApp::with_stores_and_startup(
            SessionStore::new(root.clone()),
            SettingsStore::new(root),
            StartupOptions::default(),
        );
        app.set_session_persist_on_drop(false);
        app.tab_manager = TabManager {
            tabs: names.into_iter().map(test_tab).collect(),
            active_tab_index: 0,
            pending_action: None,
            session_dirty: false,
            pending_scroll_to_active: false,
            buffer_tab_index: Default::default(),
            cold_session_tabs: Default::default(),
        };
        app.tab_manager.rebuild_buffer_tab_index();
        app.clear_tab_selection();
        app
    }

    fn test_tab(name: &str) -> WorkspaceTab {
        WorkspaceTab::new(BufferState::new(name.to_owned(), String::new(), None))
    }

    fn selected_slots(app: &ScratchpadApp) -> Vec<usize> {
        app.state.workspace_selection.selected_slots().collect()
    }
}
