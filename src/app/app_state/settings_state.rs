use super::{AppSurface, ScratchpadApp};
use crate::app::fonts::EditorFontPreset;
use crate::app::logging::{self, LogLevel};
use crate::app::paths_match;
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::{
    AppSettings, AppThemeMode, DEFAULT_EDITOR_BACKGROUND_COLOR, DEFAULT_EDITOR_TEXT_COLOR,
    LIGHT_EDITOR_BACKGROUND_COLOR, LIGHT_EDITOR_TEXT_COLOR, TabListPosition, color_from_hex,
    color_to_hex,
};
use eframe::egui;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

mod display_tabs;
mod toml_refresh;

impl ScratchpadApp {
    pub(crate) const VERTICAL_TAB_LIST_MIN_WIDTH: f32 = 96.0;
    pub(crate) const VERTICAL_TAB_LIST_MAX_WIDTH: f32 = 360.0;
    pub(crate) const VERTICAL_TAB_LIST_AUTO_HIDE_DELAY: Duration = Duration::from_secs(3);

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

    pub fn showing_settings(&self) -> bool {
        self.active_surface == AppSurface::Settings
    }

    pub(crate) fn settings_tab_open(&self) -> bool {
        self.app_settings.settings_tab_open
    }

    pub fn word_wrap(&self) -> bool {
        self.app_settings.word_wrap
    }

    pub fn logging_enabled(&self) -> bool {
        self.app_settings.logging_enabled
    }

