use crate::app::domain::TextHistoryBudget;
use crate::app::fonts::EditorFontPreset;
use crate::app::platform::PlatformProfile;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const DEFAULT_FONT_SIZE: f32 = 14.0;
pub const DEFAULT_WORD_WRAP: bool = true;
pub const DEFAULT_EDITOR_GUTTER: u8 = 0;
pub const DEFAULT_EDITOR_TEXT_COLOR: &str = "#ffffff";
pub const DEFAULT_EDITOR_BACKGROUND_COLOR: &str = "#15181d";
pub const DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR: &str = "#fff36d";
pub const DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR: &str = "#0b0f3d";
pub const LEGACY_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR: &str = "#000000";
pub const LIGHT_EDITOR_TEXT_COLOR: &str = "#000000";
pub const LIGHT_EDITOR_BACKGROUND_COLOR: &str = "#ffffff";
pub const DEFAULT_TAB_LIST_WIDTH: f32 = 184.0;
pub const DEFAULT_AUTO_HIDE_TAB_LIST: bool = false;
pub const DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS: f32 = 3.0;
pub const DEFAULT_RECENT_FILES_ENABLED: bool = true;
pub const DEFAULT_STATUS_BAR_VISIBLE: bool = true;
pub const DEFAULT_WINDOW_INNER_SIZE: [f32; 2] = [960.0, 640.0];
pub const MIN_WINDOW_INNER_SIZE: [f32; 2] = [400.0, 300.0];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOpenDisposition {
    #[default]
    NewTab,
    CurrentTab,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewTabPlacement {
    Start,
    #[default]
    End,
    BeforeSelection,
    AfterSelection,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupSessionBehavior {
    #[default]
    ContinuePreviousSession,
    StartFreshSession,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl AppThemeMode {
    #[must_use]
    pub fn theme_preference(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Light => egui::ThemePreference::Light,
            Self::Dark => egui::ThemePreference::Dark,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabListPosition {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl TabListPosition {
    #[must_use]
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabOrderMode {
    #[default]
    Custom,
    FileName,
    FileSize,
    FileAge,
    RecentEdit,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TabOrderDirection {
    #[default]
    Ascending,
    Descending,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct WindowState {
    #[serde(skip)]
    pub position: Option<[f32; 2]>,
    #[serde(skip)]
    pub inner_size: Option<[f32; 2]>,
    #[serde(default)]
    pub maximized: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EditorSettings {
    #[serde(default = "default_font_size")]
    pub font_size: f32,
    #[serde(default = "default_word_wrap")]
    pub word_wrap: bool,
    #[serde(default = "default_editor_gutter")]
    pub editor_gutter: u8,
    #[serde(default)]
    pub editor_font: EditorFontPreset,
    #[serde(default)]
    pub theme_mode: AppThemeMode,
    #[serde(default = "default_editor_text_color")]
    pub editor_text_color: String,
    #[serde(default = "default_editor_background_color")]
    pub editor_background_color: String,
    #[serde(default = "default_editor_text_highlight_color")]
    pub editor_text_highlight_color: String,
    #[serde(default = "default_editor_text_highlight_text_color")]
    pub editor_text_highlight_text_color: String,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            font_size: default_font_size(),
            word_wrap: default_word_wrap(),
            editor_gutter: default_editor_gutter(),
            editor_font: EditorFontPreset::default(),
            theme_mode: AppThemeMode::default(),
            editor_text_color: default_editor_text_color(),
            editor_background_color: default_editor_background_color(),
            editor_text_highlight_color: default_editor_text_highlight_color(),
            editor_text_highlight_text_color: default_editor_text_highlight_text_color(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceSettings {
    #[serde(default)]
    pub tab_list_position: TabListPosition,
    #[serde(default)]
    pub tab_order_mode: TabOrderMode,
    #[serde(default)]
    pub tab_order_direction: TabOrderDirection,
    #[serde(skip)]
    pub custom_tab_order: Vec<u64>,
    #[serde(default)]
    pub file_open_disposition: FileOpenDisposition,
    #[serde(default)]
    pub new_tab_placement: NewTabPlacement,
    #[serde(default)]
    pub startup_session_behavior: StartupSessionBehavior,
    #[serde(default = "default_tab_list_width")]
    pub tab_list_width: f32,
    #[serde(default = "default_auto_hide_tab_list")]
    pub auto_hide_tab_list: bool,
    #[serde(default = "default_tab_list_auto_hide_delay_seconds")]
    pub tab_list_auto_hide_delay_seconds: f32,
    #[serde(default = "default_recent_files_enabled")]
    pub recent_files_enabled: bool,
    #[serde(default)]
    pub recently_closed_files: Vec<PathBuf>,
}

impl Default for WorkspaceSettings {
    fn default() -> Self {
        Self {
            tab_list_position: TabListPosition::default(),
            tab_order_mode: TabOrderMode::default(),
            tab_order_direction: TabOrderDirection::default(),
            custom_tab_order: Vec::new(),
            file_open_disposition: FileOpenDisposition::default(),
            new_tab_placement: NewTabPlacement::default(),
            startup_session_behavior: StartupSessionBehavior::default(),
            tab_list_width: default_tab_list_width(),
            auto_hide_tab_list: default_auto_hide_tab_list(),
            tab_list_auto_hide_delay_seconds: default_tab_list_auto_hide_delay_seconds(),
            recent_files_enabled: default_recent_files_enabled(),
            recently_closed_files: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UiSettings {
    #[serde(default = "default_status_bar_visible")]
    pub status_bar_visible: bool,
    #[serde(default)]
    pub window_state: WindowState,
    #[serde(default = "default_settings_tab_open")]
    pub settings_tab_open: bool,
    #[serde(default)]
    pub settings_tab_index: Option<usize>,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            status_bar_visible: default_status_bar_visible(),
            window_state: WindowState::default(),
            settings_tab_open: default_settings_tab_open(),
            settings_tab_index: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HistorySettings {
    #[serde(default, flatten)]
    pub budget: TextHistoryBudget,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlatformSettings {
    #[serde(default)]
    pub profile: PlatformProfile,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ShortcutSettings {
    #[serde(default, flatten)]
    pub bindings: BTreeMap<String, String>,
}

impl ShortcutSettings {
    #[must_use]
    pub fn binding(&self, action_key: &str) -> Option<&str> {
        self.bindings.get(action_key).map(String::as_str)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default)]
    pub editor: EditorSettings,
    #[serde(default)]
    pub workspace: WorkspaceSettings,
    #[serde(default)]
    pub ui: UiSettings,
    #[serde(default)]
    pub history: HistorySettings,
    #[serde(default)]
    pub platform: PlatformSettings,
    #[serde(default)]
    pub shortcuts: ShortcutSettings,
}

pub(crate) fn color_from_hex(hex: &str, fallback: egui::Color32) -> egui::Color32 {
    parse_hex_color(hex).unwrap_or(fallback)
}

pub(crate) fn color_to_hex(color: egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

fn parse_hex_color(hex: &str) -> Option<egui::Color32> {
    let trimmed = hex.trim().trim_start_matches('#');
    if trimmed.len() != 6 {
        return None;
    }

    let r = u8::from_str_radix(&trimmed[0..2], 16).ok()?;
    let g = u8::from_str_radix(&trimmed[2..4], 16).ok()?;
    let b = u8::from_str_radix(&trimmed[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(r, g, b))
}

macro_rules! default_fn {
    ($name:ident, $type:ty, $val:expr) => {
        pub(crate) const fn $name() -> $type {
            $val
        }
    };
}

default_fn!(default_font_size, f32, DEFAULT_FONT_SIZE);
default_fn!(default_word_wrap, bool, DEFAULT_WORD_WRAP);
default_fn!(default_editor_gutter, u8, DEFAULT_EDITOR_GUTTER);

pub(crate) fn default_editor_text_color() -> String {
    DEFAULT_EDITOR_TEXT_COLOR.to_owned()
}

pub(crate) fn default_editor_background_color() -> String {
    DEFAULT_EDITOR_BACKGROUND_COLOR.to_owned()
}

pub(crate) fn default_editor_text_highlight_color() -> String {
    DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR.to_owned()
}

pub(crate) fn default_editor_text_highlight_text_color() -> String {
    color_to_hex(crate::app::color_contrast::optimal_text_color(
        color_from_hex(
            DEFAULT_EDITOR_TEXT_HIGHLIGHT_COLOR,
            egui::Color32::from_rgb(255, 243, 109),
        ),
    ))
}

default_fn!(default_tab_list_width, f32, DEFAULT_TAB_LIST_WIDTH);
default_fn!(default_auto_hide_tab_list, bool, DEFAULT_AUTO_HIDE_TAB_LIST);
default_fn!(
    default_tab_list_auto_hide_delay_seconds,
    f32,
    DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS
);
default_fn!(
    default_recent_files_enabled,
    bool,
    DEFAULT_RECENT_FILES_ENABLED
);
default_fn!(default_status_bar_visible, bool, DEFAULT_STATUS_BAR_VISIBLE);
default_fn!(default_settings_tab_open, bool, true);

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR, default_editor_text_highlight_text_color,
    };

    #[test]
    fn highlight_text_default_matches_generated_contrast_color() {
        assert_eq!(
            default_editor_text_highlight_text_color(),
            DEFAULT_EDITOR_TEXT_HIGHLIGHT_TEXT_COLOR
        );
    }
}
