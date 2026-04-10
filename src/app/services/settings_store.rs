use crate::app::fonts::EditorFontPreset;
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
    toml::from_str(raw).map_err(invalid_data)
}

fn default_editor_gutter() -> u8 {
    DEFAULT_EDITOR_GUTTER
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
    use super::{AppSettings, DEFAULT_EDITOR_GUTTER, SettingsStore};
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
            })
        );
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
            })
        );
        assert!(store.path().exists());
        assert!(legacy_path.exists());
    }
}
