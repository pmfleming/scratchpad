use super::ScratchpadApp;
use crate::app::fonts::{EditorFontPreset, EditorFontSelection, EditorFontSource};
use crate::app::paths_match;
use crate::app::platform::PlatformProfile;
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::{
    AppSettings, AppThemeMode, DEFAULT_EDITOR_BACKGROUND_COLOR, DEFAULT_EDITOR_TEXT_COLOR,
    DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR, DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR,
    DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS, EditorAppearanceSource, FileOpenDisposition,
    IndentationStyle, LEGACY_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR, LIGHT_EDITOR_BACKGROUND_COLOR,
    LIGHT_EDITOR_TEXT_COLOR, NewTabPlacement, StartupSessionBehavior, TabDisplayMode,
    TabListPosition, TabOrderDirection, TabOrderMode, WindowState, color_from_hex,
};
use eframe::egui;
use std::path::Path;
use std::time::Duration;

mod history_budget;
pub(crate) mod mutators;
mod tab_order;
mod toml_refresh;
mod window;

impl ScratchpadApp {
    pub(crate) const VERTICAL_TAB_LIST_MIN_WIDTH: f32 = VERTICAL_TAB_LIST_MIN_WIDTH;
    pub(crate) const VERTICAL_TAB_LIST_MAX_WIDTH: f32 = VERTICAL_TAB_LIST_MAX_WIDTH;
    pub(crate) const TAB_LIST_AUTO_HIDE_DELAY_MIN_SECONDS: f32 =
        TAB_LIST_AUTO_HIDE_DELAY_MIN_SECONDS;
    pub(crate) const TAB_LIST_AUTO_HIDE_DELAY_MAX_SECONDS: f32 =
        TAB_LIST_AUTO_HIDE_DELAY_MAX_SECONDS;
}

impl AppSettings {
    #[must_use]
    pub fn font_size(&self) -> f32 {
        if self.editor_appearance_source() == EditorAppearanceSource::System {
            return crate::app::system_appearance::editor_font_size();
        }

        self.editor.font_size
    }

    #[must_use]
    pub fn editor_appearance_source(&self) -> EditorAppearanceSource {
        self.editor.appearance_source
    }

    #[must_use]
    pub fn editor_font(&self) -> EditorFontPreset {
        self.editor.editor_font
    }

    #[must_use]
    pub fn editor_font_source(&self) -> EditorFontSource {
        self.editor.font_source
    }

    #[must_use]
    pub fn os_font_family(&self) -> &str {
        self.editor.os_font_family.as_str()
    }

    #[must_use]
    pub fn editor_font_selection(&self) -> EditorFontSelection {
        if self.editor_appearance_source() == EditorAppearanceSource::System {
            return EditorFontSelection::os(
                crate::app::system_appearance::editor_font_family(),
                self.editor.editor_font,
            );
        }

        let os_family = self.editor.os_font_family.trim();
        let os_family = (!os_family.is_empty()).then(|| os_family.to_owned());
        match self.editor.font_source {
            EditorFontSource::Scratchpad => {
                EditorFontSelection::scratchpad(self.editor.editor_font)
            }
            EditorFontSource::Os => EditorFontSelection::os(os_family, self.editor.editor_font),
        }
    }

    #[must_use]
    pub fn editor_gutter(&self) -> u8 {
        self.editor.editor_gutter
    }

    #[must_use]
    pub fn editor_tab_width(&self) -> u8 {
        self.editor.editor_tab_width.clamp(1, 16)
    }

    #[must_use]
    pub fn indentation_style(&self) -> IndentationStyle {
        self.editor.indentation_style
    }

    #[must_use]
    pub fn tab_display(&self) -> TabDisplayMode {
        self.editor.tab_display
    }

    #[must_use]
    pub fn theme_mode(&self) -> AppThemeMode {
        self.editor.theme_mode
    }

