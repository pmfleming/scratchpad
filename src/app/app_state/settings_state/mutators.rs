use super::{
    AppSettings, AppSurface, AppThemeMode, FileController, FileOpenDisposition, NewTabPlacement,
    ScratchpadApp, StartupSessionBehavior, TabListPosition, color_to_hex,
    sanitize_tab_list_auto_hide_delay_seconds, stock_editor_palette_for_selection,
};
use crate::app::domain::TextHistoryBudget;
use crate::app::fonts::EditorFontPreset;
use eframe::egui;
use std::time::Instant;

#[cfg(test)]
mod tests;

impl AppSettings {
    fn set_font_size(&mut self, font_size: f32) -> bool {
        let next = font_size.clamp(8.0, 72.0);
        if (self.editor.font_size - next).abs() < f32::EPSILON {
            return false;
        }
        self.editor.font_size = next;
        true
    }

    fn set_editor_font(&mut self, editor_font: EditorFontPreset) -> bool {
        replace_if_changed(&mut self.editor.editor_font, editor_font)
    }

    fn set_word_wrap(&mut self, enabled: bool) -> bool {
        replace_if_changed(&mut self.editor.word_wrap, enabled)
    }

    fn set_editor_gutter(&mut self, gutter: u8) -> bool {
        replace_if_changed(&mut self.editor.editor_gutter, gutter.min(32))
    }

    fn apply_theme_mode_preset(
        &mut self,
        theme_mode: AppThemeMode,
        system_theme: Option<egui::Theme>,
    ) -> bool {
        let (text_color, background_color) =
            stock_editor_palette_for_selection(theme_mode, system_theme);
        if self.editor.theme_mode == theme_mode
            && self.editor.editor_text_color == text_color
            && self.editor.editor_background_color == background_color
        {
            return false;
        }

        self.editor.theme_mode = theme_mode;
        self.editor.editor_text_color = text_color.to_owned();
        self.editor.editor_background_color = background_color.to_owned();
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
        if self.editor.editor_text_highlight_color == next
            && self.editor.editor_text_highlight_text_color == next_text
        {
            return false;
        }

        self.editor.editor_text_highlight_color = next;
        self.editor.editor_text_highlight_text_color = next_text;
        true
    }

    fn set_editor_palette_color(&mut self, next: String, is_text_color: bool) -> bool {
        let current = if is_text_color {
            &mut self.editor.editor_text_color
        } else {
            &mut self.editor.editor_background_color
        };
        replace_if_changed(current, next)
    }

    fn set_file_open_disposition(&mut self, disposition: FileOpenDisposition) -> bool {
        replace_if_changed(&mut self.workspace.file_open_disposition, disposition)
    }

    fn set_new_tab_placement(&mut self, placement: NewTabPlacement) -> bool {
        replace_if_changed(&mut self.workspace.new_tab_placement, placement)
    }

    fn set_startup_session_behavior(&mut self, behavior: StartupSessionBehavior) -> bool {
        replace_if_changed(&mut self.workspace.startup_session_behavior, behavior)
    }

    fn set_history_budget(&mut self, mut budget: TextHistoryBudget) -> bool {
        budget = budget.sanitized();
        if self.history.budget == budget {
            return false;
        }
        budget.derived_from_memory = false;
        self.history.budget = budget;
        true
    }

    fn reset_history_budget_to_auto(&mut self) {
        self.history.budget = TextHistoryBudget::derive_from_available_memory();
    }
}

fn replace_if_changed<T: PartialEq>(current: &mut T, next: T) -> bool {
    if *current == next {
        return false;
    }
    *current = next;
    true
}

pub(super) fn persist_settings_or_error(app: &mut ScratchpadApp) {
    if let Err(error) = app.persist_settings_now() {
        app.state.status.report_settings_save_failed(error);
    }
}

fn reset_tab_list_visibility_state(app: &mut ScratchpadApp, keep_open: bool) {
    app.state.vertical_tab_list_open = keep_open;
    app.state.vertical_tab_list_hide_deadline = None;
}

fn clear_tab_list_hide_deadline(app: &mut ScratchpadApp) {
    app.state.vertical_tab_list_hide_deadline = None;
}

fn set_tab_list_width(app: &mut ScratchpadApp, width: f32) {
    app.state.app_settings.workspace.tab_list_width = width;
    persist_settings_or_error(app);
}