    pub fn tab_list_position(&self) -> TabListPosition {
        self.app_settings.tab_list_position
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

    pub fn settings_path(&self) -> PathBuf {
        self.settings_store.path().to_path_buf()
    }

    pub(crate) fn is_settings_file_path(&self, path: &Path) -> bool {
        paths_match(path, &self.settings_path())
    }

    pub(crate) fn mark_active_buffer_as_settings_file(&mut self) {
        let settings_path = self.settings_path();
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

    pub(crate) fn set_font_size(&mut self, font_size: f32) {
        let next = font_size.clamp(8.0, 72.0);
        if (self.app_settings.font_size - next).abs() < f32::EPSILON {
            return;
        }

        self.app_settings.font_size = next;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn set_editor_font(&mut self, editor_font: EditorFontPreset) {
        if self.app_settings.editor_font == editor_font {
            return;
        }

        self.app_settings.editor_font = editor_font;
        self.applied_editor_font = None;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn set_word_wrap(&mut self, enabled: bool) {
        if self.app_settings.word_wrap == enabled {
            return;
        }

        self.app_settings.word_wrap = enabled;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn set_editor_gutter(&mut self, gutter: u8) {
        let next = gutter.min(32);
        if self.app_settings.editor_gutter == next {
            return;
        }

        self.app_settings.editor_gutter = next;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    #[cfg(test)]
    pub(crate) fn set_theme_mode(&mut self, theme_mode: AppThemeMode) {
        if self.app_settings.theme_mode == theme_mode {
            return;
        }

        self.app_settings.theme_mode = theme_mode;
        sync_stock_editor_palette_with_theme_mode(&mut self.app_settings);
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
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
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn set_editor_text_color(&mut self, color: egui::Color32) {
        let next = color_to_hex(color);
        if self.app_settings.editor_text_color == next {
            return;
        }

        self.app_settings.editor_text_color = next;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn set_editor_background_color(&mut self, color: egui::Color32) {
        let next = color_to_hex(color);
        if self.app_settings.editor_background_color == next {
            return;
        }

        self.app_settings.editor_background_color = next;
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
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
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
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
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
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
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn open_settings(&mut self) {
        self.reload_settings_from_active_settings_tab();
        let was_open = self.settings_tab_open();
        self.settings_tab_index = self.settings_tab_index.min(self.tabs().len());
        self.app_settings.settings_tab_open = true;
        self.active_surface = AppSurface::Settings;
        self.tab_manager.pending_scroll_to_active = true;
        if !was_open && let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
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
        if was_open && let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
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
        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
            return;
        }

        self.set_info_status("Settings reset to defaults.");
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

        if let Err(error) = self.persist_settings_now() {
            self.set_error_status(format!("Settings save failed: {error}"));
        }
    }

    pub(crate) fn activate_workspace_surface(&mut self) {
        self.active_surface = AppSurface::Workspace;
    }

    pub(crate) fn vertical_tab_list_hide_deadline_active(&self, now: Instant) -> bool {
        self.vertical_tab_list_hide_deadline
            .is_some_and(|deadline| deadline > now)
    }

    pub(crate) fn keep_vertical_tab_list_open(&mut self) {
        self.vertical_tab_list_open = true;
        self.vertical_tab_list_hide_deadline = None;
    }

    pub(crate) fn delay_vertical_tab_list_hide(&mut self, now: Instant) {
        self.vertical_tab_list_open = true;
        self.vertical_tab_list_hide_deadline = Some(now + Self::VERTICAL_TAB_LIST_AUTO_HIDE_DELAY);
    }

    pub(crate) fn close_vertical_tab_list(&mut self) {
        self.vertical_tab_list_open = false;
        self.vertical_tab_list_hide_deadline = None;
    }
}

fn sync_stock_editor_palette_with_theme_mode(settings: &mut AppSettings) {
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

fn uses_stock_editor_palette(settings: &AppSettings) -> bool {
    matches!(
        (
            settings.editor_text_color.as_str(),
            settings.editor_background_color.as_str(),
        ),
        (DEFAULT_EDITOR_TEXT_COLOR, DEFAULT_EDITOR_BACKGROUND_COLOR)
            | (LIGHT_EDITOR_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR)
    )
}

fn stock_editor_palette(theme_mode: AppThemeMode) -> Option<(&'static str, &'static str)> {
    match theme_mode {
        AppThemeMode::System => None,
        AppThemeMode::Light => Some((LIGHT_EDITOR_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR)),
        AppThemeMode::Dark => Some((DEFAULT_EDITOR_TEXT_COLOR, DEFAULT_EDITOR_BACKGROUND_COLOR)),
    }
}

fn stock_editor_palette_for_selection(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::services::session_store::SessionStore;

    fn test_app() -> ScratchpadApp {
        let session_root = tempfile::tempdir().expect("create session dir");
        let session_store = SessionStore::new(session_root.path().to_path_buf());
        ScratchpadApp::with_session_store(session_store)
    }

    fn set_custom_palette(app: &mut ScratchpadApp) {
        app.set_editor_text_color(egui::Color32::from_rgb(12, 34, 56));
        app.set_editor_background_color(egui::Color32::from_rgb(210, 220, 230));
    }

    #[test]
    fn applying_light_mode_settings_migrates_stock_editor_palette() {
        let mut app = test_app();

        app.apply_settings(AppSettings {
            theme_mode: AppThemeMode::Light,
            editor_text_color: DEFAULT_EDITOR_TEXT_COLOR.to_owned(),
            editor_background_color: DEFAULT_EDITOR_BACKGROUND_COLOR.to_owned(),
            ..AppSettings::default()
        });

        assert_eq!(app.editor_text_color(), egui::Color32::BLACK);
        assert_eq!(app.editor_background_color(), egui::Color32::WHITE);
    }

    #[test]
    fn switching_to_light_mode_updates_stock_editor_palette() {
        let mut app = test_app();

        app.set_theme_mode(AppThemeMode::Light);

        assert_eq!(app.editor_text_color(), egui::Color32::BLACK);
        assert_eq!(app.editor_background_color(), egui::Color32::WHITE);
    }

    #[test]
    fn switching_theme_preserves_custom_editor_palette() {
        let mut app = test_app();

        let custom_text = egui::Color32::from_rgb(12, 34, 56);
        let custom_background = egui::Color32::from_rgb(210, 220, 230);
        app.set_editor_text_color(custom_text);
        app.set_editor_background_color(custom_background);

        app.set_theme_mode(AppThemeMode::Light);

        assert_eq!(app.editor_text_color(), custom_text);
        assert_eq!(app.editor_background_color(), custom_background);
    }

    #[test]
    fn custom_palette_is_detected_after_editor_color_change() {
        let mut app = test_app();

        assert!(!app.has_custom_editor_palette());

        app.set_editor_text_color(egui::Color32::from_rgb(12, 34, 56));

        assert!(app.has_custom_editor_palette());
    }

    #[test]
    fn applying_light_theme_preset_clears_custom_palette() {
        let mut app = test_app();
        set_custom_palette(&mut app);

        app.apply_theme_mode_preset(AppThemeMode::Light, Some(egui::Theme::Light));

        assert_eq!(app.theme_mode(), AppThemeMode::Light);
        assert!(!app.has_custom_editor_palette());
        assert_eq!(app.editor_text_color(), egui::Color32::BLACK);
        assert_eq!(app.editor_background_color(), egui::Color32::WHITE);
    }

    #[test]
    fn applying_system_theme_preset_clears_custom_palette() {
        let mut app = test_app();
        set_custom_palette(&mut app);

        app.apply_theme_mode_preset(AppThemeMode::System, Some(egui::Theme::Dark));

        assert_eq!(app.theme_mode(), AppThemeMode::System);
        assert!(!app.has_custom_editor_palette());
        assert_eq!(app.editor_text_color(), egui::Color32::WHITE);
        assert_eq!(
            app.editor_background_color(),
            egui::Color32::from_rgb(21, 24, 29)
        );
    }
}