    #[must_use]
    pub fn theme_preference(&self) -> egui::ThemePreference {
        if self.uses_system_editor_appearance() {
            crate::app::system_appearance::theme_preference()
        } else {
            self.editor.theme_mode.theme_preference()
        }
    }

    pub(crate) fn has_custom_editor_palette(&self) -> bool {
        !uses_stock_editor_palette(self)
    }

    #[must_use]
    pub fn uses_system_editor_appearance(&self) -> bool {
        self.editor_appearance_source() == EditorAppearanceSource::System
    }

    #[must_use]
    pub fn editor_text_color(&self) -> egui::Color32 {
        if self.uses_system_editor_appearance() {
            return crate::app::system_appearance::editor_palette().text;
        }

        color_from_hex(
            &self.editor.editor_text_color,
            color_from_hex(DEFAULT_EDITOR_TEXT_COLOR, egui::Color32::WHITE),
        )
    }

    #[must_use]
    pub fn editor_background_color(&self) -> egui::Color32 {
        if self.uses_system_editor_appearance() {
            return crate::app::system_appearance::editor_palette().background;
        }

        color_from_hex(
            &self.editor.editor_background_color,
            color_from_hex(DEFAULT_EDITOR_BACKGROUND_COLOR, egui::Color32::BLACK),
        )
    }

    #[must_use]
    pub fn editor_text_highlight_color(&self) -> egui::Color32 {
        if self.uses_system_editor_appearance() {
            return crate::app::system_appearance::editor_palette().highlight;
        }

        color_from_hex(
            &self.editor.editor_text_highlight_color,
            color_from_hex(
                DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR,
                egui::Color32::from_rgb(255, 243, 109),
            ),
        )
    }

    #[must_use]
    pub fn editor_text_highlight_text_color(&self) -> egui::Color32 {
        if self.uses_system_editor_appearance() {
            return crate::app::system_appearance::editor_palette().highlight_text;
        }

        let generated =
            crate::app::color_contrast::optimal_text_color(self.editor_text_highlight_color());
        if uses_generated_highlight_text_color(&self.editor.editor_text_highlight_text_color) {
            return generated;
        }

        color_from_hex(&self.editor.editor_text_highlight_text_color, generated)
    }

    #[must_use]
    pub fn word_wrap(&self) -> bool {
        self.editor.word_wrap
    }

    #[must_use]
    pub fn tab_list_position(&self) -> TabListPosition {
        self.workspace.tab_list_position
    }

    #[must_use]
    pub fn tab_order_mode(&self) -> TabOrderMode {
        self.workspace.tab_order_mode
    }

    #[must_use]
    pub fn tab_order_direction(&self) -> TabOrderDirection {
        self.workspace.tab_order_direction
    }

    #[must_use]
    pub fn file_open_disposition(&self) -> FileOpenDisposition {
        self.workspace.file_open_disposition
    }

    #[must_use]
    pub fn new_tab_placement(&self) -> NewTabPlacement {
        self.workspace.new_tab_placement
    }

    #[must_use]
    pub fn startup_session_behavior(&self) -> StartupSessionBehavior {
        self.workspace.startup_session_behavior
    }

    #[must_use]
    pub fn tab_list_width(&self) -> f32 {
        self.workspace.tab_list_width
    }

    #[must_use]
    pub fn auto_hide_tab_list(&self) -> bool {
        self.workspace.auto_hide_tab_list
    }

    #[must_use]
    pub fn tab_list_auto_hide_delay_seconds(&self) -> f32 {
        self.workspace.tab_list_auto_hide_delay_seconds
    }

    #[must_use]
    pub fn recent_files_enabled(&self) -> bool {
        self.workspace.recent_files_enabled
    }

    #[must_use]
    pub fn status_bar_visible(&self) -> bool {
        self.ui.status_bar_visible
    }

    #[must_use]
    pub fn platform_profile(&self) -> PlatformProfile {
        self.platform.profile
    }

