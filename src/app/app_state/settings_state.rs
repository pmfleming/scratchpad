use super::{AppSurface, ScratchpadApp};
use crate::app::fonts::EditorFontPreset;
use crate::app::paths_match;
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::{
    AppSettings, AppThemeMode, DEFAULT_EDITOR_BACKGROUND_COLOR, DEFAULT_EDITOR_TEXT_COLOR,
    DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR, DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR,
    DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS, FileOpenDisposition,
    LEGACY_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR,
    LIGHT_EDITOR_TEXT_COLOR, NewTabPlacement, StartupSessionBehavior, TabListPosition,
    TabOrderMode, WindowState, color_from_hex, color_to_hex,
};
use eframe::egui;
use std::path::Path;
use std::time::Duration;

mod history_budget;
pub(crate) mod mutators;
mod tab_order;
mod toml_refresh;
mod window;

impl AppSettings {
    pub fn font_size(&self) -> f32 {
        self.font_size
    }

    pub fn editor_font(&self) -> EditorFontPreset {
        self.editor_font
    }

    pub fn editor_gutter(&self) -> u8 {
        self.editor_gutter
    }

    pub fn theme_mode(&self) -> AppThemeMode {
        self.theme_mode
    }

    pub(crate) fn has_custom_editor_palette(&self) -> bool {
        !uses_stock_editor_palette(self)
    }

    pub fn editor_text_color(&self) -> egui::Color32 {
        color_from_hex(
            &self.editor_text_color,
            color_from_hex(DEFAULT_EDITOR_TEXT_COLOR, egui::Color32::WHITE),
        )
    }

    pub fn editor_background_color(&self) -> egui::Color32 {
        color_from_hex(
            &self.editor_background_color,
            color_from_hex(DEFAULT_EDITOR_BACKGROUND_COLOR, egui::Color32::BLACK),
        )
    }

    pub fn editor_text_highlight_color(&self) -> egui::Color32 {
        color_from_hex(
            &self.editor_text_highlight_color,
            color_from_hex(
                DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR,
                egui::Color32::from_rgb(255, 243, 109),
            ),
        )
    }

    pub fn editor_text_highlight_text_color(&self) -> egui::Color32 {
        let generated =
            crate::app::color_contrast::optimal_text_color(self.editor_text_highlight_color());
        if uses_generated_highlight_text_color(&self.editor_text_highlight_text_color) {
            return generated;
        }

        color_from_hex(&self.editor_text_highlight_text_color, generated)
    }

    pub fn word_wrap(&self) -> bool {
        self.word_wrap
    }

    pub fn tab_list_position(&self) -> TabListPosition {
        self.tab_list_position
    }

    pub fn tab_order_mode(&self) -> TabOrderMode {
        self.tab_order_mode
    }

    pub fn file_open_disposition(&self) -> FileOpenDisposition {
        self.file_open_disposition
    }

    pub fn new_tab_placement(&self) -> NewTabPlacement {
        self.new_tab_placement
    }

    pub fn startup_session_behavior(&self) -> StartupSessionBehavior {
        self.startup_session_behavior
    }

    pub fn tab_list_width(&self) -> f32 {
        self.tab_list_width
    }

    pub fn auto_hide_tab_list(&self) -> bool {
        self.auto_hide_tab_list
    }

    pub fn tab_list_auto_hide_delay_seconds(&self) -> f32 {
        self.tab_list_auto_hide_delay_seconds
    }

    pub fn recent_files_enabled(&self) -> bool {
        self.recent_files_enabled
    }

    pub fn status_bar_visible(&self) -> bool {
        self.status_bar_visible
    }

    pub(crate) fn tab_list_auto_hide_delay(&self) -> Duration {
        Duration::from_secs_f32(sanitize_tab_list_auto_hide_delay_seconds(
            self.tab_list_auto_hide_delay_seconds,
        ))
    }
}

impl ScratchpadApp {
    pub(crate) const VERTICAL_TAB_LIST_MIN_WIDTH: f32 = 96.0;
    pub(crate) const VERTICAL_TAB_LIST_MAX_WIDTH: f32 = 360.0;
    pub(crate) const TAB_LIST_AUTO_HIDE_DELAY_MIN_SECONDS: f32 = 0.0;
    pub(crate) const TAB_LIST_AUTO_HIDE_DELAY_MAX_SECONDS: f32 = 10.0;

    pub fn showing_settings(&self) -> bool {
        self.state.active_surface == AppSurface::Settings
    }

    pub(crate) fn settings_tab_open(&self) -> bool {
        self.state.app_settings.settings_tab_open
    }

    pub(crate) fn vertical_tab_list_width(&self) -> f32 {
        self.state.app_settings.tab_list_width.clamp(
            Self::VERTICAL_TAB_LIST_MIN_WIDTH,
            Self::VERTICAL_TAB_LIST_MAX_WIDTH,
        )
    }

    pub fn settings_path(&self) -> &Path {
        self.state.settings_store.path()
    }

    pub(crate) fn is_settings_file_path(&self, path: &Path) -> bool {
        paths_match(path, self.settings_path())
    }

    pub(crate) fn mark_active_buffer_as_settings_file(&mut self) {
        let settings_path = self.settings_path().to_path_buf();
        let Some(tab) = self.tab_manager.active_tab_mut() else {
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
            self.tab_manager.mark_session_dirty();
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
                self.state.status.report_settings_load_failed(error);
                false
            }
        }
    }

    fn load_settings_snapshot(&self) -> std::io::Result<Option<AppSettings>> {
        self.state.settings_store.load()
    }

    pub(super) fn apply_settings(&mut self, settings: AppSettings) {
        let mut settings = settings;
        sync_stock_editor_palette_with_theme_mode(&mut settings);
        settings.history_budget = settings.history_budget.sanitized();
        settings.tab_list_auto_hide_delay_seconds =
            sanitize_tab_list_auto_hide_delay_seconds(settings.tab_list_auto_hide_delay_seconds);
        self.state.active_surface = if settings.settings_tab_open {
            AppSurface::Settings
        } else {
            AppSurface::Workspace
        };
        self.state.settings_tab_index = settings.settings_tab_index.unwrap_or(usize::MAX);
        self.state.app_settings = settings;
        self.apply_history_budget_to_open_buffers();
    }

    fn refresh_settings_snapshot(&mut self) {
        self.state.app_settings.settings_tab_open = self.settings_tab_open();
        self.state.app_settings.settings_tab_index = (self.state.settings_tab_index != usize::MAX)
            .then_some(
                self.state
                    .settings_tab_index
                    .min(self.tab_manager.tabs.as_slice().len()),
            );
    }

    pub(crate) fn persist_settings_now(&mut self) -> std::io::Result<()> {
        self.refresh_settings_snapshot();
        self.state.settings_store.save(&self.state.app_settings)
    }

    pub fn apply_theme_to_context(&self, ctx: &egui::Context) {
        ctx.set_theme(self.state.app_settings.theme_mode.theme_preference());
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

fn uses_generated_highlight_text_color(hex: &str) -> bool {
    let normalized = hex.trim().trim_start_matches('#');
    normalized
        .eq_ignore_ascii_case(DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR.trim_start_matches('#'))
        || normalized
            .eq_ignore_ascii_case(LEGACY_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR.trim_start_matches('#'))
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
