use super::{AppSurface, ScratchpadApp};
use crate::app::fonts::EditorFontPreset;
use crate::app::paths_match;
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::{
    AppSettings, AppThemeMode, DEFAULT_EDITOR_BACKGROUND_COLOR, DEFAULT_EDITOR_TEXT_COLOR,
    DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR, DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR,
    DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS, FileOpenDisposition, LIGHT_EDITOR_BACKGROUND_COLOR,
    LIGHT_EDITOR_TEXT_COLOR, StartupSessionBehavior, TabListPosition, color_from_hex, color_to_hex,
};
use eframe::egui;
use std::path::Path;
use std::time::Duration;

mod display_tabs;
mod mutators;
#[cfg(test)]
mod tests;
mod toml_refresh;

impl ScratchpadApp {
    pub(crate) const VERTICAL_TAB_LIST_MIN_WIDTH: f32 = 96.0;
    pub(crate) const VERTICAL_TAB_LIST_MAX_WIDTH: f32 = 360.0;
    pub(crate) const TAB_LIST_AUTO_HIDE_DELAY_MIN_SECONDS: f32 = 0.0;
    pub(crate) const TAB_LIST_AUTO_HIDE_DELAY_MAX_SECONDS: f32 = 10.0;

    pub fn font_size(&self) -> f32 {
        self.app_settings.font_size
    }

    pub fn editor_font(&self) -> EditorFontPreset {
        self.app_settings.editor_font
    }

    pub fn editor_gutter(&self) -> u8 {
        self.app_settings.editor_gutter
    }

    pub fn theme_mode(&self) -> AppThemeMode {
        self.app_settings.theme_mode
    }

    pub(crate) fn has_custom_editor_palette(&self) -> bool {
        !uses_stock_editor_palette(&self.app_settings)
    }

    pub fn editor_text_color(&self) -> egui::Color32 {
        color_from_hex(
            &self.app_settings.editor_text_color,
            color_from_hex(DEFAULT_EDITOR_TEXT_COLOR, egui::Color32::WHITE),
        )
    }

    pub fn editor_background_color(&self) -> egui::Color32 {
        color_from_hex(
            &self.app_settings.editor_background_color,
            color_from_hex(DEFAULT_EDITOR_BACKGROUND_COLOR, egui::Color32::BLACK),
        )
    }

    pub fn editor_text_highlight_color(&self) -> egui::Color32 {
        color_from_hex(
            &self.app_settings.editor_text_highlight_color,
            color_from_hex(
                DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR,
                egui::Color32::from_rgb(255, 243, 109),
            ),
        )
    }

    pub fn editor_text_highlight_text_color(&self) -> egui::Color32 {
        color_from_hex(
            &self.app_settings.editor_text_highlight_text_color,
            color_from_hex(
                DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR,
                egui::Color32::BLACK,
            ),
        )
    }

    pub fn showing_settings(&self) -> bool {
        self.active_surface == AppSurface::Settings
    }

    pub(crate) fn settings_tab_open(&self) -> bool {
        self.app_settings.settings_tab_open
    }

    pub fn word_wrap(&self) -> bool {
        self.app_settings.word_wrap
    }

    pub fn tab_list_position(&self) -> TabListPosition {
        self.app_settings.tab_list_position
    }

    pub fn file_open_disposition(&self) -> FileOpenDisposition {
        self.app_settings.file_open_disposition
    }

    pub fn startup_session_behavior(&self) -> StartupSessionBehavior {
        self.app_settings.startup_session_behavior
    }

    pub fn tab_list_width(&self) -> f32 {
        self.app_settings.tab_list_width
    }

    pub(crate) fn vertical_tab_list_width(&self) -> f32 {
        self.app_settings.tab_list_width.clamp(
            Self::VERTICAL_TAB_LIST_MIN_WIDTH,
            Self::VERTICAL_TAB_LIST_MAX_WIDTH,
        )
    }

    pub fn auto_hide_tab_list(&self) -> bool {
        self.app_settings.auto_hide_tab_list
    }

    pub fn tab_list_auto_hide_delay_seconds(&self) -> f32 {
        self.app_settings.tab_list_auto_hide_delay_seconds
    }

    pub fn recent_files_enabled(&self) -> bool {
        self.app_settings.recent_files_enabled
    }

    pub(crate) fn tab_list_auto_hide_delay(&self) -> Duration {
        Duration::from_secs_f32(sanitize_tab_list_auto_hide_delay_seconds(
            self.app_settings.tab_list_auto_hide_delay_seconds,
        ))
    }

    pub fn settings_path(&self) -> &Path {
        self.settings_store.path()
    }

    pub(crate) fn is_settings_file_path(&self, path: &Path) -> bool {
        paths_match(path, self.settings_path())
    }

