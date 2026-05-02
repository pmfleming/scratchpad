use super::{
    AppSettings, AppSurface, AppThemeMode, FileController, FileOpenDisposition, ScratchpadApp,
    StartupSessionBehavior, TabListPosition, color_to_hex,
    sanitize_tab_list_auto_hide_delay_seconds, stock_editor_palette_for_selection,
};
use crate::app::domain::TextHistoryBudget;
use crate::app::fonts::EditorFontPreset;
use eframe::egui;
use std::time::Instant;

impl ScratchpadApp {
    fn persist_settings_or_error(&mut self) {
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    fn persist_settings_if_changed<T, F>(&mut self, current: T, next: T, apply: F)
    where
        T: PartialEq,
        F: FnOnce(&mut Self, T),
    {
        if current == next {
            return;
        }

        apply(self, next);
        self.persist_settings_or_error();
    }

    fn reset_tab_list_visibility_state(&mut self, keep_open: bool) {
        self.vertical_tab_list_open = keep_open;
        self.vertical_tab_list_hide_deadline = None;
    }

    fn clear_tab_list_hide_deadline(&mut self) {
        self.vertical_tab_list_hide_deadline = None;
    }

    fn set_tab_list_width(&mut self, width: f32) {
        self.app_settings.tab_list_width = width;
        self.persist_settings_or_error();
    }

    fn set_settings_surface(&mut self, surface: AppSurface, open: bool) -> bool {
        let changed = self.settings_tab_open() != open;
        self.settings_tab_index = self.settings_tab_index.min(self.tabs().len());
        self.app_settings.settings_tab_open = open;
        self.active_surface = surface;
        self.ensure_active_tab_slot_selected();
        self.tab_manager.pending_scroll_to_active = true;
        changed
    }

    pub(crate) fn set_font_size(&mut self, font_size: f32) {
        let next = font_size.clamp(8.0, 72.0);
        if (self.app_settings.font_size - next).abs() < f32::EPSILON {
            return;
        }

        self.app_settings.font_size = next;
        self.persist_settings_or_error();
    }

    pub(crate) fn set_editor_font(&mut self, editor_font: EditorFontPreset) {
        self.persist_settings_if_changed(
            self.app_settings.editor_font,
            editor_font,
            |app, next| {
                app.app_settings.editor_font = next;
                app.applied_editor_font = None;
            },
        );
    }

    pub(crate) fn set_word_wrap(&mut self, enabled: bool) {
        self.persist_settings_if_changed(self.app_settings.word_wrap, enabled, |app, next| {
            app.app_settings.word_wrap = next
        });
    }

    pub(crate) fn set_editor_gutter(&mut self, gutter: u8) {
        let next = gutter.min(32);
        self.persist_settings_if_changed(self.app_settings.editor_gutter, next, |app, value| {
            app.app_settings.editor_gutter = value
        });
    }

    pub(crate) fn apply_theme_mode_preset(
        &mut self,
        theme_mode: AppThemeMode,
        system_theme: Option<egui::Theme>,
    ) {
        let (text_color, background_color) =
            stock_editor_palette_for_selection(theme_mode, system_theme);
        if self.app_settings.theme_mode == theme_mode
            && self.app_settings.editor_text_color == text_color
            && self.app_settings.editor_background_color == background_color
        {
            return;
        }

        self.app_settings.theme_mode = theme_mode;
        self.app_settings.editor_text_color = text_color.to_owned();
        self.app_settings.editor_background_color = background_color.to_owned();
        self.persist_settings_or_error();
    }

    pub(crate) fn set_editor_text_color(&mut self, color: egui::Color32) {
        self.set_editor_palette_color(color_to_hex(color), true);
    }

    pub(crate) fn set_editor_background_color(&mut self, color: egui::Color32) {
        self.set_editor_palette_color(color_to_hex(color), false);
    }

    pub(crate) fn set_editor_text_highlight_color(&mut self, color: egui::Color32) {
        let next = color_to_hex(color);
        let next_text = color_to_hex(crate::app::color_contrast::optimal_text_color(color));
        if self.app_settings.editor_text_highlight_color == next
            && self.app_settings.editor_text_highlight_text_color == next_text
        {
            return;
        }

        self.app_settings.editor_text_highlight_color = next;
        self.app_settings.editor_text_highlight_text_color = next_text;
        self.persist_settings_or_error();
    }

    fn set_editor_palette_color(&mut self, next: String, is_text_color: bool) {
        let changed = {
            let current = if is_text_color {
                &mut self.app_settings.editor_text_color
            } else {
                &mut self.app_settings.editor_background_color
            };
            if *current == next {
                false
            } else {
                *current = next;
                true
            }
        };

        if changed {
            self.persist_settings_or_error();
        }
    }

    pub(crate) fn set_tab_list_position(&mut self, position: TabListPosition) {
        if self.app_settings.tab_list_position == position {
            return;
        }

        self.app_settings.tab_list_position = position;
        self.begin_layout_transition();
        self.reset_tab_list_visibility_state(false);
        if position.is_vertical() {
            self.overflow_popup_open = false;
        }
        self.tab_manager.pending_scroll_to_active = true;
        self.persist_settings_or_error();
    }

    pub(crate) fn set_file_open_disposition(&mut self, disposition: FileOpenDisposition) {
        self.persist_settings_if_changed(
            self.app_settings.file_open_disposition,
            disposition,
            |app, next| app.app_settings.file_open_disposition = next,
        );
    }

    pub(crate) fn set_startup_session_behavior(&mut self, behavior: StartupSessionBehavior) {
        self.persist_settings_if_changed(
            self.app_settings.startup_session_behavior,
            behavior,
            |app, next| app.app_settings.startup_session_behavior = next,
        );
    }

    pub(crate) fn set_auto_hide_tab_list(&mut self, enabled: bool) {
        if self.app_settings.auto_hide_tab_list == enabled {
            return;
        }

        self.app_settings.auto_hide_tab_list = enabled;
        self.begin_layout_transition();
        self.reset_tab_list_visibility_state(enabled && self.vertical_tab_list_open);
        self.persist_settings_or_error();
    }

    pub(crate) fn set_tab_list_auto_hide_delay_seconds(&mut self, seconds: f32) {
        let next = sanitize_tab_list_auto_hide_delay_seconds(seconds);
        if (self.app_settings.tab_list_auto_hide_delay_seconds - next).abs() < f32::EPSILON {
            return;
        }

        self.app_settings.tab_list_auto_hide_delay_seconds = next;
        self.clear_tab_list_hide_deadline();
        self.persist_settings_or_error();
    }

    pub(crate) fn set_recent_files_enabled(&mut self, enabled: bool) {
        self.persist_settings_if_changed(
            self.app_settings.recent_files_enabled,
            enabled,
            |app, next| app.app_settings.recent_files_enabled = next,
        );
    }

    pub(crate) fn set_status_bar_visible(&mut self, visible: bool) {
        if self.app_settings.status_bar_visible == visible {
            return;
        }

        self.app_settings.status_bar_visible = visible;
        self.begin_layout_transition();
        self.persist_settings_or_error();
    }

    pub(crate) fn set_history_budget(&mut self, mut budget: TextHistoryBudget) {
        budget = budget.sanitized();
        if self.app_settings.history_budget == budget {
            return;
        }
        budget.derived_from_memory = false;
        self.app_settings.history_budget = budget;
        self.apply_history_budget_to_open_buffers();
        self.persist_settings_or_error();
    }

    pub(crate) fn reset_history_budget_to_auto(&mut self) {
        self.app_settings.history_budget = TextHistoryBudget::derive_from_available_memory();
        self.apply_history_budget_to_open_buffers();
        self.persist_settings_or_error();
    }

    pub(crate) fn set_tab_list_width_from_layout(&mut self, width: f32) {
        let next = width.clamp(
            Self::VERTICAL_TAB_LIST_MIN_WIDTH,
            Self::VERTICAL_TAB_LIST_MAX_WIDTH,
        );
        if (self.app_settings.tab_list_width - next).abs() < 1.0 {
            return;
        }

        self.begin_layout_transition();
        self.set_tab_list_width(next);
    }

    pub(crate) fn open_settings(&mut self) {
        self.reload_settings_before_workspace_change();
        self.begin_layout_transition();
        if !self.settings_tab_open() {
            self.settings_preview_quote_index = (self.settings_preview_quote_index + 1)
                % crate::app::ui::settings::PREVIEW_QUOTES.len();
        }
        if self.set_settings_surface(AppSurface::Settings, true) {
            self.persist_settings_or_error();
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
        self.request_focus_for_active_view();
    }

    pub(crate) fn reset_settings_to_defaults(&mut self) {
        let defaults = AppSettings {
            settings_tab_open: self.settings_tab_open(),
            settings_tab_index: Some(self.settings_tab_index.min(self.tabs().len())),
            ..AppSettings::default()
        };
        self.apply_settings(defaults);
        self.applied_editor_font = None;
        match self.persist_settings_now() {
            Ok(()) => self.set_info_status("Settings reset to defaults."),
            Err(error) => self.set_error_status(format!("Settings save failed: {error}")),
        }
    }

    pub(crate) fn apply_workspace_tab_order(&mut self, workspace_order: Vec<usize>) {
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
        self.ensure_active_tab_slot_selected();
        self.tab_manager.pending_scroll_to_active = true;
        self.mark_session_dirty();
        self.persist_settings_or_error();
    }

    pub(crate) fn activate_workspace_surface(&mut self) {
        self.active_surface = AppSurface::Workspace;
    }

    pub(crate) fn keep_tab_list_open(&mut self) {
        self.reset_tab_list_visibility_state(true);
    }

    pub(crate) fn delay_tab_list_hide(&mut self, now: Instant) {
        self.vertical_tab_list_open = true;
        self.vertical_tab_list_hide_deadline = Some(now + self.tab_list_auto_hide_delay());
    }

    pub(crate) fn close_tab_list(&mut self) {
        self.reset_tab_list_visibility_state(false);
    }
}
