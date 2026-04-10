use super::{AppSurface, ScratchpadApp};
use crate::app::fonts::EditorFontPreset;
use crate::app::logging::{self, LogLevel};
use crate::app::paths_match;
use crate::app::services::file_controller::FileController;
use crate::app::services::settings_store::AppSettings;
use std::path::{Path, PathBuf};

mod display_tabs;
mod toml_refresh;

impl ScratchpadApp {
    pub fn font_size(&self) -> f32 {
        self.app_settings.font_size
    }

    pub fn editor_font(&self) -> EditorFontPreset {
        self.app_settings.editor_font
    }

    pub fn editor_gutter(&self) -> u8 {
        self.app_settings.editor_gutter
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
}