    pub(crate) fn mark_active_buffer_as_settings_file(&mut self) {
        let settings_path = self.settings_path().to_path_buf();
        let Some(tab) = self.active_tab_mut() else {
            return;
        };
        let buffer = tab.active_buffer_mut();
        if buffer
            .path
            .as_ref()
            .is_some_and(|path| paths_match(path, &settings_path))
            && !buffer.is_settings_file
        {
            buffer.is_settings_file = true;
            self.mark_session_dirty();
        }
    }

    pub(super) fn load_settings_from_store(&mut self) -> bool {
        match self.load_settings_snapshot() {
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

    fn load_settings_snapshot(&self) -> std::io::Result<Option<AppSettings>> {
        self.settings_store.load()
    }

    pub(super) fn apply_settings(&mut self, settings: AppSettings) {
        let mut settings = settings;
        sync_stock_editor_palette_with_theme_mode(&mut settings);
        settings.tab_list_auto_hide_delay_seconds =
            sanitize_tab_list_auto_hide_delay_seconds(settings.tab_list_auto_hide_delay_seconds);
        self.active_surface = if settings.settings_tab_open {
            AppSurface::Settings
        } else {
            AppSurface::Workspace
        };
        self.settings_tab_index = settings.settings_tab_index.unwrap_or(usize::MAX);
        self.app_settings = settings;
    }

    fn refresh_settings_snapshot(&mut self) {
        self.app_settings.settings_tab_open = self.settings_tab_open();
        self.app_settings.settings_tab_index = (self.settings_tab_index != usize::MAX)
            .then_some(self.settings_tab_index.min(self.tabs().len()));
    }

    pub(crate) fn persist_settings_now(&mut self) -> std::io::Result<()> {
        self.refresh_settings_snapshot();
        self.settings_store.save(&self.app_settings)
    }

    pub fn apply_theme_to_context(&self, ctx: &egui::Context) {
        ctx.set_theme(self.app_settings.theme_mode.theme_preference());
        ctx.set_visuals_of(egui::Theme::Dark, egui::Visuals::dark());
        ctx.set_visuals_of(egui::Theme::Light, egui::Visuals::light());
    }
}

pub(super) fn sync_stock_editor_palette_with_theme_mode(settings: &mut AppSettings) {
    let Some((text_color, background_color)) = stock_editor_palette(settings.theme_mode) else {
        return;
    };

    if !uses_stock_editor_palette(settings) {
        return;
    }

    if settings.editor_text_color == text_color
        && settings.editor_background_color == background_color
    {
        return;
    }

    settings.editor_text_color = text_color.to_owned();
    settings.editor_background_color = background_color.to_owned();
}

pub(super) fn uses_stock_editor_palette(settings: &AppSettings) -> bool {
    matches!(
        (
            settings.editor_text_color.as_str(),
            settings.editor_background_color.as_str(),
        ),
        (DEFAULT_EDITOR_TEXT_COLOR, DEFAULT_EDITOR_BACKGROUND_COLOR)
            | (LIGHT_EDITOR_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR)
    )
}

pub(super) fn stock_editor_palette(
    theme_mode: AppThemeMode,
) -> Option<(&'static str, &'static str)> {
    match theme_mode {
        AppThemeMode::System => None,
        AppThemeMode::Light => Some((LIGHT_EDITOR_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR)),
        AppThemeMode::Dark => Some((DEFAULT_EDITOR_TEXT_COLOR, DEFAULT_EDITOR_BACKGROUND_COLOR)),
    }
}

pub(super) fn stock_editor_palette_for_selection(
    theme_mode: AppThemeMode,
    system_theme: Option<egui::Theme>,
) -> (&'static str, &'static str) {
    match theme_mode {
        AppThemeMode::System => match system_theme.unwrap_or(egui::Theme::Dark) {
            egui::Theme::Light => (LIGHT_EDITOR_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR),
            egui::Theme::Dark => (DEFAULT_EDITOR_TEXT_COLOR, DEFAULT_EDITOR_BACKGROUND_COLOR),
        },
        AppThemeMode::Light => (LIGHT_EDITOR_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR),
        AppThemeMode::Dark => (DEFAULT_EDITOR_TEXT_COLOR, DEFAULT_EDITOR_BACKGROUND_COLOR),
    }
}

fn sanitize_tab_list_auto_hide_delay_seconds(seconds: f32) -> f32 {
    if !seconds.is_finite() {
        return DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS;
    }

    seconds.clamp(
        ScratchpadApp::TAB_LIST_AUTO_HIDE_DELAY_MIN_SECONDS,
        ScratchpadApp::TAB_LIST_AUTO_HIDE_DELAY_MAX_SECONDS,
    )
}