    pub(crate) fn tab_list_auto_hide_delay(&self) -> Duration {
        Duration::from_secs_f32(sanitize_tab_list_auto_hide_delay_seconds(
            self.workspace.tab_list_auto_hide_delay_seconds,
        ))
    }
}

pub(crate) const VERTICAL_TAB_LIST_MIN_WIDTH: f32 = 96.0;
pub(crate) const VERTICAL_TAB_LIST_MAX_WIDTH: f32 = 360.0;
pub(crate) const TAB_LIST_AUTO_HIDE_DELAY_MIN_SECONDS: f32 = 0.0;
pub(crate) const TAB_LIST_AUTO_HIDE_DELAY_MAX_SECONDS: f32 = 10.0;

pub fn showing_settings(app: &ScratchpadApp) -> bool {
    app.state.chrome.showing_settings()
}

pub(crate) fn settings_tab_open(app: &ScratchpadApp) -> bool {
    app.state.app_settings.ui.settings_tab_open
}

pub(crate) fn vertical_tab_list_width(app: &ScratchpadApp) -> f32 {
    app.state.app_settings.workspace.tab_list_width.clamp(
        ScratchpadApp::VERTICAL_TAB_LIST_MIN_WIDTH,
        ScratchpadApp::VERTICAL_TAB_LIST_MAX_WIDTH,
    )
}

pub fn settings_path(app: &ScratchpadApp) -> &Path {
    app.state.persistence.settings_store.path()
}

pub(crate) fn is_settings_file_path(app: &ScratchpadApp, path: &Path) -> bool {
    paths_match(path, settings_path(app))
}

pub(crate) fn mark_active_buffer_as_settings_file(app: &mut ScratchpadApp) {
    let settings_path = settings_path(app).to_path_buf();
    let Some(tab) = app.tab_manager.active_tab_mut() else {
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
        app.tab_manager.mark_session_dirty();
    }
}

pub(super) fn load_settings_from_store(app: &mut ScratchpadApp) -> bool {
    match load_settings_snapshot(app) {
        Ok(Some(settings)) => {
            apply_settings(app, settings);
            true
        }
        Ok(None) => {
            apply_settings(app, AppSettings::default());
            false
        }
        Err(error) => {
            apply_settings(app, AppSettings::default());
            app.state.status.report_settings_load_failed(error);
            false
        }
    }
}

fn load_settings_snapshot(app: &ScratchpadApp) -> std::io::Result<Option<AppSettings>> {
    app.state.persistence.settings_store.load()
}

pub(super) fn apply_settings(app: &mut ScratchpadApp, settings: AppSettings) {
    let mut settings = settings;
    let invalid_shortcuts =
        crate::app::shortcut_keymap::invalid_shortcut_overrides(&settings.shortcuts);
    sync_stock_editor_palette_with_theme_mode(&mut settings);
    settings.history.budget = settings.history.budget.sanitized();
    settings
        .workspace
        .recently_closed_files
        .truncate(crate::app::app_state::RECENTLY_CLOSED_FILE_LIMIT);
    settings.workspace.tab_list_auto_hide_delay_seconds = sanitize_tab_list_auto_hide_delay_seconds(
        settings.workspace.tab_list_auto_hide_delay_seconds,
    );
    if !settings.ui.settings_tab_open && app.state.chrome.showing_settings() {
        app.state.chrome.activate_workspace_surface();
    }
    app.state.settings_tab_index = settings.ui.settings_tab_index.unwrap_or(usize::MAX);
    app.state.recently_closed_files = settings
        .workspace
        .recently_closed_files
        .iter()
        .take(crate::app::app_state::RECENTLY_CLOSED_FILE_LIMIT)
        .cloned()
        .collect();
    app.state.app_settings = settings;
    app.state.window.applied_editor_font = None;
    app.state.window.applied_theme_mode = None;
    app.apply_history_budget_to_open_buffers();
    if !invalid_shortcuts.is_empty() {
        app.state
            .status
            .report_invalid_shortcut_overrides(&invalid_shortcuts);
    }
}

