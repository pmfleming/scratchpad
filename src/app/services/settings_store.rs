use crate::app::fonts::EditorFontPreset;
use crate::app::services::store_io::write_atomic;
use eframe::egui;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SETTINGS_FILE_NAME: &str = "settings.toml";
const LEGACY_SETTINGS_FILE_NAME: &str = "settings.yaml";
pub const DEFAULT_FONT_SIZE: f32 = 14.0;
pub const DEFAULT_WORD_WRAP: bool = true;
pub const DEFAULT_LOGGING_ENABLED: bool = true;
pub const DEFAULT_EDITOR_GUTTER: u8 = 0;
pub const DEFAULT_EDITOR_TEXT_COLOR: &str = "#ffffff";
pub const DEFAULT_EDITOR_BACKGROUND_COLOR: &str = "#15181d";
pub const LIGHT_EDITOR_TEXT_COLOR: &str = "#000000";
pub const LIGHT_EDITOR_BACKGROUND_COLOR: &str = "#ffffff";
pub const DEFAULT_TAB_LIST_WIDTH: f32 = 184.0;
pub const DEFAULT_AUTO_HIDE_TAB_LIST: bool = false;
pub const DEFAULT_TAB_LIST_AUTO_HIDE_DELAY_SECONDS: f32 = 3.0;
pub const DEFAULT_RECENT_FILES_ENABLED: bool = true;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileOpenDisposition {
    #[default]
    NewTab,
    CurrentTab,
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
    pub fn is_vertical(self) -> bool {
        matches!(self, Self::Left | Self::Right)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub font_size: f32,
    pub word_wrap: bool,
    pub logging_enabled: bool,
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
    #[serde(default)]
    pub tab_list_position: TabListPosition,
    #[serde(default)]
    pub file_open_disposition: FileOpenDisposition,
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
    pub settings_tab_open: bool,
    #[serde(default)]
    pub settings_tab_index: Option<usize>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_size: default_font_size(),
            word_wrap: default_word_wrap(),
            logging_enabled: default_logging_enabled(),
            editor_gutter: default_editor_gutter(),
            editor_font: EditorFontPreset::default(),
            theme_mode: AppThemeMode::default(),
            editor_text_color: default_editor_text_color(),
            editor_background_color: default_editor_background_color(),
            tab_list_position: TabListPosition::default(),
            file_open_disposition: FileOpenDisposition::default(),
            startup_session_behavior: StartupSessionBehavior::default(),
            tab_list_width: default_tab_list_width(),
            auto_hide_tab_list: default_auto_hide_tab_list(),
            tab_list_auto_hide_delay_seconds: default_tab_list_auto_hide_delay_seconds(),
            recent_files_enabled: default_recent_files_enabled(),
            settings_tab_open: false,
            settings_tab_index: None,
        }
    }
}

pub struct SettingsStore {
    root: PathBuf,
    settings_path: PathBuf,
    legacy_settings_path: PathBuf,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new(std::env::temp_dir().join("scratchpad"))
    }
}

impl SettingsStore {
    pub fn new(root: PathBuf) -> Self {
        let settings_path = root.join(SETTINGS_FILE_NAME);
        let legacy_settings_path = root.join(LEGACY_SETTINGS_FILE_NAME);
        Self {
            root,
            settings_path,
            legacy_settings_path,
        }
    }

    pub fn load(&self) -> io::Result<Option<AppSettings>> {
        if self.settings_path.exists() {
            return self.load_toml();
        }

        if self.legacy_settings_path.exists() {
            let settings = self.load_legacy_yaml()?;
            self.save(&settings)?;
            return Ok(Some(settings));
        }

        Ok(None)
    }

    fn load_toml(&self) -> io::Result<Option<AppSettings>> {
        let raw = fs::read_to_string(&self.settings_path)?;
        let settings = parse_toml_settings(&raw)?;
        Ok(Some(settings))
    }

    fn load_legacy_yaml(&self) -> io::Result<AppSettings> {
        let raw = fs::read_to_string(&self.legacy_settings_path)?;
        serde_yaml::from_str(&raw).map_err(invalid_data)
    }

    pub fn save(&self, settings: &AppSettings) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let toml = toml::to_string_pretty(settings).map_err(invalid_data)?;
        write_atomic(&self.settings_path, toml.as_bytes())
    }

    pub fn path(&self) -> &Path {
        &self.settings_path
    }
}

pub(crate) fn parse_toml_settings(raw: &str) -> io::Result<AppSettings> {
    let mut value = raw.parse::<toml::Value>().map_err(invalid_data)?;
    let Some(table) = value.as_table_mut() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "settings TOML must contain a top-level table",
        ));
    };

    migrate_auto_hide_fields(table);

    value.try_into().map_err(invalid_data)
}

fn migrate_auto_hide_fields(table: &mut toml::map::Map<String, toml::Value>) {
    if !table.contains_key("auto_hide_tab_list") {
        let auto_hide = ["auto_hide_side_bars", "auto_hide_top_bars"]
            .into_iter()
            .find_map(|key| table.get(key).and_then(toml::Value::as_bool))
            .unwrap_or(DEFAULT_AUTO_HIDE_TAB_LIST);
        table.insert(
            "auto_hide_tab_list".to_owned(),
            toml::Value::Boolean(auto_hide),
        );
    }

    table.remove("auto_hide_side_bars");
    table.remove("auto_hide_top_bars");
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
default_fn!(default_logging_enabled, bool, DEFAULT_LOGGING_ENABLED);
default_fn!(default_editor_gutter, u8, DEFAULT_EDITOR_GUTTER);

pub(crate) fn default_editor_text_color() -> String {
    DEFAULT_EDITOR_TEXT_COLOR.to_owned()
}

pub(crate) fn default_editor_background_color() -> String {
    DEFAULT_EDITOR_BACKGROUND_COLOR.to_owned()
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

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests;
