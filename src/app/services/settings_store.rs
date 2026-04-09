use crate::app::fonts::EditorFontPreset;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const SETTINGS_FILE_NAME: &str = "settings.yaml";
pub const DEFAULT_FONT_SIZE: f32 = 14.0;
pub const DEFAULT_WORD_WRAP: bool = true;
pub const DEFAULT_LOGGING_ENABLED: bool = true;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppSettings {
    pub font_size: f32,
    pub word_wrap: bool,
    pub logging_enabled: bool,
    #[serde(default)]
    pub editor_font: EditorFontPreset,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_size: DEFAULT_FONT_SIZE,
            word_wrap: DEFAULT_WORD_WRAP,
            logging_enabled: DEFAULT_LOGGING_ENABLED,
            editor_font: EditorFontPreset::default(),
        }
    }
}

pub struct SettingsStore {
    root: PathBuf,
    settings_path: PathBuf,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new(std::env::temp_dir().join("scratchpad"))
    }
}

impl SettingsStore {
    pub fn new(root: PathBuf) -> Self {
        let settings_path = root.join(SETTINGS_FILE_NAME);
        Self {
            root,
            settings_path,
        }
    }

    pub fn load(&self) -> io::Result<Option<AppSettings>> {
        if !self.settings_path.exists() {
            return Ok(None);
        }

        let raw = fs::read_to_string(&self.settings_path)?;
        let settings = serde_yaml::from_str(&raw).map_err(invalid_data)?;
        Ok(Some(settings))
    }

    pub fn save(&self, settings: &AppSettings) -> io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let yaml = serde_yaml::to_string(settings).map_err(invalid_data)?;
        write_atomic(&self.settings_path, yaml.as_bytes())
    }

    pub fn path(&self) -> &Path {
        &self.settings_path
    }
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
    use super::{AppSettings, SettingsStore};
    use crate::app::fonts::EditorFontPreset;
    use std::fs;

    #[test]
    fn missing_settings_file_returns_none() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());

        assert_eq!(store.load().expect("load settings"), None);
    }

    #[test]
    fn save_and_load_round_trip_yaml_settings() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());
        let settings = AppSettings {
            font_size: 18.0,
            word_wrap: false,
            logging_enabled: false,
            editor_font: EditorFontPreset::Roboto,
        };

        store.save(&settings).expect("save settings");
        let loaded = store.load().expect("load settings");

        assert_eq!(loaded, Some(settings));
    }

    #[test]
    fn malformed_yaml_returns_invalid_data_error() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());
        fs::write(store.path(), "font_size: [oops").expect("write invalid yaml");

        let error = store.load().expect_err("load should fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn missing_editor_font_field_defaults_for_older_yaml() {
        let root = tempfile::tempdir().expect("create settings dir");
        let store = SettingsStore::new(root.path().to_path_buf());
        fs::write(
            store.path(),
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
                editor_font: EditorFontPreset::SystemDefault,
            })
        );
    }
}