fn refresh_settings_snapshot(app: &mut ScratchpadApp) {
    app.state.app_settings.ui.settings_tab_open = settings_tab_open(app);
    app.state.app_settings.ui.settings_tab_index = (app.state.settings_tab_index != usize::MAX)
        .then_some(
            app.state
                .settings_tab_index
                .min(app.tab_manager.tabs.as_slice().len()),
        );
}

pub(crate) fn persist_settings_now(app: &mut ScratchpadApp) -> std::io::Result<()> {
    refresh_settings_snapshot(app);
    app.state
        .persistence
        .settings_store
        .save(&app.state.app_settings)
}

pub fn apply_theme_to_context(app: &mut ScratchpadApp, ctx: &egui::Context) {
    crate::app::system_appearance::observe_system_theme(ctx.system_theme());
    let theme_mode = app.state.app_settings.editor.theme_mode;
    if app.state.window.applied_theme_mode == Some(theme_mode) {
        return;
    }

    ctx.set_theme(app.state.app_settings.theme_preference());
    ctx.set_visuals_of(egui::Theme::Dark, egui::Visuals::dark());
    ctx.set_visuals_of(egui::Theme::Light, egui::Visuals::light());
    app.state.window.applied_theme_mode = Some(theme_mode);
}

pub(super) fn sync_stock_editor_palette_with_theme_mode(settings: &mut AppSettings) {
    let Some((text_color, background_color)) = stock_editor_palette(settings.editor.theme_mode)
    else {
        return;
    };

    if !uses_stock_editor_palette(settings) {
        return;
    }

    if settings.editor.editor_text_color == text_color
        && settings.editor.editor_background_color == background_color
    {
        return;
    }

    text_color.clone_into(&mut settings.editor.editor_text_color);
    background_color.clone_into(&mut settings.editor.editor_background_color);
}

pub(super) fn uses_stock_editor_palette(settings: &AppSettings) -> bool {
    matches!(
        (
            settings.editor.editor_text_color.as_str(),
            settings.editor.editor_background_color.as_str()
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

#[cfg(test)]
mod tests {
    use super::apply_settings;
    use crate::app::app_state::{ScratchpadApp, StatusDomain, StatusSeverity, StatusState};
    use crate::app::fonts::{EditorFontPreset, EditorFontSource};
    use crate::app::services::settings_store::{AppSettings, EditorAppearanceSource};
    use std::collections::BTreeMap;

    #[test]
    fn app_appearance_honors_os_font_selection() {
        let mut settings = AppSettings::default();
        settings.editor.appearance_source = EditorAppearanceSource::App;
        settings.editor.editor_font = EditorFontPreset::Mono;
        settings.editor.font_source = EditorFontSource::Os;
        settings.editor.os_font_family = "System Mono".to_owned();

        let selection = settings.editor_font_selection();

        assert_eq!(selection.source, EditorFontSource::Os);
        assert_eq!(selection.scratchpad_preset, EditorFontPreset::Mono);
        assert_eq!(selection.os_family.as_deref(), Some("System Mono"));
        assert_eq!(selection.label(), "System Mono");
    }

    #[test]
    fn applying_settings_reports_invalid_shortcut_overrides() {
        let mut app = ScratchpadApp::default();
        app.state.status = StatusState::default();
        let mut settings = AppSettings::default();
        settings.shortcuts.bindings =
            BTreeMap::from([("open_file".to_owned(), "ctrl+bogus".to_owned())]);

        apply_settings(&mut app, settings);

        let status = app.state.status.current.as_ref().expect("warning status");
        assert_eq!(status.severity, StatusSeverity::Warning);
        assert_eq!(status.domain, StatusDomain::Settings);
        assert_eq!(
            status.text,
            "A shortcut override was ignored; using the default binding."
        );
        assert_eq!(status.detail.as_deref(), Some("open_file = \"ctrl+bogus\""));
    }
}
