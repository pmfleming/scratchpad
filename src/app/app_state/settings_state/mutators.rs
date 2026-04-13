#[cfg(test)]
use super::sync_stock_editor_palette_with_theme_mode;
use super::{
    AppSettings, AppSurface, AppThemeMode, FileController, LogLevel, ScratchpadApp,
    TabListPosition, color_to_hex, logging, sanitize_tab_list_auto_hide_delay_seconds,
    stock_editor_palette_for_selection,
};
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
            app.app_settings.word_wrap = next;
        });
    }

    pub(crate) fn set_editor_gutter(&mut self, gutter: u8) {
        let next = gutter.min(32);
        self.persist_settings_if_changed(self.app_settings.editor_gutter, next, |app, value| {
            app.app_settings.editor_gutter = value;
        });
    }

    #[cfg(test)]
    pub(crate) fn set_theme_mode(&mut self, theme_mode: AppThemeMode) {
        self.persist_settings_if_changed(self.app_settings.theme_mode, theme_mode, |app, next| {
            app.app_settings.theme_mode = next;
            sync_stock_editor_palette_with_theme_mode(&mut app.app_settings);
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

    fn set_editor_palette_color(&mut self, next: String, is_text_color: bool) {
        let current = if is_text_color {
            self.app_settings.editor_text_color.clone()
        } else {
            self.app_settings.editor_background_color.clone()
        };
        self.persist_settings_if_changed(current, next, |app, value| {
            if is_text_color {
                app.app_settings.editor_text_color = value;
            } else {
                app.app_settings.editor_background_color = value;
            }
        });
    }

    pub(crate) fn set_tab_list_position(&mut self, position: TabListPosition) {
        if self.app_settings.tab_list_position == position {
            return;
        }

        self.app_settings.tab_list_position = position;
        self.vertical_tab_list_open = false;
        self.vertical_tab_list_hide_deadline = None;
        if position.is_vertical() {
            self.overflow_popup_open = false;
        }
        self.tab_manager.pending_scroll_to_active = true;
        self.persist_settings_or_error();
    }

    pub(crate) fn set_auto_hide_tab_list(&mut self, enabled: bool) {
        if self.app_settings.auto_hide_tab_list == enabled {
            return;
        }

        self.app_settings.auto_hide_tab_list = enabled;
        if !enabled {
            self.vertical_tab_list_open = false;
        }
        self.vertical_tab_list_hide_deadline = None;
        self.persist_settings_or_error();
    }

    pub(crate) fn set_tab_list_auto_hide_delay_seconds(&mut self, seconds: f32) {
        let next = sanitize_tab_list_auto_hide_delay_seconds(seconds);
        if (self.app_settings.tab_list_auto_hide_delay_seconds - next).abs() < f32::EPSILON {
            return;
        }

        self.app_settings.tab_list_auto_hide_delay_seconds = next;
        self.vertical_tab_list_hide_deadline = None;
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

        self.app_settings.tab_list_width = next;
        self.persist_settings_or_error();
    }

    pub(crate) fn open_settings(&mut self) {
        self.reload_settings_from_active_settings_tab();
        let was_open = self.settings_tab_open();
        self.settings_tab_index = self.settings_tab_index.min(self.tabs().len());
        self.app_settings.settings_tab_open = true;
        self.active_surface = AppSurface::Settings;
        self.tab_manager.pending_scroll_to_active = true;
        if !was_open {
            self.persist_settings_or_error();
        }
    }

    pub(crate) fn open_settings_file_tab(&mut self) {
        let path = self.settings_path();
        self.activate_workspace_surface();
        FileController::open_paths(self, vec![path]);
    }

    pub(crate) fn close_settings(&mut self) {
        let was_open = self.settings_tab_open();
        self.app_settings.settings_tab_open = false;
        self.active_surface = AppSurface::Workspace;
        self.settings_tab_index = self.settings_tab_index.min(self.tabs().len());
        self.tab_manager.pending_scroll_to_active = true;
        if was_open {
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

    pub(crate) fn set_logging_enabled(&mut self, enabled: bool) {
        if self.app_settings.logging_enabled == enabled {
            return;
        }

        self.app_settings.logging_enabled = enabled;
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
        self.tab_manager.pending_scroll_to_active = true;
        self.mark_session_dirty();
        self.persist_settings_or_error();
    }

    pub(crate) fn activate_workspace_surface(&mut self) {
        self.active_surface = AppSurface::Workspace;
    }

    pub(crate) fn keep_tab_list_open(&mut self) {
        self.vertical_tab_list_open = true;
        self.vertical_tab_list_hide_deadline = None;
    }

    pub(crate) fn delay_tab_list_hide(&mut self, now: Instant) {
        self.vertical_tab_list_open = true;
        self.vertical_tab_list_hide_deadline = Some(now + self.tab_list_auto_hide_delay());
    }

    pub(crate) fn close_tab_list(&mut self) {
        self.vertical_tab_list_open = false;
        self.vertical_tab_list_hide_deadline = None;
    }
}