fn set_settings_surface(app: &mut ScratchpadApp, surface: AppSurface, open: bool) -> bool {
    let changed = app.settings_tab_open() != open;
    let surface_changed = app.state.active_surface != surface;
    app.state.settings_tab_index = app
        .state
        .settings_tab_index
        .min(app.tab_manager.tabs.as_slice().len());
    app.state.app_settings.ui.settings_tab_open = open;
    app.state.active_surface = surface;
    if surface_changed {
        app.tab_manager.mark_session_dirty();
    }
    app.ensure_active_tab_slot_selected();
    app.tab_manager.pending_scroll_to_active = true;
    changed
}

pub(crate) fn set_font_size(app: &mut ScratchpadApp, font_size: f32) {
    if app.state.app_settings.set_font_size(font_size) {
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_editor_font(app: &mut ScratchpadApp, editor_font: EditorFontPreset) {
    if app.state.app_settings.set_editor_font(editor_font) {
        app.state.applied_editor_font = None;
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_word_wrap(app: &mut ScratchpadApp, enabled: bool) {
    if app.state.app_settings.set_word_wrap(enabled) {
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_editor_gutter(app: &mut ScratchpadApp, gutter: u8) {
    if app.state.app_settings.set_editor_gutter(gutter) {
        persist_settings_or_error(app);
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
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_editor_text_color(app: &mut ScratchpadApp, color: egui::Color32) {
    if app.state.app_settings.set_editor_text_color(color) {
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_editor_background_color(app: &mut ScratchpadApp, color: egui::Color32) {
    if app.state.app_settings.set_editor_background_color(color) {
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_editor_text_highlight_color(app: &mut ScratchpadApp, color: egui::Color32) {
    if app
        .state
        .app_settings
        .set_editor_text_highlight_color(color)
    {
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_tab_list_position(app: &mut ScratchpadApp, position: TabListPosition) {
    if app.state.app_settings.workspace.tab_list_position == position {
        return;
    }

    app.state.app_settings.workspace.tab_list_position = position;
    app.begin_layout_transition();
    reset_tab_list_visibility_state(app, false);
    if position.is_vertical() {
        app.state.overflow_popup_open = false;
    }
    app.tab_manager.pending_scroll_to_active = true;
    persist_settings_or_error(app);
}

pub(crate) fn set_file_open_disposition(app: &mut ScratchpadApp, disposition: FileOpenDisposition) {
    if app
        .state
        .app_settings
        .set_file_open_disposition(disposition)
    {
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_new_tab_placement(app: &mut ScratchpadApp, placement: NewTabPlacement) {
    if app.state.app_settings.set_new_tab_placement(placement) {
        persist_settings_or_error(app);
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
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_auto_hide_tab_list(app: &mut ScratchpadApp, enabled: bool) {
    if app.state.app_settings.workspace.auto_hide_tab_list == enabled {
        return;
    }

    app.state.app_settings.workspace.auto_hide_tab_list = enabled;
    app.begin_layout_transition();
    reset_tab_list_visibility_state(app, enabled && app.state.vertical_tab_list_open);
    persist_settings_or_error(app);
}

pub(crate) fn set_tab_list_auto_hide_delay_seconds(app: &mut ScratchpadApp, seconds: f32) {
    let next = sanitize_tab_list_auto_hide_delay_seconds(seconds);
    if (app
        .state
        .app_settings
        .workspace
        .tab_list_auto_hide_delay_seconds
        - next)
        .abs()
        < f32::EPSILON
    {
        return;
    }

    app.state
        .app_settings
        .workspace
        .tab_list_auto_hide_delay_seconds = next;
    clear_tab_list_hide_deadline(app);
    persist_settings_or_error(app);
}

pub(crate) fn set_recent_files_enabled(app: &mut ScratchpadApp, enabled: bool) {
    if replace_if_changed(
        &mut app.state.app_settings.workspace.recent_files_enabled,
        enabled,
    ) {
        persist_settings_or_error(app);
    }
}

pub(crate) fn set_status_bar_visible(app: &mut ScratchpadApp, visible: bool) {
    if app.state.app_settings.ui.status_bar_visible == visible {
        app.state.pending_status_bar_visible = None;
        return;
    }

    app.state.app_settings.ui.status_bar_visible = visible;
    app.begin_layout_transition();
    persist_settings_or_error(app);
}

pub(crate) fn defer_status_bar_visible(
    app: &mut ScratchpadApp,
    visible: bool,
    ctx: &egui::Context,
) {
    app.state.pending_status_bar_visible =
        (app.state.app_settings.ui.status_bar_visible != visible).then_some(visible);
    if app.state.pending_status_bar_visible.is_some() {
        ctx.request_repaint();
    }
}

pub(crate) fn set_history_budget(app: &mut ScratchpadApp, budget: TextHistoryBudget) {
    if app.state.app_settings.set_history_budget(budget) {
        app.apply_history_budget_to_open_buffers();
        persist_settings_or_error(app);
    }
}

pub(crate) fn reset_history_budget_to_auto(app: &mut ScratchpadApp) {
    app.state.app_settings.reset_history_budget_to_auto();
    app.apply_history_budget_to_open_buffers();
    persist_settings_or_error(app);
}

pub(crate) fn set_tab_list_width_from_layout(app: &mut ScratchpadApp, width: f32) {
    let next = width.clamp(
        ScratchpadApp::VERTICAL_TAB_LIST_MIN_WIDTH,
        ScratchpadApp::VERTICAL_TAB_LIST_MAX_WIDTH,
    );
    if (app.state.app_settings.workspace.tab_list_width - next).abs() < 1.0 {
        return;
    }

    app.begin_layout_transition();
    set_tab_list_width(app, next);
}

pub(crate) fn open_settings(app: &mut ScratchpadApp) {
    open_settings_with_tab_selection(app, false);
}

pub(crate) fn open_settings_preserving_tab_selection(app: &mut ScratchpadApp) {
    open_settings_with_tab_selection(app, true);
}

fn open_settings_with_tab_selection(app: &mut ScratchpadApp, preserve_tab_selection: bool) {
    app.reload_settings_before_workspace_change();
    app.begin_layout_transition();
    if !app.settings_tab_open() {
        app.state.settings_preview_quote_index = (app.state.settings_preview_quote_index + 1)
            % crate::app::ui::settings::PREVIEW_QUOTES.len();
    }
    if set_settings_surface(app, AppSurface::Settings, true) {
        persist_settings_or_error(app);
    }
    if !preserve_tab_selection {
        app.select_only_tab_slot(app.active_tab_slot_index());
    }
}

pub(crate) fn open_settings_file_tab(app: &mut ScratchpadApp) {
    let path = app.settings_path().to_path_buf();
    activate_workspace_surface(app);
    FileController::open_paths_async(app, vec![path]);
}

pub(crate) fn close_settings(app: &mut ScratchpadApp) {
    app.begin_layout_transition();
    if set_settings_surface(app, AppSurface::Workspace, false) {
        persist_settings_or_error(app);
    }
    app.select_only_tab_slot(app.active_tab_slot_index());
    app.request_focus_for_active_view();
}

pub(crate) fn reset_settings_to_defaults(app: &mut ScratchpadApp) {
    app.initialize_default_workspace_tabs();
    app.apply_settings(AppSettings::default());
    if app.settings_tab_open() {
        app.state.active_surface = AppSurface::Settings;
    }
    app.state.applied_editor_font = None;
    app.select_only_tab_slot(app.active_tab_slot_index());
    let _ = app.persist_session_now();
    match app.persist_settings_now() {
        Ok(()) => app.state.status.set_info_status_in_domain(
            crate::app::app_state::StatusDomain::Settings,
            "Settings reset to defaults.",
        ),
        Err(error) => app.state.status.report_settings_save_failed(error),
    }
}

pub(crate) fn activate_workspace_surface(app: &mut ScratchpadApp) {
    if app.state.active_surface == AppSurface::Workspace {
        return;
    }
    app.state.active_surface = AppSurface::Workspace;
    app.tab_manager.mark_session_dirty();
}

pub(crate) fn keep_tab_list_open(app: &mut ScratchpadApp) {
    reset_tab_list_visibility_state(app, true);
}

pub(crate) fn delay_tab_list_hide(app: &mut ScratchpadApp, now: Instant) {
    app.state.vertical_tab_list_open = true;
    app.state.vertical_tab_list_hide_deadline =
        Some(now + app.state.app_settings.tab_list_auto_hide_delay());
}

pub(crate) fn close_tab_list(app: &mut ScratchpadApp) {
    reset_tab_list_visibility_state(app, false);
}

macro_rules! compat_scratchpad_app_methods {
    ($type:ty { $($item:item)* }) => {
        #[allow(dead_code)]
        impl $type {
            $($item)*
        }
    };
}

compat_scratchpad_app_methods!(ScratchpadApp {
    pub(super) fn persist_settings_or_error(&mut self) {
        persist_settings_or_error(self)
    }

    pub(crate) fn set_font_size(&mut self, font_size: f32) {
        set_font_size(self, font_size)
    }

    pub(crate) fn set_editor_font(&mut self, editor_font: EditorFontPreset) {
        set_editor_font(self, editor_font)
    }

    pub(crate) fn set_word_wrap(&mut self, enabled: bool) {
        set_word_wrap(self, enabled)
    }

    pub(crate) fn set_editor_gutter(&mut self, gutter: u8) {
        set_editor_gutter(self, gutter)
    }

    pub(crate) fn apply_theme_mode_preset(&mut self, theme_mode: AppThemeMode, system_theme: Option<egui::Theme>) {
        apply_theme_mode_preset(self, theme_mode, system_theme)
    }

    pub(crate) fn set_editor_text_color(&mut self, color: egui::Color32) {
        set_editor_text_color(self, color)
    }

    pub(crate) fn set_editor_background_color(&mut self, color: egui::Color32) {
        set_editor_background_color(self, color)
    }

    pub(crate) fn set_editor_text_highlight_color(&mut self, color: egui::Color32) {
        set_editor_text_highlight_color(self, color)
    }

    pub(crate) fn set_tab_list_position(&mut self, position: TabListPosition) {
        set_tab_list_position(self, position)
    }

    pub(crate) fn set_file_open_disposition(&mut self, disposition: FileOpenDisposition) {
        set_file_open_disposition(self, disposition)
    }

    pub(crate) fn set_new_tab_placement(&mut self, placement: NewTabPlacement) {
        set_new_tab_placement(self, placement)
    }

    pub(crate) fn set_startup_session_behavior(&mut self, behavior: StartupSessionBehavior) {
        set_startup_session_behavior(self, behavior)
    }

    pub(crate) fn set_auto_hide_tab_list(&mut self, enabled: bool) {
        set_auto_hide_tab_list(self, enabled)
    }

    pub(crate) fn set_tab_list_auto_hide_delay_seconds(&mut self, seconds: f32) {
        set_tab_list_auto_hide_delay_seconds(self, seconds)
    }

    pub(crate) fn set_recent_files_enabled(&mut self, enabled: bool) {
        set_recent_files_enabled(self, enabled)
    }

    pub(crate) fn set_status_bar_visible(&mut self, visible: bool) {
        set_status_bar_visible(self, visible)
    }

    pub(crate) fn defer_status_bar_visible(&mut self, visible: bool, ctx: &egui::Context) {
        defer_status_bar_visible(self, visible, ctx)
    }

    pub(crate) fn set_history_budget(&mut self, budget: TextHistoryBudget) {
        set_history_budget(self, budget)
    }

    pub(crate) fn reset_history_budget_to_auto(&mut self) {
        reset_history_budget_to_auto(self)
    }

    pub(crate) fn set_tab_list_width_from_layout(&mut self, width: f32) {
        set_tab_list_width_from_layout(self, width)
    }

    pub(crate) fn open_settings(&mut self) {
        open_settings(self)
    }

    pub(crate) fn open_settings_preserving_tab_selection(&mut self) {
        open_settings_preserving_tab_selection(self)
    }

    pub(crate) fn open_settings_file_tab(&mut self) {
        open_settings_file_tab(self)
    }

    pub(crate) fn close_settings(&mut self) {
        close_settings(self)
    }

    pub(crate) fn reset_settings_to_defaults(&mut self) {
        reset_settings_to_defaults(self)
    }

    pub(crate) fn activate_workspace_surface(&mut self) {
        activate_workspace_surface(self)
    }

    pub(crate) fn keep_tab_list_open(&mut self) {
        keep_tab_list_open(self)
    }

    pub(crate) fn delay_tab_list_hide(&mut self, now: Instant) {
        delay_tab_list_hide(self, now)
    }

    pub(crate) fn close_tab_list(&mut self) {
        close_tab_list(self)
    }
});
