use crate::app::fonts::EditorFontPreset;
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

impl AppThemeMode {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "Use system setting",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

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
    pub const ALL: [Self; 4] = [Self::Top, Self::Bottom, Self::Left, Self::Right];

    pub fn label(self) -> &'static str {
        match self {
            Self::Top => "Top",
            Self::Bottom => "Bottom",
            Self::Left => "Left",
            Self::Right => "Right",
        }
    }

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
    #[serde(default = "default_tab_list_width")]
    pub tab_list_width: f32,
    #[serde(default = "default_auto_hide_tab_list")]
    pub auto_hide_tab_list: bool,
    #[serde(default)]
    pub settings_tab_open: bool,
    #[serde(default)]
    pub settings_tab_index: Option<usize>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_FONT_SIZE,
            word_wrap: DEFAULT_WORD_WRAP,
            logging_enabled: DEFAULT_LOGGING_ENABLED,
            editor_gutter: DEFAULT_EDITOR_GUTTER,
            editor_font: EditorFontPreset::default(),
            theme_mode: AppThemeMode::default(),
            editor_text_color: DEFAULT_EDITOR_TEXT_COLOR.to_owned(),
            editor_background_color: DEFAULT_EDITOR_BACKGROUND_COLOR.to_owned(),
            tab_list_position: TabListPosition::default(),
            tab_list_width: DEFAULT_TAB_LIST_WIDTH,
            auto_hide_tab_list: DEFAULT_AUTO_HIDE_TAB_LIST,
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

    if !table.contains_key("auto_hide_tab_list") {
        let auto_hide = table
            .get("auto_hide_side_bars")
            .and_then(toml::Value::as_bool)
            .or_else(|| table.get("auto_hide_top_bars").and_then(toml::Value::as_bool))
            .unwrap_or(DEFAULT_AUTO_HIDE_TAB_LIST);
        table.insert("auto_hide_tab_list".to_owned(), toml::Value::Boolean(auto_hide));
    }

    table.remove("auto_hide_side_bars");
    table.remove("auto_hide_top_bars");

    value.try_into().map_err(invalid_data)
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

fn default_editor_gutter() -> u8 {
    DEFAULT_EDITOR_GUTTER
}

fn default_editor_text_color() -> String {
    DEFAULT_EDITOR_TEXT_COLOR.to_owned()
}

fn default_editor_background_color() -> String {
    DEFAULT_EDITOR_BACKGROUND_COLOR.to_owned()
}

fn default_tab_list_width() -> f32 {
    DEFAULT_TAB_LIST_WIDTH
}

fn default_auto_hide_tab_list() -> bool {
    DEFAULT_AUTO_HIDE_TAB_LIST
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp_path = path.with_extension(format!(
        "{}.write",
        path.extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("tmp")
    ));
    fs::write(&temp_path, bytes)?;

    if path.exists() {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }

    fs::rename(temp_path, path)
}

fn invalid_data(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

#[cfg(test)]
mod tests {
    use super::{
        AppSettings, AppThemeMode, DEFAULT_EDITOR_GUTTER, DEFAULT_TAB_LIST_WIDTH, SettingsStore,
        TabListPosition,
    };
    use crate::app::fonts::EditorFontPreset;
    use std::fs;

    #[test]
    fn missing_settings_file_returns_none() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());

        assert_eq!(store.load().expect("load settings"), None);
    }

    #[test]
    fn save_and_load_round_trip_toml_settings() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());
        let settings = AppSettings {
            font_size: 18.0,
            word_wrap: false,
            logging_enabled: false,
            editor_gutter: 6,
            editor_font: EditorFontPreset::Roboto,
            theme_mode: AppThemeMode::Light,
            editor_text_color: "#111111".to_owned(),
            editor_background_color: "#eeeeee".to_owned(),
            tab_list_position: TabListPosition::Right,
            tab_list_width: 220.0,
            auto_hide_tab_list: true,
            settings_tab_open: true,
            settings_tab_index: Some(2),
        };

        store.save(&settings).expect("save settings");
        let loaded = store.load().expect("load settings");

        assert_eq!(loaded, Some(settings));
        assert_eq!(
            store.path().file_name().and_then(|name| name.to_str()),
            Some("settings.toml")
        );
    }

    #[test]
    fn malformed_toml_returns_invalid_data_error() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());
        fs::write(store.path(), "font_size = [oops").expect("write invalid toml");

        let error = store.load().expect_err("load should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn missing_editor_font_field_defaults_for_older_toml() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());
        fs::write(
            store.path(),
            "font_size = 16.0\nword_wrap = false\nlogging_enabled = true\n",
        )
        .expect("write legacy toml");

        let loaded = store.load().expect("load settings");
        assert_eq!(
            loaded,
            Some(AppSettings {
                font_size: 16.0,
                word_wrap: false,
                logging_enabled: true,
                editor_gutter: DEFAULT_EDITOR_GUTTER,
                editor_font: EditorFontPreset::SystemDefault,
                settings_tab_open: false,
                settings_tab_index: None,
                ..AppSettings::default()
            })
        );
    }

    #[test]
    fn missing_tab_list_position_defaults_for_older_toml() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());
        fs::write(
            store.path(),
            "font_size = 16.0\nword_wrap = false\nlogging_enabled = true\n",
        )
        .expect("write legacy toml");

        let loaded = store.load().expect("load settings").expect("settings");

        assert_eq!(loaded.tab_list_position, TabListPosition::Top);
        assert_eq!(loaded.tab_list_width, DEFAULT_TAB_LIST_WIDTH);
    }

    #[test]
    fn legacy_auto_hide_fields_migrate_to_single_tab_list_setting() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());
        fs::write(
            store.path(),
            "font_size = 16.0\nword_wrap = false\nlogging_enabled = true\nauto_hide_top_bars = true\n",
        )
        .expect("write legacy toml");

        let loaded = store.load().expect("load settings").expect("settings");

        assert!(loaded.auto_hide_tab_list);
    }

    #[test]
    fn legacy_yaml_migrates_to_toml_when_toml_is_missing() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());
        let legacy_path = root.path().join("settings.yaml");
        fs::write(
            &legacy_path,
            "font_size: 16.0\nword_wrap: false\nlogging_enabled: true\n",
        )
        .expect("write legacy yaml");

        let loaded = store.load().expect("load settings");

        assert_eq!(
            loaded,
            Some(AppSettings {
                font_size: 16.0,
                word_wrap: false,
                logging_enabled: true,
                editor_gutter: DEFAULT_EDITOR_GUTTER,
                editor_font: EditorFontPreset::SystemDefault,
                settings_tab_open: false,
                settings_tab_index: None,
                ..AppSettings::default()
            })
        );
        assert!(store.path().exists());
        assert!(legacy_path.exists());
    }
}
